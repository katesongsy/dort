mod support;
use dort_core::{
    Error,
    context::HostContext,
    local::{LocalConfig, RepositoryRegistry, repository_state_path},
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

#[test]
fn readonly_load_does_not_create_state() {
    let f = support::TestHome::new();
    let config = LocalConfig::load(&f.host).unwrap();
    assert!(config.repositories.is_empty());
    assert!(!f.host.config_dir.exists());
    assert!(!f.host.state_dir.exists());
}

#[test]
fn xdg_layout_is_consistent_for_macos_and_ubuntu_and_rejects_relative_roots() {
    for platform in [Platform::Macos, Platform::Ubuntu] {
        let h = HostContext::new(
            "/home/example".into(),
            Some("/config".into()),
            Some("/state".into()),
            platform,
            Architecture::Aarch64,
        )
        .unwrap();
        assert_eq!(h.config_dir, std::path::Path::new("/config/dort"));
        assert_eq!(h.state_dir, std::path::Path::new("/state/dort"));
    }
    assert!(
        HostContext::new(
            "relative".into(),
            None,
            None,
            Platform::Ubuntu,
            Architecture::X86_64
        )
        .is_err()
    );
    assert!(
        HostContext::new(
            "/home/example".into(),
            Some("relative".into()),
            None,
            Platform::Ubuntu,
            Architecture::X86_64
        )
        .is_err()
    );
    assert!(
        HostContext::new(
            "/home/example".into(),
            Some("/same".into()),
            Some("/same".into()),
            Platform::Ubuntu,
            Architecture::X86_64
        )
        .is_err()
    );
}

#[test]
fn writer_lock_is_exclusive_persistent_and_released_on_drop() {
    let f = support::TestHome::new();
    let lock = WriterLock::acquire(&f.host).unwrap();
    assert!(matches!(WriterLock::acquire(&f.host), Err(Error::Locked)));
    let path = f.host.state_dir.join("lock");
    assert!(path.is_file());
    drop(lock);
    assert!(path.is_file());
    WriterLock::acquire(&f.host).unwrap();
    assert_eq!(
        fs::metadata(&f.host.state_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn repository_registration_is_idempotent_scoped_and_selectable() {
    let f = support::TestHome::new();
    let one = f.repository("one");
    let two = f.repository("two");
    let lock = WriterLock::acquire(&f.host).unwrap();
    let first = RepositoryRegistry::register(&f.host, &lock, &one).unwrap();
    assert_eq!(
        RepositoryRegistry::register(&f.host, &lock, &one.join(".")).unwrap(),
        first
    );
    let second = RepositoryRegistry::register(&f.host, &lock, &two).unwrap();
    assert_ne!(first, second);
    let local = LocalConfig::load(&f.host).unwrap();
    assert_eq!(local.repositories.len(), 2);
    assert_eq!(local.select_repository(None).unwrap(), two);
    assert_eq!(local.select_repository(Some(&one)).unwrap(), one);
    assert_ne!(
        repository_state_path(&f.host, &first).unwrap(),
        repository_state_path(&f.host, &second).unwrap()
    );
    assert!(repository_state_path(&f.host, "../escape").is_err());
    assert_eq!(
        fs::metadata(f.host.local_config_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn deliberate_relocation_preserves_id_but_refuses_existing_old_root() {
    let f = support::TestHome::new();
    let old = f.repository("old");
    let moved = f.sandbox.path().join("moved");
    let lock = WriterLock::acquire(&f.host).unwrap();
    let id = RepositoryRegistry::register(&f.host, &lock, &old).unwrap();
    assert!(RepositoryRegistry::relocate(&f.host, &lock, &id, &old).is_err());
    fs::rename(&old, &moved).unwrap();
    RepositoryRegistry::relocate(&f.host, &lock, &id, &moved).unwrap();
    assert_eq!(
        LocalConfig::load(&f.host)
            .unwrap()
            .select_repository(None)
            .unwrap(),
        moved.canonicalize().unwrap()
    );
}

#[test]
fn lock_for_another_context_cannot_authorise_registration() {
    let one = support::TestHome::new();
    let two = support::TestHome::new();
    let lock = WriterLock::acquire(&one.host).unwrap();
    let repo = two.repository("repo");
    assert!(RepositoryRegistry::register(&two.host, &lock, &repo).is_err());
    assert!(!two.host.config_dir.exists());
}

#[test]
fn symlinked_state_parents_and_config_files_are_not_followed() {
    let f = support::TestHome::new();
    let foreign = f.sandbox.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::create_dir_all(f.host.state_dir.parent().unwrap()).unwrap();
    symlink(&foreign, &f.host.state_dir).unwrap();
    assert!(WriterLock::acquire(&f.host).is_err());
    assert!(!foreign.join("lock").exists());

    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    let file = foreign.join("local.toml");
    fs::write(&file, "schema=1").unwrap();
    symlink(&file, f.host.local_config_path()).unwrap();
    assert!(LocalConfig::load(&f.host).is_err());
}

#[test]
fn insecure_and_hardlinked_state_files_are_rejected() {
    let f = support::TestHome::new();
    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(LocalConfig::load(&f.host).is_err());
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.host.local_config_path(), "schema=1").unwrap();
    fs::set_permissions(
        f.host.local_config_path(),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(LocalConfig::load(&f.host).is_err());
    fs::set_permissions(
        f.host.local_config_path(),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::hard_link(f.host.local_config_path(), f.host.config_dir.join("alias")).unwrap();
    assert!(LocalConfig::load(&f.host).is_err());
}

#[test]
fn local_configuration_errors_redact_values() {
    let f = support::TestHome::new();
    let _lock = WriterLock::acquire(&f.host).unwrap();
    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.host.local_config_path(), "schema = 'SENSITIVE-SENTINEL'").unwrap();
    fs::set_permissions(
        f.host.local_config_path(),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let error = LocalConfig::load(&f.host).err().unwrap().to_string();
    assert!(!error.contains("SENSITIVE-SENTINEL"));
}

#[test]
fn platform_detection_requires_ubuntu_not_ubuntu_like() {
    assert_eq!(
        Platform::from_os_release("NAME=Ubuntu\nID=\"ubuntu\"\n").unwrap(),
        Platform::Ubuntu
    );
    assert!(Platform::from_os_release("ID=debian\nID_LIKE=ubuntu").is_err());
}

#[test]
fn managed_targets_must_not_escape_home_or_overlap_state() {
    let f = support::TestHome::new();
    assert!(
        f.host
            .target_path(std::path::Path::new("~/.zshrc"), None)
            .is_ok()
    );
    for target in [
        "~/../escape",
        "/etc/profile",
        "~",
        "~/.config/dort/local.toml",
        "~/.local/state/dort/lock",
        "~/.config",
    ] {
        assert!(
            f.host
                .target_path(std::path::Path::new(target), None)
                .is_err(),
            "{target}"
        );
    }
}
