mod support;
use dort_core::{
    local::{LocalConfig, RepositoryRegistry},
    manifest_edit::RepairDraft,
    state::WriterLock,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};
use support::TestHome;

const OLD: &str = r#"schema = 1
# keep this
[profiles]
available = ["dev"]
[profiles.platform]
ubuntu = ["dev"]

[[tools]]
id = "git"
binary = "git"
[tools.install]
macos = { provider = "brew", package = "git" }

[[files]]
id = "editor"
mode = "link"
source = "editor"
target = "~/.editor"
[files.overrides]
ubuntu = "editor-linux"
"#;

fn fixture() -> (TestHome, PathBuf, PathBuf) {
    let t = TestHome::new();
    let repo = t.host.home.join("clone");
    fs::create_dir(&repo).unwrap();
    let git = [
        "/usr/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ]
    .into_iter()
    .map(Path::new)
    .find(|p| p.is_file())
    .expect("Git for repository tests");
    assert!(
        Command::new(git)
            .current_dir(&repo)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
    (t, repo, git.to_path_buf())
}

#[test]
fn manifest_repair_reviews_legacy_fields_and_changes_only_the_manifest() {
    let (t, repo, git) = fixture();
    let path = repo.join("dort.toml");
    fs::write(&path, OLD).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let draft = RepairDraft::open(&t.host, &repo, &git).unwrap();
    assert!(draft.review(&t.host, OLD.into()).is_err());
    let replacement = draft.suggested();
    assert!(replacement.contains("# keep this"));
    let review = draft.review(&t.host, replacement.into()).unwrap();
    assert!(review.human().contains("Before | ubuntu"));
    assert!(review.human().contains("After  | linux"));
    assert!(review.human().contains("No registration"));
    assert_eq!(fs::read_to_string(&path).unwrap(), OLD);
    assert!(!t.host.state_dir.exists());
    assert!(!t.host.local_config_path().exists());
    let backup = review.approve().execute(&t.host).unwrap().unwrap();
    assert_eq!(fs::read_to_string(&backup).unwrap(), OLD);
    assert_eq!(
        backup.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), replacement);
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o640);
    assert!(LocalConfig::load(&t.host).unwrap().repositories.is_empty());
    assert!(!t.host.home.join(".editor").exists());
}

#[test]
fn manifest_repair_refuses_changed_file_or_registration_after_review() {
    let (t, repo, git) = fixture();
    let path = repo.join("dort.toml");
    fs::write(&path, OLD).unwrap();
    let draft = RepairDraft::open(&t.host, &repo, &git).unwrap();
    let mut review = draft.review(&t.host, "schema = 1\n".into()).unwrap();
    fs::write(&path, format!("{OLD}\n# local change\n")).unwrap();
    assert!(review.approve().execute(&t.host).is_err());
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .ends_with("# local change\n")
    );
    assert!(!t.host.state_dir.join("manifest-repairs").exists());
    let draft = RepairDraft::open(&t.host, &repo, &git).unwrap();
    review = draft.review(&t.host, "schema = 1\n".into()).unwrap();
    let other = t.repository("another");
    let lock = WriterLock::acquire(&t.host).unwrap();
    RepositoryRegistry::register(&t.host, &lock, &other).unwrap();
    drop(lock);
    assert!(review.approve().execute(&t.host).is_err());
    assert!(!t.host.state_dir.join("manifest-repairs").exists());
}

#[test]
fn manifest_repair_never_guesses_a_new_schema_or_unsupported_install_rule() {
    let (t, repo, git) = fixture();
    let path = repo.join("dort.toml");
    fs::write(&path, "schema = 2\n").unwrap();
    assert!(
        RepairDraft::open(&t.host, &repo, &git)
            .err()
            .unwrap()
            .to_string()
            .contains("Unsupported manifest schema")
    );
    let apt = OLD.replace(
        "macos = { provider = \"brew\"",
        "ubuntu = { provider = \"apt\"",
    );
    fs::write(&path, &apt).unwrap();
    let draft = RepairDraft::open(&t.host, &repo, &git).unwrap();
    assert!(draft.suggested().contains("ubuntu = { provider = \"apt\""));
    assert!(draft.review(&t.host, draft.suggested().into()).is_err());
    fs::remove_file(&path).unwrap();
    let elsewhere = t.host.home.join("outside");
    fs::write(&elsewhere, OLD).unwrap();
    symlink(&elsewhere, &path).unwrap();
    assert!(RepairDraft::open(&t.host, &repo, &git).is_err());
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), OLD);
}
