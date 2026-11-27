mod support;
use dort_core::{
    Error, Result, generated::Deployment, local::RepositoryRegistry, state::WriterLock,
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
