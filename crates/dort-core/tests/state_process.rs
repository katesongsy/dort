mod support;
use dort_core::{
    Error,
    context::HostContext,
    local::{LocalConfig, RepositoryRegistry},
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};

// This helper runs in an isolated test subprocess; no parent environment is changed.
#[test]
fn child_lock_probe() {
    let Some(home) = std::env::var_os("DORT_TEST_LOCK_HOME") else {
        return;
    };
    let host = HostContext::new(
        home.into(),
        None,
        None,
        Platform::Ubuntu,
        Architecture::X86_64,
    )
    .unwrap();
    assert!(matches!(WriterLock::acquire(&host), Err(Error::Locked)));
}

#[test]
fn separate_process_cannot_acquire_a_held_writer_lock() {
    let f = support::TestHome::new();
    let _lock = WriterLock::acquire(&f.host).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_lock_probe", "--nocapture"])
        .env("DORT_TEST_LOCK_HOME", &f.host.home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn registration_preserves_local_profiles_and_variables() {
    let f = support::TestHome::new();
    let lock = WriterLock::acquire(&f.host).unwrap();
    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        f.host.local_config_path(),
        "active_profiles=['dev']\n[variables]\ngit_name='Example'\n",
    )
    .unwrap();
    fs::set_permissions(
        f.host.local_config_path(),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let repository = f.repository("repository");
    RepositoryRegistry::register(&f.host, &lock, &repository).unwrap();
    let local = LocalConfig::load(&f.host).unwrap();
    assert_eq!(local.active_profiles, ["dev"]);
    assert_eq!(local.variables["git_name"].as_str(), Some("Example"));
}

#[test]
fn state_ancestor_symlinks_do_not_create_foreign_directories() {
    let f = support::TestHome::new();
    let foreign = f.sandbox.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    symlink(&foreign, f.host.home.join(".local")).unwrap();
    assert!(WriterLock::acquire(&f.host).is_err());
    assert_eq!(fs::read_dir(&foreign).unwrap().count(), 0);
}

#[test]
fn invalid_registry_version_is_not_overwritten_by_registration() {
    let f = support::TestHome::new();
    let lock = WriterLock::acquire(&f.host).unwrap();
    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.host.local_config_path(), "schema=99\n").unwrap();
    fs::set_permissions(
        f.host.local_config_path(),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let repository = f.repository("repository");
    assert!(RepositoryRegistry::register(&f.host, &lock, &repository).is_err());
    assert_eq!(
        fs::read_to_string(f.host.local_config_path()).unwrap(),
        "schema=99\n"
    );
}
