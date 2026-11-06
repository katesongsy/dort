mod support;
use dort_core::{
    discovery,
    discovery_ignore::{Preview, Rules},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};
use support::TestHome;

fn change(f: &TestHome, path: &str, ignored: bool) {
    Preview::prepare(&f.host, Path::new(path), ignored)
        .unwrap()
        .approve()
        .execute(&f.host, false)
        .unwrap();
}
fn store(f: &TestHome, bytes: &[u8]) {
    fs::create_dir_all(&f.host.config_dir).unwrap();
    fs::set_permissions(&f.host.config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    let path = f.host.config_dir.join("discovery-ignore.toml");
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn persistent_exact_component_prefixes_prune_new_descendants_and_undo_without_live_changes() {
    let f = TestHome::new();
    fs::create_dir_all(f.host.home.join(".tool/sub")).unwrap();
    fs::write(
        f.host.home.join(".tool/sub/config"),
        "ordinary configuration",
    )
    .unwrap();
    fs::write(f.host.home.join(".tool-other"), "other").unwrap();
    let old = discovery::discover(&f.host, None)
        .unwrap()
        .candidates
        .into_iter()
        .find(|c| c.entry.path.ends_with("sub/config"))
        .unwrap()
        .entry;
    change(&f, ".tool", true);
    fs::write(f.host.home.join(".tool/new"), "new").unwrap();
    let rules = Rules::load(&f.host).unwrap();
    assert!(rules.excludes(Path::new(".tool/sub/config")));
    assert!(!rules.excludes(Path::new(".tool-other")));
    let report = discovery::discover(&f.host, None).unwrap();
    assert!(
        report
            .candidates
            .iter()
            .all(|c| !c.entry.path.starts_with(f.host.home.join(".tool")))
    );
    assert!(
        report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with(".tool-other"))
    );
    assert!(discovery::selected_content(&f.host, None, &old).is_err());
    change(&f, ".tool/sub", true);
    change(&f, ".tool", false);
    let rules = Rules::load(&f.host).unwrap();
    assert!(rules.excludes(Path::new(".tool/sub/config")));
    assert!(!rules.excludes(Path::new(".tool/new")));
    change(&f, ".tool/sub", false);
    assert_eq!(Rules::load(&f.host).unwrap().paths().count(), 0);
    assert_eq!(
        fs::read_to_string(f.host.home.join(".tool/sub/config")).unwrap(),
        "ordinary configuration"
    );
    assert_eq!(
        fs::metadata(f.host.config_dir.join("discovery-ignore.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&f.host.config_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!f.host.config_dir.join("local.toml").exists());
    assert!(!f.host.state_dir.join("transactions").exists());
}
#[test]
fn previews_and_dry_run_do_not_create_state_and_stale_or_cross_host_approval_is_refused() {
    let f = TestHome::new();
    let preview = Preview::prepare(&f.host, Path::new(".tool"), true).unwrap();
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    assert!(preview.approve().execute(&f.host, true).is_err());
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let stale = Preview::prepare(&f.host, Path::new(".tool"), true)
        .unwrap()
        .approve();
    change(&f, ".other", true);
    assert!(stale.execute(&f.host, false).is_err());
    let other = TestHome::new();
    let approval = Preview::prepare(&f.host, Path::new(".tool"), true)
        .unwrap()
        .approve();
    assert!(approval.execute(&other.host, false).is_err());
    assert!(!other.host.state_dir.exists());
    // Even a byte-identical inode replacement invalidates a preview.
    let approval = Preview::prepare(&f.host, Path::new(".tool"), true)
        .unwrap()
        .approve();
    let path = f.host.config_dir.join("discovery-ignore.toml");
    let text = fs::read(&path).unwrap();
    fs::rename(&path, path.with_extension("old")).unwrap();
    store(&f, &text);
    assert!(approval.execute(&f.host, false).is_err());
}
#[test]
fn writer_lock_and_global_history_blockers_prevent_preference_publication() {
    let f = TestHome::new();
    let approval = Preview::prepare(&f.host, Path::new(".tool"), true)
        .unwrap()
        .approve();
    let lock = dort_core::state::WriterLock::acquire(&f.host).unwrap();
    assert!(approval.execute(&f.host, false).is_err());
    assert!(!f.host.config_dir.exists());
    drop(lock);
    let transactions = f
        .host
        .state_dir
        .join("repositories")
        .join("a".repeat(64))
        .join("transactions");
    fs::create_dir_all(&transactions).unwrap();
    fs::set_permissions(&transactions, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        transactions.join("invalid-transaction"),
        "malformed fixture",
    )
    .unwrap();
    let approval = Preview::prepare(&f.host, Path::new(".tool"), true)
        .unwrap()
        .approve();
    assert!(approval.execute(&f.host, false).is_err());
    assert!(!f.host.config_dir.exists());
}

#[test]
fn malformed_unsafe_non_utf8_oversized_and_linked_preferences_fail_closed() {
    for text in [
        "paths = ['../outside']",
        "paths = ['/absolute']",
        "paths = ['.a//b']",
        "paths = ['.a/./b']",
        "paths = ['.a/*']",
        "paths = ['']",
        "unknown = 'PRIVATE-SENTINEL'",
        "paths = [",
    ] {
        let f = TestHome::new();
        store(&f, text.as_bytes());
        let error = Rules::load(&f.host).err().unwrap().to_string();
        assert!(!error.contains("PRIVATE-SENTINEL"));
        assert!(discovery::discover(&f.host, None).is_err());
    }
    let f = TestHome::new();
    store(&f, &[0xff]);
    assert!(Rules::load(&f.host).is_err());
    store(&f, &vec![b' '; 262145]);
    assert!(Rules::load(&f.host).is_err());
    store(&f, b"paths = []\n");
    let path = f.host.config_dir.join("discovery-ignore.toml");
    fs::hard_link(&path, path.with_extension("link")).unwrap();
    assert!(Rules::load(&f.host).is_err());
    fs::remove_file(&path).unwrap();
    symlink(path.with_extension("link"), &path).unwrap();
    assert!(Rules::load(&f.host).is_err());
}
