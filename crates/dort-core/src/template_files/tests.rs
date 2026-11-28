use super::*;
use crate::{
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
};
use tempfile::TempDir;

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
        f.manifest("");
        f
    }

    fn registered() -> Self {
        let f = Self::new();
        let lock = WriterLock::acquire(&f.host).unwrap();
        RepositoryRegistry::register(&f.host, &lock, &f.root).unwrap();
        f
    }

    fn with_templates() -> Self {
        let f = Self::registered();
        fs::create_dir(f.root.join("templates")).unwrap();
        f
    }

    fn manifest(&self, extra: &str) {
        self.write("dort.toml", format!("schema = 1\n{extra}"));
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.root.join(path), bytes).unwrap();
    }

    fn create(&self, path: &str, text: &str) -> Result<Preview> {
        Preview::prepare(
            &self.host,
            &self.root,
            Change::Create(path.into()),
            text.into(),
        )
    }

    fn show(&self, path: &str) -> Result<Source> {
        show(&self.host, &self.root, Path::new(path))
    }

    fn edit(&self, source: Source, text: &str) -> Result<Preview> {
        Preview::prepare(&self.host, &self.root, Change::Edit(source), text.into())
    }

    fn delete(&self, path: &str) -> Result<Preview> {
        Preview::prepare(
            &self.host,
            &self.root,
            Change::Delete(self.show(path)?),
            String::new(),
        )
    }
}

#[test]
fn template_files_unregistered_repositories_cannot_be_opened() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("templates")).unwrap();
    f.write("templates/existing.jinja", "hello");
    assert!(matches!(list(&f.host, &f.root), Err(Error::Invalid(_))));
    assert!(matches!(
        f.show("templates/existing.jinja"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        f.create("templates/new.jinja", "hello"),
        Err(Error::Invalid(_))
    ));
    assert!(!f.host.config_dir.exists());
}

#[test]
fn template_files_blank_repository_lists_without_creating_folders() {
    let f = Fixture::registered();
    assert!(list(&f.host, &f.root).unwrap().is_empty());
    let preview = f.create("templates/new.jinja", "hello").unwrap();
    let review = preview.review();
    assert_eq!(review.action, Action::Create);
    assert_eq!(
        review.folder,
        Some(Folder {
            path: "templates".into(),
            permissions: 0o755
        })
    );
    assert_eq!(review.permissions, 0o644);
    assert!(!f.root.join("templates").exists());
    let Outcome::Created { folder_created } = preview.approve().execute(&f.host).unwrap() else {
        panic!("folder and file create should succeed")
    };
    assert!(folder_created);
    assert_eq!(
        fs::metadata(f.root.join("templates")).unwrap().mode() & 0o777,
        0o755
    );
    assert_eq!(
        fs::read(f.root.join("templates/new.jinja")).unwrap(),
        b"hello"
    );
    assert_eq!(list(&f.host, &f.root).unwrap().len(), 1);
}

#[test]
fn template_files_create_refuses_a_folder_that_appears_after_the_review() {
    let f = Fixture::registered();
    let path = "templates/new.jinja";
    let preview = f.create(path, "reviewed").unwrap();
    fs::create_dir(f.root.join("templates")).unwrap();
    fs::set_permissions(f.root.join("templates"), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(preview.approve().execute(&f.host).is_err());
    assert!(!f.root.join(path).exists());
    assert_eq!(
        fs::metadata(f.root.join("templates")).unwrap().mode() & 0o777,
        0o700
    );
    let link = Fixture::registered();
    fs::create_dir(link.root.join("elsewhere")).unwrap();
    let preview = link.create(path, "reviewed").unwrap();
    symlink("elsewhere", link.root.join("templates")).unwrap();
    assert!(preview.approve().execute(&link.host).is_err());
    assert!(!link.root.join("elsewhere/new.jinja").exists());
    let file = Fixture::registered();
    let preview = file.create(path, "reviewed").unwrap();
    file.write("templates", "not a folder");
    assert!(preview.approve().execute(&file.host).is_err());
    assert!(!file.root.join(path).exists());
}

#[test]
fn template_files_nested_folder_without_templates_refuses_the_review() {
    let f = Fixture::registered();
    let Some(Error::Invalid(message)) = f.create("templates/nested/new.jinja", "hi").err() else {
        panic!("a missing nested folder should block the preview");
    };
    assert!(message.contains("Create templates/"), "{message}");
    assert!(!f.root.join("templates").exists());
}

#[test]
fn template_files_create_reviews_exact_text_and_does_not_write_until_approved() {
    let f = Fixture::with_templates();
    let path = "templates/new.jinja";
    let text = " \t{{ 42 }}\r\n最後";
    let before_manifest = fs::read(f.root.join("dort.toml")).unwrap();
    let preview = f.create(path, text).unwrap();
    let review = preview.review();
    assert_eq!(review.action, Action::Create);
    assert_eq!(review.path, Path::new(path));
    assert_eq!(review.permissions, 0o644);
    assert!(review.folder.is_none());
    assert_eq!(review.before, None);
    assert_eq!(review.after, Some(text));
    assert!(review.retained.is_none());
    assert!(list(&f.host, &f.root).unwrap().is_empty());
    let config = LocalConfig::load(&f.host).unwrap();
    let state = repository_state_path(&f.host, config.repository_id(&f.root).unwrap()).unwrap();
    assert!(!state.join("template-edits").exists());
    assert!(
        matches!(
            preview.approve().execute(&f.host).unwrap(),
            Outcome::Created {
                folder_created: false
            }
        ),
        "an existing folder is not a reviewed effect"
    );
    assert_eq!(fs::read(f.root.join(path)).unwrap(), text.as_bytes());
    assert_eq!(
        fs::metadata(f.root.join(path)).unwrap().mode() & 0o777,
        0o644
    );
    assert_eq!(f.show(path).unwrap().text(), text);
    assert_eq!(fs::read(f.root.join("dort.toml")).unwrap(), before_manifest);
    assert!(f.create(path, "different").is_err());
}

#[test]
fn template_files_list_and_show_include_unlinked_and_external_referenced_sources() {
    let f = Fixture::with_templates();
    f.manifest(
        r#"
[[files]]
id = "settings"
mode = "template"
source = "old.conf"
target = "~/settings"
overrides = { linux = "templates/nested/settings.jinja" }

[[files]]
id = "other"
mode = "template"
source = "templates/nested/settings.jinja"
target = "~/other"
"#,
    );
    fs::create_dir(f.root.join("templates/nested")).unwrap();
    f.write("old.conf", "{{ invalid");
    f.write("templates/nested/settings.jinja", "hello");
    f.write("templates/unlinked.jinja", "there");
    f.write("templates/not-a-template.txt", "skip");
    f.write("unlinked.jinja", "skip");
    symlink("nested/settings.jinja", f.root.join("templates/link.jinja")).unwrap();
    assert_eq!(
        list(&f.host, &f.root).unwrap(),
        [
            Row {
                path: "old.conf".into(),
                mappings: 1,
                editable: false
            },
            Row {
                path: "templates/nested/settings.jinja".into(),
                mappings: 2,
                editable: true
            },
            Row {
                path: "templates/unlinked.jinja".into(),
                mappings: 0,
                editable: true
            },
        ]
    );
    let legacy = f.show("old.conf").unwrap();
    assert_eq!(legacy.text(), "{{ invalid");
    assert!(f.edit(legacy, "corrected").is_err());
    assert!(f.show("unlinked.jinja").is_err());
}

#[test]
fn template_files_edit_keeps_exact_predecessor_and_mode() {
    let f = Fixture::with_templates();
    let path = "templates/settings.jinja";
    let absolute = f.root.join(path);
    let before = "\tbefore\r\nlast ";
    let after = "after\n";
    f.write(path, before);
    fs::set_permissions(&absolute, fs::Permissions::from_mode(0o640)).unwrap();
    let old = fs::metadata(&absolute).unwrap();
    let preview = f.edit(f.show(path).unwrap(), after).unwrap();
    let review = preview.review();
    assert_eq!(review.action, Action::Edit);
    assert_eq!(review.before, Some(before));
    assert_eq!(review.after, Some(after));
    assert_eq!(review.permissions, 0o640);
    assert!(review.folder.is_none());
    let paths = review.retained.unwrap();
    assert!(!paths.reviewed.parent().unwrap().exists());
    assert_eq!(fs::read(&absolute).unwrap(), before.as_bytes());
    let Outcome::Edited(retained) = preview.approve().execute(&f.host).unwrap() else {
        panic!("edit should succeed")
    };
    assert_eq!(fs::read(&absolute).unwrap(), after.as_bytes());
    assert_eq!(fs::read(&retained.reviewed).unwrap(), before.as_bytes());
    assert_eq!(fs::read(&retained.displaced).unwrap(), before.as_bytes());
    let displaced = fs::metadata(&retained.displaced).unwrap();
    let current = fs::metadata(&absolute).unwrap();
    assert_eq!((displaced.dev(), displaced.ino()), (old.dev(), old.ino()));
    assert_eq!(current.mode() & 0o777, 0o640);
    assert_eq!(current.gid(), old.gid());
    assert_eq!(
        fs::metadata(retained.displaced.parent().unwrap())
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn template_files_stale_source_manifest_and_registration_refuse_writes() {
    let f = Fixture::with_templates();
    let path = "templates/settings.jinja";
    f.write(path, "before");
    let shown = f.show(path).unwrap();
    f.write(path, "stale");
    assert!(f.edit(shown, "after").is_err());
    let approved = f.edit(f.show(path).unwrap(), "after").unwrap().approve();
    f.write(path, "changed after review");
    assert!(approved.execute(&f.host).is_err());
    assert_eq!(
        fs::read(f.root.join(path)).unwrap(),
        b"changed after review"
    );
    let candidate = "templates/new.jinja";
    let approved = f.create(candidate, "after").unwrap().approve();
    f.manifest("# changed after review\n");
    assert!(approved.execute(&f.host).is_err());
    assert!(!f.root.join(candidate).exists());
    let approved = f.create(candidate, "after").unwrap().approve();
    fs::rename(
        f.host.local_config_path(),
        f.host.config_dir.join("moved.toml"),
    )
    .unwrap();
    assert!(approved.execute(&f.host).is_err());
    assert!(!f.root.join(candidate).exists());
}

#[test]
fn template_files_no_replace_create_handles_last_moment_race() {
    let f = Fixture::with_templates();
    let path = "templates/new.jinja";
    let approval = f.create(path, "reviewed").unwrap().approve();
    assert!(
        approval
            .execute_before_publish(&f.host, || f.write(path, "racer"))
            .is_err()
    );
    assert_eq!(fs::read(f.root.join(path)).unwrap(), b"racer");
}

#[test]
fn template_files_edit_reports_last_moment_race_and_retains_both_objects() {
    let f = Fixture::with_templates();
    let path = "templates/settings.jinja";
    f.write(path, "before");
    let approval = f
        .edit(f.show(path).unwrap(), "reviewed change")
        .unwrap()
        .approve();
    let incoming = f.root.join("incoming");
    fs::write(&incoming, "racer").unwrap();
    let inode = fs::metadata(&incoming).unwrap().ino();
    let outcome = approval
        .execute_before_publish(&f.host, || {
            fs::rename(&incoming, f.root.join(path)).unwrap();
        })
        .unwrap();
    let Outcome::NeedsAttention {
        retained: Some(retained),
        competing_change: true,
        durability_unknown: false,
        ..
    } = outcome
    else {
        panic!("expected a conflict: {outcome:?}");
    };
    assert_eq!(fs::read(f.root.join(path)).unwrap(), b"reviewed change");
    assert_eq!(fs::read(&retained.displaced).unwrap(), b"racer");
    assert_eq!(fs::metadata(&retained.displaced).unwrap().ino(), inode);
    assert_eq!(fs::read(retained.reviewed).unwrap(), b"before");
}

#[test]
fn template_files_unsafe_paths_bytes_and_other_mappings_are_refused() {
    let f = Fixture::with_templates();
    f.manifest(
        r#"
[[files]]
id = "settings"
mode = "link"
source = "plain"
target = "~/settings"
overrides = { macos = "templates/LINKED.jinja" }

[[files]]
id = "parent"
mode = "link"
source = "templates/shared"
target = "~/shared"

[[files]]
id = "generated"
mode = "template"
source = "templates/linked.jinja"
target = "~/generated"
"#,
    );
    fs::create_dir(f.root.join("templates/shared")).unwrap();
    f.write("templates/linked.jinja", "cross reference");
    f.write("templates/regular.jinja", "valid source");
    fs::hard_link(
        f.root.join("templates/regular.jinja"),
        f.root.join("templates/hard.jinja"),
    )
    .unwrap();
    symlink("regular.jinja", f.root.join("templates/soft.jinja")).unwrap();
    symlink("shared", f.root.join("templates/symdir")).unwrap();
    f.write("templates/invalid.jinja", [0xff]);
    f.write("templates/control.jinja", b"text\x1b[0m");
    f.write("templates/private.jinja", "password = example");
    f.write("templates/oversize.jinja", vec![b'a'; SOURCE_LIMIT + 1]);
    f.write("templates/world.jinja", "unsafe mode");
    fs::set_permissions(
        f.root.join("templates/world.jinja"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    for path in [
        "templates/hard.jinja",
        "templates/regular.jinja",
        "templates/soft.jinja",
        "templates/invalid.jinja",
        "templates/control.jinja",
        "templates/private.jinja",
        "templates/oversize.jinja",
        "templates/world.jinja",
        "templates/linked.jinja",
        "dort.toml",
    ] {
        assert!(f.show(path).is_err(), "should not read {path}");
    }
    for path in [
        "templates/../traversal.jinja",
        "templates/.git/new.jinja",
        "templates/secrets.jinja",
        "templates/symdir/new.jinja",
        "templates/shared/new.jinja",
        "templates/linked.jinja",
        "outside.jinja",
        "templates/new.txt",
    ] {
        assert!(f.create(path, "safe").is_err(), "should not write {path}");
    }
    for text in [
        "{{ unclosed",
        "password = example",
        "before\0after",
        &"a".repeat(SOURCE_LIMIT + 1),
    ] {
        assert!(f.create("templates/new.jinja", text).is_err());
    }
    assert!(!f.root.join("templates/new.jinja").exists());
    assert_eq!(
        fs::read(f.root.join("templates/linked.jinja")).unwrap(),
        b"cross reference"
    );
}

#[test]
fn template_files_delete_moves_unreferenced_file_and_keeps_bytes() {
    let f = Fixture::with_templates();
    let path = "templates/unused.jinja";
    let absolute = f.root.join(path);
    let text = "kept \r\nbytes ";
    f.write(path, text);
    fs::set_permissions(&absolute, fs::Permissions::from_mode(0o600)).unwrap();
    let old = fs::metadata(&absolute).unwrap();
    let preview = f.delete(path).unwrap();
    let review = preview.review();
    assert_eq!(review.action, Action::Delete);
    assert_eq!(review.path, Path::new(path));
    assert_eq!(review.before, Some(text));
    assert_eq!(review.after, None);
    assert_eq!(review.permissions, 0o600);
    assert_eq!(review.mappings, 0);
    assert!(review.folder.is_none() && review.retained.is_none());
    let removed = review.removed.unwrap();
    assert!(!removed.parent().unwrap().exists());
    assert_eq!(fs::read(&absolute).unwrap(), text.as_bytes());
    let Outcome::Deleted(removed) = preview.approve().execute(&f.host).unwrap() else {
        panic!("delete should succeed")
    };
    assert!(!absolute.exists());
    assert_eq!(fs::read(&removed.path).unwrap(), text.as_bytes());
    let moved = fs::metadata(&removed.path).unwrap();
    assert_eq!((moved.dev(), moved.ino()), (old.dev(), old.ino()));
    assert_eq!(moved.mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(removed.path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert!(list(&f.host, &f.root).unwrap().is_empty());
    let config = LocalConfig::load(&f.host).unwrap();
    let state = repository_state_path(&f.host, config.repository_id(&f.root).unwrap()).unwrap();
    assert!(removed.path.starts_with(state));
}

#[test]
fn template_files_delete_refuses_referenced_files_by_id() {
    let f = Fixture::with_templates();
    f.manifest(
        r#"
[[files]]
id = "shared"
mode = "template"
source = "templates/shared.jinja"
target = "~/shared"

[[files]]
id = "theme"
mode = "template"
source = "templates/base.jinja"
target = "~/theme"
overrides = { linux = "templates/shared.jinja" }
"#,
    );
    f.write("templates/shared.jinja", "shared");
    f.write("templates/base.jinja", "base");
    let Some(Error::Invalid(message)) = f.delete("templates/shared.jinja").err() else {
        panic!("references should refuse the review");
    };
    assert!(message.contains("shared (source)"), "{message}");
    assert!(message.contains("theme (linux override)"), "{message}");
    assert_eq!(
        fs::read(f.root.join("templates/shared.jinja")).unwrap(),
        b"shared"
    );
}

#[test]
fn template_files_delete_refuses_unsafe_stale_and_foreign_paths() {
    let f = Fixture::with_templates();
    f.manifest(
        r#"
[[files]]
id = "legacy"
mode = "template"
source = "legacy.jinja"
target = "~/legacy"
"#,
    );
    f.write("legacy.jinja", "legacy");
    f.write("templates/regular.jinja", "valid source");
    fs::hard_link(
        f.root.join("templates/regular.jinja"),
        f.root.join("templates/hard.jinja"),
    )
    .unwrap();
    symlink("regular.jinja", f.root.join("templates/soft.jinja")).unwrap();
    f.write("templates/private.jinja", "password = example");
    f.write("templates/world.jinja", "unsafe mode");
    fs::set_permissions(
        f.root.join("templates/world.jinja"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    f.write("templates/notes.txt", "not a template");
    for path in [
        "legacy.jinja",
        "dort.toml",
        "templates/hard.jinja",
        "templates/soft.jinja",
        "templates/private.jinja",
        "templates/world.jinja",
        "templates/notes.txt",
        "templates/secrets.jinja",
        "templates/missing.jinja",
        "templates/../legacy.jinja",
    ] {
        assert!(f.delete(path).is_err(), "should not delete {path}");
    }
    for path in [
        "legacy.jinja",
        "templates/regular.jinja",
        "templates/hard.jinja",
    ] {
        assert!(f.root.join(path).exists());
    }
    let path = "templates/changing.jinja";
    f.write(path, "before");
    let preview = f.delete(path).unwrap();
    f.write(path, "changed after review");
    assert!(preview.approve().execute(&f.host).is_err());
    assert_eq!(
        fs::read(f.root.join(path)).unwrap(),
        b"changed after review"
    );
}

#[test]
fn template_files_delete_cancel_writes_nothing() {
    let f = Fixture::with_templates();
    let path = "templates/unused.jinja";
    f.write(path, "keep");
    let preview = f.delete(path).unwrap();
    let removed = preview.review().removed.unwrap();
    drop(preview);
    assert_eq!(fs::read(f.root.join(path)).unwrap(), b"keep");
    assert!(!removed.parent().unwrap().exists());
    let config = LocalConfig::load(&f.host).unwrap();
    let state = repository_state_path(&f.host, config.repository_id(&f.root).unwrap()).unwrap();
    assert!(!state.join("template-edits").exists());
}

#[test]
fn template_files_delete_refuses_a_file_that_returns_at_the_last_moment() {
    let f = Fixture::with_templates();
    let path = "templates/unused.jinja";
    f.write(path, "reviewed");
    let preview = f.delete(path).unwrap();
    let removed = preview.review().removed.unwrap();
    let incoming = f.root.join("incoming");
    fs::write(&incoming, "racer").unwrap();
    assert!(
        preview
            .approve()
            .execute_before_publish(&f.host, || {
                fs::rename(&incoming, f.root.join(path)).unwrap();
            })
            .is_err()
    );
    assert_eq!(fs::read(f.root.join(path)).unwrap(), b"racer");
    assert!(!removed.exists());
}

#[test]
fn template_files_delete_never_overwrites_an_earlier_retained_copy() {
    let f = Fixture::with_templates();
    let path = "templates/unused.jinja";
    f.write(path, "one");
    let first = f.delete(path).unwrap();
    let Outcome::Deleted(first) = first.approve().execute(&f.host).unwrap() else {
        panic!("first delete should succeed")
    };
    f.write(path, "two");
    let second = f.delete(path).unwrap();
    let Outcome::Deleted(second) = second.approve().execute(&f.host).unwrap() else {
        panic!("second delete should succeed")
    };
    assert_ne!(first.path, second.path);
    assert_eq!(fs::read(&first.path).unwrap(), b"one");
    assert_eq!(fs::read(&second.path).unwrap(), b"two");
}
