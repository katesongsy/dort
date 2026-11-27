mod support;
use dort_core::{
    Error, Result,
    generated::{Deployment, DriftStatus, Preview},
    local::{LocalConfig, RepositoryRegistry, repository_state_path},
    state::WriterLock,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
use support::TestHome;

const FILE: &str = r#"
[[files]]
id = "settings"
mode = "template"
source = "settings.tmpl"
target = "~/app/settings"
inputs = { theme = "青" }
"#;
const SOURCE: &[u8] = "  theme={{ theme }}\t\r\n next=\\value\0".as_bytes();
const RENDERED: &[u8] = "  theme=青\t\r\n next=\\value\0".as_bytes();

struct Fixture {
    home: TestHome,
    repo: PathBuf,
    folder: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let home = TestHome::new();
        let repo = home.repository("repo");
        let folder = home.host.home.join("app");
        fs::create_dir(&folder).unwrap();
        let f = Self { home, repo, folder };
        f.manifest(&format!("schema = 1\n{FILE}"));
        fs::write(f.repo.join("settings.tmpl"), SOURCE).unwrap();
        let lock = WriterLock::acquire(&f.home.host).unwrap();
        RepositoryRegistry::register(&f.home.host, &lock, &f.repo).unwrap();
        f
    }
    fn manifest(&self, text: &str) {
        fs::write(self.repo.join("dort.toml"), text).unwrap();
    }
    fn prepare(&self) -> Result<Deployment> {
        Deployment::prepare(&self.home.host, None, "settings", &[])
    }
    fn blocked(&self) -> String {
        let Err(Error::Invalid(reason)) = self.prepare() else {
            panic!("expected deployment to be blocked");
        };
        reason
    }
    fn destination(&self) -> PathBuf {
        self.folder.join("settings")
    }
    fn status(&self) -> DriftStatus {
        Preview::prepare(&self.home.host, &self.repo, "settings")
            .unwrap()
            .drift_status
            .unwrap()
    }
    fn deploy(&self) {
        let published = self.prepare().unwrap().approve().execute().unwrap();
        assert!(published.record_warning.is_none());
    }
    fn records(&self) -> PathBuf {
        let local = LocalConfig::load(&self.home.host).unwrap();
        repository_state_path(&self.home.host, local.active_repository.as_deref().unwrap())
            .unwrap()
            .join("generated")
    }
}

#[test]
fn review_and_cancel_write_nothing_then_approval_writes_exact_bytes_and_mode() {
    for (extra, label, mode) in [("", "0600", 0o600), ("permissions = '0644'", "0644", 0o644)] {
        let f = Fixture::new();
        f.manifest(&format!("schema = 1\n{FILE}{extra}\n"));
        let canceled = f.prepare().unwrap();
        assert_eq!(canceled.review().destination, f.destination());
        assert_eq!(canceled.review().permissions, label);
        assert_eq!(canceled.review().rendered.as_bytes(), RENDERED);
        drop(canceled);
        assert!(!f.records().exists());
        assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
        let result = f.prepare().unwrap().approve().execute().unwrap();
        assert_eq!(result.destination, f.destination());
        assert!(result.directory_synced);
        assert_eq!(fs::read(&result.destination).unwrap(), RENDERED);
        assert_eq!(
            fs::metadata(&result.destination)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            mode
        );
        assert!(f.blocked().contains("already exists"));
        assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 1);
    }
}

#[test]
fn drift_distinguishes_proposed_changes_from_local_edits_permissions_and_deletion() {
    let f = Fixture::new();
    assert_eq!(f.status(), DriftStatus::Missing);
    assert!(!f.records().exists());
    f.deploy();
    assert_eq!(f.status(), DriftStatus::UpToDate);
    f.manifest(&format!("schema = 1\n{FILE}permissions = '0600'\n"));
    assert_eq!(f.status(), DriftStatus::UpToDate);
    f.manifest(&format!("schema = 1\n{FILE}permissions = '0644'\n"));
    assert_eq!(f.status(), DriftStatus::UpdateAvailable);
    let path = f.destination();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(f.status(), DriftStatus::LocalPermissionsChanged);
    fs::write(f.repo.join("settings.tmpl"), "changed").unwrap();
    fs::write(&path, "changed").unwrap();
    assert_eq!(f.status(), DriftStatus::LocallyEditedAndPermissionsChanged);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(f.status(), DriftStatus::LocallyEdited);
    fs::write(&path, RENDERED).unwrap();
    assert_eq!(f.status(), DriftStatus::UpdateAvailable);
    fs::remove_file(&path).unwrap();
    assert_eq!(f.status(), DriftStatus::LocallyDeleted);
    assert!(f.blocked().contains("First deployment cannot recreate it"));
    fs::remove_dir(&f.folder).unwrap();
    assert_eq!(f.status(), DriftStatus::LocallyDeleted);
    assert!(f.blocked().contains("First deployment cannot recreate it"));
}

#[test]
fn receipt_scope_uses_the_repository_mapping_id_and_normalized_destination() {
    let f = Fixture::new();
    fs::write(f.destination(), RENDERED).unwrap();
    assert_eq!(f.status(), DriftStatus::UnrecordedExisting);
    assert!(f.blocked().contains("already exists"));
    fs::remove_file(f.destination()).unwrap();
    f.deploy();
    let absolute_alias = format!("{}//app/settings/", f.home.host.home.display());
    f.manifest(&format!(
        "schema = 1\n{}",
        FILE.replace("~/app/settings", &absolute_alias)
    ));
    assert_eq!(f.status(), DriftStatus::UpToDate);
    let unrelated_repo = f.home.repository("unregistered");
    fs::copy(f.repo.join("dort.toml"), unrelated_repo.join("dort.toml")).unwrap();
    fs::write(unrelated_repo.join("settings.tmpl"), SOURCE).unwrap();
    for register in [false, true] {
        if register {
            let lock = WriterLock::acquire(&f.home.host).unwrap();
            RepositoryRegistry::register(&f.home.host, &lock, &unrelated_repo).unwrap();
        }
        let unrelated = Preview::prepare(&f.home.host, &unrelated_repo, "settings").unwrap();
        assert_eq!(
            unrelated.drift_status,
            Some(DriftStatus::UnrecordedExisting)
        );
    }
    f.manifest(&format!(
        "schema = 1\n{}",
        FILE.replace("id = \"settings\"", "id = \"other\"")
    ));
    let other = Preview::prepare(&f.home.host, &f.repo, "other").unwrap();
    assert_eq!(other.drift_status, Some(DriftStatus::UnrecordedExisting));
    f.manifest(&format!(
        "schema = 1\n{}",
        FILE.replace("~/app/settings", "~/app/elsewhere")
    ));
    assert_eq!(f.status(), DriftStatus::Missing);
}

#[test]
fn drift_fails_closed_for_replacements_sensitive_content_and_unreadable_or_large_files() {
    let f = Fixture::new();
    f.deploy();
    let path = f.destination();
    fs::write(&path, "token=do-not-report-this").unwrap();
    assert_eq!(f.status(), DriftStatus::Unsafe);
    let json = serde_json::to_string(&Preview::prepare(&f.home.host, &f.repo, "settings").unwrap())
        .unwrap();
    assert!(!json.contains("do-not-report-this") && !json.contains("sha256"));
    fs::write(&path, vec![b'x'; 128 * 1024 + 1]).unwrap();
    assert_eq!(f.status(), DriftStatus::Unverifiable);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(f.status(), DriftStatus::Unverifiable);
    fs::remove_file(&path).unwrap();
    let credential = f.home.host.home.join(".netrc");
    fs::write(&credential, b"private").unwrap();
    symlink(&credential, &path).unwrap();
    assert_eq!(f.status(), DriftStatus::Unsafe);
    fs::remove_file(&path).unwrap();
    fs::hard_link(&credential, &path).unwrap();
    assert_eq!(f.status(), DriftStatus::Unsafe);
    fs::remove_file(&path).unwrap();
    f.manifest(&format!(
        "schema = 1\n{}",
        FILE.replace("~/app/settings", "~/.netrc")
    ));
    assert_eq!(f.status(), DriftStatus::Unsafe);
    assert_eq!(fs::read(&credential).unwrap(), b"private");
}

#[test]
fn invalid_or_oversized_private_records_are_not_used_or_overwritten() {
    let f = Fixture::new();
    f.deploy();
    let file = fs::read_dir(f.records())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let original = fs::read(&file).unwrap();
    let text = std::str::from_utf8(&original).unwrap();
    for private in ["theme=", "{{", "青"] {
        assert!(!text.contains(private));
    }
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let valid: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let mut bad_id = valid.clone();
    bad_id["identity"]["mapping_id"] = "other".into();
    let mut bad_mode = valid.clone();
    bad_mode["deployed"]["mode"] = 0o777.into();
    let mut wrong_version = valid;
    wrong_version["schema"] = 2.into();
    for bad in [
        vec![b' '; 32 * 1024 + 1],
        b"unfinished".to_vec(),
        serde_json::to_vec(&bad_id).unwrap(),
        serde_json::to_vec(&bad_mode).unwrap(),
        serde_json::to_vec(&wrong_version).unwrap(),
    ] {
        fs::write(&file, &bad).unwrap();
        assert_eq!(f.status(), DriftStatus::Unverifiable);
        assert!(f.blocked().contains("local state"));
        assert_eq!(fs::read(&file).unwrap(), bad);
    }
    fs::remove_file(&file).unwrap();
    symlink(f.destination(), &file).unwrap();
    assert_eq!(f.status(), DriftStatus::Unverifiable);
    fs::remove_file(f.destination()).unwrap();
    assert!(f.blocked().contains("local state"));
}

#[test]
fn a_post_publish_record_error_keeps_the_file_and_warns() {
    if rustix::process::geteuid().as_raw() == 0 {
        return; // Root can write to the read-only test directory.
    }
    let f = Fixture::new();
    let folder = f.records();
    fs::create_dir_all(&folder).unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap();
    let result = f.prepare().unwrap().approve().execute();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    let result = result.unwrap();
    assert!(result.directory_synced);
    assert!(result.record_warning.unwrap().contains("Future previews"));
    assert_eq!(fs::read(result.destination).unwrap(), RENDERED);
    assert_eq!(f.status(), DriftStatus::UnrecordedExisting);
    assert!(f.blocked().contains("already exists"));
}

#[test]
fn changed_declaration_template_parent_target_or_preferences_invalidate_approval() {
    for (change, kept) in [
        ("manifest", None),
        ("source", None),
        ("parent", None),
        ("target", Some(b"racer".as_slice())),
        ("preferences", None),
    ] {
        let f = Fixture::new();
        let approval = f.prepare().unwrap().approve();
        match change {
            "manifest" => f.manifest(&format!("schema = 1\n{FILE}# edited\n")),
            "source" => fs::write(
                f.repo.join("settings.tmpl"),
                b"  theme={{theme}}\t\r\n next=\\value\0",
            )
            .unwrap(),
            "parent" => {
                fs::rename(&f.folder, f.folder.with_extension("old")).unwrap();
                fs::create_dir(&f.folder).unwrap();
            }
            "target" => fs::write(f.destination(), b"racer").unwrap(),
            "preferences" => {
                let lock = WriterLock::acquire(&f.home.host).unwrap();
                let sibling = f.home.repository("another");
                RepositoryRegistry::register(&f.home.host, &lock, &sibling).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(approval.execute(), Err(Error::Invalid(_))),
            "{change}"
        );
        assert_eq!(fs::read(f.destination()).ok().as_deref(), kept, "{change}");
        assert_eq!(
            fs::read_dir(&f.folder).unwrap().count(),
            usize::from(kept.is_some()),
            "{change}"
        );
        if change == "parent" {
            assert_eq!(
                fs::read_dir(f.folder.with_extension("old"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
}

#[test]
fn existing_symlinks_and_untrusted_or_missing_folders_are_never_written() {
    let f = Fixture::new();
    let path = f.destination();
    symlink("nowhere", &path).unwrap();
    assert!(f.blocked().contains("already exists"));
    assert_eq!(fs::read_link(path).unwrap(), Path::new("nowhere"));
    fs::remove_file(f.destination()).unwrap();
    fs::set_permissions(&f.folder, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(f.blocked().contains("not trusted"));
    fs::remove_dir(&f.folder).unwrap();
    let error = f.blocked();
    assert!(error.contains(f.folder.to_str().unwrap()) && error.contains("Create it"));
    symlink(&f.repo, &f.folder).unwrap();
    assert!(f.prepare().err().unwrap().to_string().contains("symlink"));
    assert!(!f.repo.join("settings").exists());
}

#[test]
fn protected_destinations_secret_contents_and_scope_blocks_do_not_offer_approval() {
    let f = Fixture::new();
    for target in [
        "~/.ssh/settings",
        "~/.config/dort/other",
        "~/proj/.git/settings",
    ] {
        let declaration = FILE.replace("~/app/settings", target);
        f.manifest(&format!("schema = 1\n{declaration}"));
        assert!(f.prepare().is_err(), "{target}");
    }
    let declaration = FILE.replace("~/app/settings", "~/APP-CONFLICT");
    f.manifest(&format!(
        "schema = 1\n{declaration}\n[[files]]\nid='other'\nmode='link'\nsource='other'\ntarget={:?}\n",
        f.home.host.home.join("APP-CONFLICT").to_str().unwrap(),
    ));
    assert!(f.blocked().contains("conflicting expanded destinations"));
    f.manifest(&format!(
        "schema = 1\n[profiles]\navailable=['work']\n{FILE}profiles=['work']\n"
    ));
    assert!(f.blocked().contains("inactive profile"));
    f.manifest(&format!(
        "schema = 1\n[[tools]]\nid='editor'\nbinary='editor'\ninstall={{provider='brew', package='editor'}}\n{FILE}requires=['editor']\n"
    ));
    assert!(f.blocked().contains("requires declared tools"));
    f.manifest(&format!("schema = 1\n{FILE}"));
    fs::write(f.repo.join("settings.tmpl"), b"password=do-not-display").unwrap();
    let error = f.blocked();
    assert!(error.contains("may contain secrets") && !error.contains("do-not-display"));
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}

#[test]
fn held_lock_and_unresolved_recovery_block_approved_deployment() {
    let f = Fixture::new();
    let deployment = f.prepare().unwrap();
    let lock = WriterLock::acquire(&f.home.host).unwrap();
    assert!(matches!(deployment.approve().execute(), Err(Error::Locked)));
    let id = RepositoryRegistry::register(&f.home.host, &lock, &f.repo).unwrap();
    drop(lock);
    let approval = f.prepare().unwrap().approve();
    let transactions = f
        .home
        .host
        .state_dir
        .join("repositories")
        .join(id)
        .join("transactions");
    fs::create_dir_all(&transactions).unwrap();
    for path in [
        transactions.parent().unwrap().parent().unwrap(),
        transactions.parent().unwrap(),
        &transactions,
    ] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(transactions.join("unknown"), b"unfinished state").unwrap();
    assert!(matches!(
        approval.execute(),
        Err(Error::RecoveryRequired(_))
    ));
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_write_acl_is_blocked_even_when_unix_mode_is_private() {
    let f = Fixture::new();
    fs::set_permissions(&f.folder, fs::Permissions::from_mode(0o700)).unwrap();
    let chmod = |args: &[&str]| {
        assert!(
            std::process::Command::new("/bin/chmod")
                .args(args)
                .arg(&f.folder)
                .status()
                .unwrap()
                .success()
        );
    };
    chmod(&["+a", "group:everyone deny delete"]);
    let approval = f.prepare().unwrap().approve();
    chmod(&["+a", "group:everyone allow add_file,delete_child"]);
    assert_eq!(
        fs::metadata(&f.folder).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(f.blocked().contains("extra write permissions"));
    assert!(matches!(approval.execute(), Err(Error::Invalid(_))));
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_inherited_file_access_blocks_deployment_and_removes_the_stage() {
    let f = Fixture::new();
    assert!(
        std::process::Command::new("/bin/chmod")
            .args(["+a", "group:everyone allow read,file_inherit,only_inherit"])
            .arg(&f.folder)
            .status()
            .unwrap()
            .success()
    );
    let result = f.prepare().unwrap().approve().execute();
    let Err(Error::Invalid(reason)) = result else {
        panic!("expected inherited file access to block deployment");
    };
    assert!(reason.contains("grants extra access"));
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}
