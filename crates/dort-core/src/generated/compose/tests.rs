use super::*;
use crate::{
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    template_files,
};
use std::{fs, os::unix::fs::MetadataExt};
use tempfile::TempDir;

const TEXT: &str = "name = {{ inputs.name }}\n";

struct Fixture {
    _sandbox: TempDir,
    host: HostContext,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let base = sandbox.path().canonicalize().unwrap();
        let home = base.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        let root = base.join("repo");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        let f = Self {
            _sandbox: sandbox,
            host,
            root,
        };
        f.manifest("schema = 1\n");
        let lock = WriterLock::acquire(&f.host).unwrap();
        RepositoryRegistry::register(&f.host, &lock, &f.root).unwrap();
        f
    }

    fn manifest(&self, text: &str) {
        fs::write(self.root.join("dort.toml"), text).unwrap();
    }

    fn mapping(&self, id: &str, source: &str) -> ManagedFile {
        ManagedFile {
            id: id.into(),
            source: Some(source.into()),
            template: None,
            target: format!("~/.{id}").into(),
            mode: FileMode::Template,
            profiles: vec!["base".into()],
            requires: vec![],
            overrides: Default::default(),
            inputs: [("name".to_owned(), "dort".to_owned())].into(),
            syntax: None,
            host_inputs: None,
            local_inputs: None,
            permissions: None,
        }
    }

    fn prepare(&self, mapping: ManagedFile, text: &str) -> Result<Preview> {
        Preview::prepare(&self.host, &self.root, mapping, text.into())
    }

    fn create(&self, id: &str, path: &str) -> Result<Preview> {
        self.prepare(self.mapping(id, path), TEXT)
    }

    fn entries(&self) -> usize {
        let text = fs::read_to_string(self.root.join("dort.toml")).unwrap();
        crate::Manifest::parse(&text).unwrap().files.len()
    }
}

#[test]
fn compose_blank_repository_creates_folder_file_and_mapping() {
    let f = Fixture::new();
    let manifest_before = fs::read(f.root.join("dort.toml")).unwrap();
    let preview = f.create("name", "templates/name.jinja").unwrap();
    let review = preview.review();
    assert_eq!(review.source, Path::new("templates/name.jinja"));
    assert_eq!(review.permissions, 0o644);
    let template = review.template.expect("folder and file effect");
    assert_eq!(
        template.folder,
        Some(template_files::Folder {
            path: "templates".into(),
            permissions: 0o755
        })
    );
    assert_eq!(template.after, Some(TEXT));
    let rendered = review.rendered.expect("rendered output");
    assert_eq!(rendered.rendered, "name = dort\n");
    assert_eq!(rendered.destination, f.host.home.join(".name"));
    // Review writes nothing.
    assert_eq!(fs::read(f.root.join("dort.toml")).unwrap(), manifest_before);
    assert!(!f.root.join("templates").exists());

    let outcome = preview.approve().execute(&f.host).unwrap();
    let Outcome::Created {
        folder_created,
        backup: Some(backup),
    } = outcome
    else {
        panic!("expected create: {}", outcome.human());
    };
    assert!(folder_created);
    assert_eq!(
        fs::metadata(f.root.join("templates")).unwrap().mode() & 0o777,
        0o755
    );
    let file = f.root.join("templates/name.jinja");
    assert_eq!(fs::read(&file).unwrap(), TEXT.as_bytes());
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o644);
    assert_eq!(fs::read(&backup).unwrap(), manifest_before);
    assert_eq!(f.entries(), 1);
    let rows = template_files::list(&f.host, &f.root).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, Path::new("templates/name.jinja"));
    assert_eq!(rows[0].mappings, 1);
    let saved =
        crate::Manifest::parse(&fs::read_to_string(f.root.join("dort.toml")).unwrap()).unwrap();
    let file = &saved.files[0];
    assert_eq!(file.id, "name");
    assert_eq!(
        file.source.as_deref(),
        Some(Path::new("templates/name.jinja"))
    );
    assert_eq!(file.target, Path::new("~/.name"));
    assert_eq!(file.inputs.get("name").map(String::as_str), Some("dort"));
    assert_eq!(file.profiles, ["base"]);
}

#[test]
fn compose_cancel_writes_nothing() {
    let f = Fixture::new();
    let manifest = fs::read(f.root.join("dort.toml")).unwrap();
    let preview = f.create("name", "templates/name.jinja").unwrap();
    drop(preview);
    assert!(!f.root.join("templates").exists());
    assert_eq!(fs::read(f.root.join("dort.toml")).unwrap(), manifest);

    let locked = Fixture::new();
    let preview = locked.create("name", "templates/name.jinja").unwrap();
    let _lock = WriterLock::acquire(&locked.host).unwrap();
    assert!(matches!(
        preview.approve().execute(&locked.host),
        Err(Error::Locked)
    ));
    assert!(!locked.root.join("templates").exists());
}

#[test]
fn compose_refuses_existing_other_file_and_stale_review() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("templates")).unwrap();
    fs::write(f.root.join("templates/name.jinja"), "other\n").unwrap();
    assert!(f.create("name", "templates/name.jinja").is_err());

    let stale = Fixture::new();
    let approved = stale
        .create("name", "templates/name.jinja")
        .unwrap()
        .approve();
    stale.manifest("schema = 1\n# changed\n");
    assert!(approved.execute(&stale.host).is_err());
    assert!(!stale.root.join("templates/name.jinja").exists());
    assert_eq!(stale.entries(), 0);

    for path in [
        "plain.jinja",
        "templates/nested/name.jinja",
        "templates/name.txt",
    ] {
        assert!(f.create("name", path).is_err(), "{path} must be refused");
    }
    let mut inline = f.mapping("inline", "templates/inline.jinja");
    inline.template = Some("x".into());
    assert!(f.prepare(inline, TEXT).is_err());
    let mut link = f.mapping("link", "templates/link.jinja");
    link.mode = FileMode::Link;
    assert!(f.prepare(link, TEXT).is_err());
}

#[test]
fn compose_refuses_a_file_another_mapping_uses() {
    let f = Fixture::new();
    f.manifest(
        "schema = 1\n\n[[files]]\nid = \"taken\"\nmode = \"template\"\nsource = \"templates/name.jinja\"\ntarget = \"~/.taken\"\n",
    );
    fs::create_dir(f.root.join("templates")).unwrap();
    fs::write(f.root.join("templates/name.jinja"), TEXT).unwrap();
    assert!(f.create("name", "templates/name.jinja").is_err());
    assert_eq!(f.entries(), 1);
}

#[test]
fn compose_manifest_failure_keeps_the_file_and_reports_both_facts() {
    let f = Fixture::new();
    let preview = f.create("name", "templates/name.jinja").unwrap();
    let approval = preview.approve();
    let outcome = approval
        .execute_before_manifest(&f.host, || f.manifest("schema = 1\n# raced\n"))
        .unwrap();
    let Outcome::MappingNotWritten {
        ref file,
        folder_created,
        ref reason,
        remedy,
    } = outcome
    else {
        panic!("expected an unwritten mapping: {}", outcome.human());
    };
    assert!(folder_created);
    assert_eq!(file, Path::new("templates/name.jinja"));
    assert!(!reason.is_empty());
    assert!(!remedy.is_empty());
    let message = outcome.human();
    assert!(message.contains("templates/name.jinja"), "{message}");
    assert!(message.contains("Mapping not written"), "{message}");
    assert_eq!(
        fs::read(f.root.join("templates/name.jinja")).unwrap(),
        TEXT.as_bytes()
    );
    assert_eq!(f.entries(), 0);

    // Reviewing again sees the file in place and writes only the mapping.
    let again = f.create("name", "templates/name.jinja").unwrap();
    let review = again.review();
    assert!(review.template.is_none(), "no second file effect");
    assert_eq!(review.rendered.expect("rendered").rendered, "name = dort\n");
    let inode = fs::metadata(f.root.join("templates/name.jinja"))
        .unwrap()
        .ino();
    let Outcome::Mapped { backup: Some(_) } = again.approve().execute(&f.host).unwrap() else {
        panic!("expected the mapping step only");
    };
    assert_eq!(f.entries(), 1);
    let file = fs::metadata(f.root.join("templates/name.jinja")).unwrap();
    assert_eq!(file.ino(), inode);
    assert_eq!(
        fs::read(f.root.join("templates/name.jinja")).unwrap(),
        TEXT.as_bytes()
    );
}

#[test]
fn compose_repeat_approval_does_not_duplicate_or_overwrite() {
    let f = Fixture::new();
    f.create("name", "templates/name.jinja")
        .unwrap()
        .approve()
        .execute(&f.host)
        .unwrap();
    let manifest = fs::read(f.root.join("dort.toml")).unwrap();
    let file = fs::metadata(f.root.join("templates/name.jinja")).unwrap();
    // The mapping already owns the ID, so a second create is refused.
    assert!(f.create("name", "templates/name.jinja").is_err());
    assert_eq!(fs::read(f.root.join("dort.toml")).unwrap(), manifest);
    assert_eq!(f.entries(), 1);
    let after = fs::metadata(f.root.join("templates/name.jinja")).unwrap();
    assert_eq!(after.ino(), file.ino());
    // The same file with different reviewed bytes is refused too.
    assert!(
        f.prepare(f.mapping("other", "templates/name.jinja"), "changed\n")
            .is_err()
    );
}

#[test]
fn compose_stale_file_refuses_the_in_place_review() {
    let f = Fixture::new();
    f.create("name", "templates/name.jinja")
        .unwrap()
        .approve()
        .execute(&f.host)
        .unwrap();
    fs::write(f.root.join("dort.toml"), "schema = 1\n").unwrap();
    let preview = f.create("name", "templates/name.jinja").unwrap();
    assert!(preview.review().template.is_none());
    fs::write(f.root.join("templates/name.jinja"), "changed\n").unwrap();
    assert!(preview.approve().execute(&f.host).is_err());
    assert_eq!(f.entries(), 0);
    assert_eq!(
        fs::read(f.root.join("templates/name.jinja")).unwrap(),
        b"changed\n"
    );
}
