mod support;
use dort_core::{
    Error, Manifest,
    adoption::Event,
    context::HostContext,
    journal,
    local::RepositoryRegistry,
    model::Tool,
    platform::{Architecture, Platform},
    recovery,
    state::WriterLock,
    tools::declaration::{self, Approval},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
};
const ORIGINAL: &str = "# PRIVATE_MANIFEST_COMMENT\nschema = 1 # keep formatting\n\n[[files]]\nid='keep'\nsource='keep'\ntarget='~/.keep'\nmode='link'\n[files.overrides]\nubuntu='keep-ubuntu'\n";
fn tool() -> Tool {
    Manifest::parse("schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install.macos]\nprovider='brew'\npackage='demo'\n[tools.install.ubuntu]\nprovider='apt'\npackage='demo'\n").unwrap().tools.remove(0)
}
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    id: String,
    lock: Option<WriterLock>,
}
impl Fixture {
    fn new() -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), ORIGINAL).unwrap();
        fs::write(repo.join("keep"), "untouched source").unwrap();
        fs::write(repo.join("keep-ubuntu"), "untouched override").unwrap();
        fs::write(home.host.home.join(".keep"), "untouched live file").unwrap();
        let lock = WriterLock::acquire(&home.host).unwrap();
        let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            id,
            lock: Some(lock),
        }
    }
    fn approve(&self) -> Approval {
        Approval::from_preview(
            declaration::preview(&self.home.host, &self.repo, &[], tool()).unwrap(),
        )
        .unwrap()
    }
    fn execute(
        &self,
        cb: impl FnMut(&Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<Option<String>> {
        declaration::execute(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            self.approve(),
            cb,
        )
    }
    fn tx(&self) -> String {
        journal::diagnose(&self.home.host).unwrap()[0]
            .transaction_id
            .clone()
    }
    fn undo(&self, cb: impl FnMut(&Event) -> dort_core::Result<()>) -> dort_core::Result<()> {
        let p = recovery::preview(&self.home.host, &self.id, &self.tx())?;
        recovery::execute_with_observer(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            recovery::Approval::from_preview(p)?,
            cb,
        )
    }
    fn restored(&self) {
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            ORIGINAL
        );
        assert_eq!(
            fs::read_to_string(self.home.host.home.join(".keep")).unwrap(),
            "untouched live file"
        );
        assert_eq!(
            fs::read_to_string(self.repo.join("keep")).unwrap(),
            "untouched source"
        );
        assert!(!journal::diagnose(&self.home.host).unwrap()[0].blocks_writes());
    }
}
#[test]
fn focused_edit_preserves_comments_other_tables_and_permissions_noop_and_rollback() {
    let f = Fixture::new();
    fs::set_permissions(f.repo.join("dort.toml"), fs::Permissions::from_mode(0o640)).unwrap();
    let preview = declaration::preview(&f.home.host, &f.repo, &[], tool()).unwrap();
    let report = serde_json::to_string(&preview).unwrap();
    assert!(!report.contains("PRIVATE_MANIFEST_COMMENT"));
    assert!(!preview.no_op);
    let tx = declaration::execute(
        &f.home.host,
        f.lock.as_ref().unwrap(),
        Approval::from_preview(preview).unwrap(),
        |_| Ok(()),
    )
    .unwrap()
    .unwrap();
    let text = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
    assert!(text.contains(ORIGINAL.trim_end()));
    assert_eq!(
        fs::metadata(f.repo.join("dort.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(Manifest::parse(&text).unwrap().tools.len(), 1);
    let d = journal::diagnose(&f.home.host).unwrap();
    assert_eq!(d[0].journal_schema, Some(5));
    assert_eq!(d[0].phase, Some(journal::Phase::Completed));
    assert_eq!(f.execute(|_| Ok(())).unwrap(), None);
    assert_eq!(journal::diagnose(&f.home.host).unwrap().len(), 1);
    let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
        .unwrap()
        .join("transactions")
        .join(tx);
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.path().extension().is_some_and(|e| e == "json") {
            assert!(
                !fs::read_to_string(e.path())
                    .unwrap()
                    .contains("PRIVATE_MANIFEST_COMMENT")
            );
        }
    }
    f.undo(|_| Ok(())).unwrap();
    f.restored();
}
#[test]
fn multiple_additions_and_inline_array_preserve_all_prior_semantics() {
    for original in ["# keep\nschema=1\ntools=[]\n", "# keep\nschema=1\n"] {
        let f = Fixture::new();
        fs::write(f.repo.join("dort.toml"), original).unwrap();
        f.execute(|_| Ok(())).unwrap();
        let before = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
        let mut next = tool();
        next.id = "second".into();
        next.binary = "second".into();
        next.requires = vec!["demo".into()];
        let p = declaration::preview(&f.home.host, &f.repo, &[], next).unwrap();
        declaration::execute(
            &f.home.host,
            f.lock.as_ref().unwrap(),
            Approval::from_preview(p).unwrap(),
            |_| Ok(()),
        )
        .unwrap();
        let after = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
        assert!(after.contains("# keep"));
        assert_eq!(Manifest::parse(&after).unwrap().tools.len(), 2);
        let d = journal::diagnose(&f.home.host).unwrap();
        let last = d
            .iter()
            .find(|d| recovery::preview(&f.home.host, &f.id, &d.transaction_id).is_ok())
            .unwrap();
        let p = recovery::preview(&f.home.host, &f.id, &last.transaction_id).unwrap();
        recovery::execute(
            &f.home.host,
            f.lock.as_ref().unwrap(),
            recovery::Approval::from_preview(p).unwrap(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
            before
        );
    }
}
#[test]
fn unregistered_invalid_conflicting_modified_and_stale_inputs_never_allocate_intent() {
    let h = support::TestHome::new();
    let repo = h.repository("unregistered");
    assert!(declaration::preview(&h.host, &repo, &[], tool()).is_err());
    assert!(!h.host.state_dir.exists());
    let f = Fixture::new();
    for mutate in [0, 1, 2] {
        let mut t = tool();
        match mutate {
            0 => t.binary = "../unsafe".into(),
            1 => t.requires.push("missing".into()),
            _ => t.profiles = vec!["unknown".into()],
        }
        assert!(declaration::preview(&f.home.host, &f.repo, &[], t).is_err());
    }
    let mut p = declaration::preview(&f.home.host, &f.repo, &[], tool()).unwrap();
    p.tool.binary = "changed".into();
    assert!(Approval::from_preview(p).is_err());
    let a = f.approve();
    fs::write(
        f.repo.join("dort.toml"),
        format!("{ORIGINAL}\n# later edit"),
    )
    .unwrap();
    assert!(declaration::execute(&f.home.host, f.lock.as_ref().unwrap(), a, |_| Ok(())).is_err());
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    f.execute(|_| Ok(())).unwrap();
    let mut t = tool();
    t.binary = "different".into();
    assert!(declaration::preview(&f.home.host, &f.repo, &[], t).is_err());
    assert_eq!(journal::diagnose(&f.home.host).unwrap().len(), 1);
}
#[test]
fn unsafe_manifest_and_parent_or_later_edits_cannot_be_overwritten() {
    for case in 0..4 {
        let f = Fixture::new();
        let path = f.repo.join("dort.toml");
        match case {
            0 => {
                fs::rename(&path, f.repo.join("real")).unwrap();
                symlink("real", &path).unwrap();
            }
            1 => fs::hard_link(&path, f.repo.join("extra")).unwrap(),
            2 => fs::set_permissions(&path, fs::Permissions::from_mode(0o4644)).unwrap(),
            _ => {
                fs::remove_file(&path).unwrap();
                fs::create_dir(&path).unwrap();
            }
        }
        assert!(declaration::preview(&f.home.host, &f.repo, &[], tool()).is_err());
    }
    let f = Fixture::new();
    f.execute(|_| Ok(())).unwrap();
    fs::write(f.repo.join("dort.toml"), "schema=1\n# user edit").unwrap();
    assert!(f.undo(|_| Ok(())).is_err());
    assert!(
        fs::read_to_string(f.repo.join("dort.toml"))
            .unwrap()
            .contains("user edit")
    );
}
fn count(undo: bool) -> usize {
    let f = Fixture::new();
    let mut n = 0;
    if undo {
        f.execute(|_| Ok(())).unwrap();
        f.undo(|_| {
            n += 1;
            Ok(())
        })
        .unwrap();
    } else {
        f.execute(|_| {
            n += 1;
            Ok(())
        })
        .unwrap();
    }
    n
}
#[test]
fn every_forward_inverse_failure_boundary_recovers_and_blocks_other_writers() {
    for undo in [false, true] {
        for stop in 1..=count(undo) {
            let f = Fixture::new();
            if undo {
                f.execute(|_| Ok(())).unwrap();
            }
            let mut n = 0;
            let cb = |_: &Event| {
                n += 1;
                if n == stop {
                    Err(Error::Invalid("fixture failure".into()))
                } else {
                    Ok(())
                }
            };
            assert!(
                if undo {
                    f.undo(cb).map(|_| None)
                } else {
                    f.execute(cb)
                }
                .is_err()
            );
            if journal::diagnose(&f.home.host).unwrap()[0].blocks_writes() {
                assert!(
                    RepositoryRegistry::register(&f.home.host, f.lock.as_ref().unwrap(), &f.repo)
                        .is_err()
                );
                assert!(f.execute(|_| Ok(())).is_err());
            }
            f.undo(|_| Ok(()))
                .unwrap_or_else(|e| panic!("undo={undo} stop={stop}: {e}"));
            f.restored();
        }
    }
}
#[test]
fn malformed_tail_or_backup_blocks_all_writes_and_never_replays() {
    for backup in [false, true] {
        let f = Fixture::new();
        f.execute(|_| Ok(())).unwrap();
        let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
            .unwrap()
            .join("transactions")
            .join(f.tx());
        if backup {
            fs::write(dir.join("manifest.backup"), b"changed").unwrap();
        } else {
            let next = journal::diagnose(&f.home.host).unwrap()[0].valid_checkpoints;
            fs::write(dir.join(format!("{next:06}.json")), b"{").unwrap();
        }
        assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
        assert!(f.undo(|_| Ok(())).is_err());
        assert!(f.execute(|_| Ok(())).is_err());
        assert_eq!(
            Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap())
                .unwrap()
                .tools
                .len(),
            1
        );
    }
}
#[test]
fn preparation_collision_and_changed_live_manifest_fail_without_overwrite() {
    for preparing in [true, false] {
        let f = Fixture::new();
        let mut injected = false;
        let result = f.execute(|event| {
            if !injected && event.sequence == usize::from(!preparing) {
                injected = true;
                if preparing {
                    let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)?
                        .join("transactions")
                        .join(&event.transaction_id);
                    fs::write(dir.join("manifest.backup"), "fixture collision").unwrap();
                } else {
                    fs::write(f.repo.join("dort.toml"), "schema=1\n# later edit\n").unwrap();
                }
            }
            Ok(())
        });
        assert!(injected && result.is_err());
        assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
        if preparing {
            f.undo(|_| Ok(())).unwrap();
            f.restored();
        } else {
            assert!(f.undo(|_| Ok(())).is_err());
            assert_eq!(
                fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
                "schema=1\n# later edit\n"
            );
        }
    }
}
#[test]
fn checksum_valid_changed_declaration_is_not_rollback_authority() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    f.execute(|_| Ok(())).unwrap();
    let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
        .unwrap()
        .join("transactions")
        .join(f.tx());
    let count = journal::diagnose(&f.home.host).unwrap()[0].valid_checkpoints;
    let mut previous = serde_json::Value::Null;
    for i in 0..count {
        let path = dir.join(format!("{i:06}.json"));
        let mut e: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut r: serde_json::Value =
            serde_json::from_str(e["payload"].as_str().unwrap()).unwrap();
        r["header"]["tool"]["binary"] = "substituted".into();
        r["previous_sha256"] = previous;
        let payload = serde_json::to_string(&r).unwrap();
        let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        e["payload"] = payload.into();
        e["sha256"] = hash.clone().into();
        fs::write(path, serde_json::to_vec(&e).unwrap()).unwrap();
        previous = hash.into();
    }
    assert!(
        journal::diagnose(&f.home.host).unwrap()[0]
            .problem
            .is_none()
    );
    assert!(f.undo(|_| Ok(())).is_err());
    assert_eq!(
        Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap())
            .unwrap()
            .tools[0]
            .binary,
        "demo"
    );
}
#[test]
fn cross_device_manifest_refuses_exchange_and_preparation_can_be_closed() {
    let Some(other) = support::cross_device_dir() else {
        return;
    };
    let mut f = Fixture::new();
    f.home.host.state_dir = other.path().canonicalize().unwrap().join("state");
    drop(f.lock.take());
    f.lock = Some(WriterLock::acquire(&f.home.host).unwrap());
    assert!(f.execute(|_| Ok(())).is_err());
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        ORIGINAL
    );
    assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
    f.undo(|_| Ok(())).unwrap();
    f.restored();
}
#[test]
fn child_crash_probe() {
    let Some(home) = std::env::var_os("DORT_DECLARATION_HOME") else {
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
    let repo = PathBuf::from(std::env::var_os("DORT_DECLARATION_REPO").unwrap());
    let lock = WriterLock::acquire(&host).unwrap();
    let stop: usize = std::env::var("DORT_DECLARATION_STOP")
        .unwrap()
        .parse()
        .unwrap();
    let mut n = 0;
    let cb = |_: &Event| {
        n += 1;
        if n == stop {
            std::process::exit(77);
        }
        Ok(())
    };
    if let Ok(tx) = std::env::var("DORT_DECLARATION_TX") {
        let p =
            recovery::preview(&host, &std::env::var("DORT_DECLARATION_ID").unwrap(), &tx).unwrap();
        recovery::execute_with_observer(
            &host,
            &lock,
            recovery::Approval::from_preview(p).unwrap(),
            cb,
        )
        .unwrap();
    } else {
        let p = declaration::preview(&host, &repo, &[], tool()).unwrap();
        declaration::execute(&host, &lock, Approval::from_preview(p).unwrap(), cb).unwrap();
    }
    panic!("boundary not reached");
}
#[test]
fn process_exit_at_every_forward_and_inverse_boundary_releases_lock_and_recovers() {
    for undo in [false, true] {
        for stop in 1..=count(undo) {
            let mut f = Fixture::new();
            let tx = if undo {
                f.execute(|_| Ok(())).unwrap().unwrap()
            } else {
                String::new()
            };
            drop(f.lock.take());
            let mut cmd = Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", "child_crash_probe", "--nocapture"])
                .env_clear()
                .env("PATH", "/no-external-programs")
                .env("HOME", &f.home.host.home)
                .env("DORT_DECLARATION_HOME", &f.home.host.home)
                .env("DORT_DECLARATION_REPO", &f.repo)
                .env("DORT_DECLARATION_STOP", stop.to_string());
            if undo {
                cmd.env("DORT_DECLARATION_TX", tx)
                    .env("DORT_DECLARATION_ID", &f.id);
            }
            let out = cmd.output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(77),
                "{undo} {stop}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            f.lock = Some(WriterLock::acquire(&f.home.host).unwrap());
            f.undo(|_| Ok(())).unwrap();
            f.restored();
        }
    }
}
