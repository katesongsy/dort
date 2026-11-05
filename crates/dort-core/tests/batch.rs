mod support;
use dort_core::{
    Error, Manifest,
    adoption::Event,
    batch::{self, Action, Approval, RemovalAcknowledgements, Request},
    context::HostContext,
    journal,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{fs, os::unix::fs::symlink, path::PathBuf, process::Command};
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    id: String,
    lock: Option<WriterLock>,
    action: Action,
    original: String,
}
fn mappings() -> Vec<dort_core::model::ManagedFile> {
    Manifest::parse("schema=1\n[[files]]\nid='a'\nsource='files/shared/a'\ntarget='~/.a'\nmode='link'\n[[files]]\nid='b'\nsource='files/shared/b'\ntarget='~/.b'\nmode='link'\n[[files]]\nid='c'\nsource='files/c'\ntarget='~/.c'\nmode='link'\n").unwrap().files
}
fn request(action: Action) -> Request {
    let ids = vec!["a".into(), "b".into()];
    match action {
        Action::Adopt => Request::Adopt(mappings().into_iter().take(2).collect()),
        Action::Apply => Request::Apply(ids),
        Action::Keep => Request::Keep(ids),
        Action::Remove => Request::Remove(ids),
    }
}
fn approve(host: &HostContext, repo: &std::path::Path, action: Action) -> Approval {
    let plan = batch::plan(batch::observe(host, repo, &[], request(action)).unwrap());
    if action == Action::Remove {
        Approval::from_removal_plan(
            plan,
            RemovalAcknowledgements::confirm(vec!["b".into(), "a".into()]),
        )
        .unwrap()
    } else {
        Approval::from_plan(plan).unwrap()
    }
}
impl Fixture {
    fn new(action: Action) -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        let original = if action == Action::Adopt {
            "# retain comment\nschema=1\nfiles=[]\n".to_owned()
        } else {
            {
                let mut manifest = Manifest::parse("schema=1").unwrap();
                manifest.files = mappings();
                toml::to_string(&manifest).unwrap()
            }
        };
        fs::write(repo.join("dort.toml"), &original).unwrap();
        if action != Action::Adopt {
            fs::create_dir_all(repo.join("files/shared")).unwrap();
        }
        for name in ["a", "b", "c"] {
            let path = home.host.home.join(format!(".{name}"));
            if action == Action::Adopt || action == Action::Apply {
                fs::write(&path, format!("ordinary {name}\n")).unwrap();
            }
            if action != Action::Adopt {
                let source = if name == "c" {
                    repo.join("files/c")
                } else {
                    repo.join(format!("files/shared/{name}"))
                };
                fs::write(&source, format!("ordinary {name}\n")).unwrap();
                if matches!(action, Action::Keep | Action::Remove) {
                    symlink(source, path).unwrap();
                }
            }
        }
        let lock = WriterLock::acquire(&home.host).unwrap();
        let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            id,
            lock: Some(lock),
            action,
            original,
        }
    }
    fn execute(
        &self,
        callback: impl FnMut(&Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<Option<String>> {
        batch::execute(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            approve(&self.home.host, &self.repo, self.action),
            callback,
        )
    }
    fn tx(&self) -> String {
        journal::diagnose(&self.home.host).unwrap()[0]
            .transaction_id
            .clone()
    }
    fn undo(&self, cb: impl FnMut(&Event) -> dort_core::Result<()>) -> dort_core::Result<()> {
        batch::rollback(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            &self.id,
            &self.tx(),
            cb,
        )
    }
    fn restored(&self) {
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            self.original
        );
        for name in ["a", "b"] {
            let path = self.home.host.home.join(format!(".{name}"));
            if matches!(self.action, Action::Keep | Action::Remove) {
                assert!(path.is_symlink());
            } else {
                assert!(!path.is_symlink());
                assert_eq!(
                    fs::read_to_string(path).unwrap(),
                    format!("ordinary {name}\n")
                );
            }
        }
        if self.action == Action::Adopt {
            assert!(!self.repo.join("files").exists());
        }
        assert!(
            journal::diagnose(&self.home.host)
                .unwrap()
                .iter()
                .all(|d| !d.blocks_writes())
        );
    }
}
fn actions() -> [Action; 4] {
    [Action::Adopt, Action::Apply, Action::Keep, Action::Remove]
}
#[test]
fn combined_batches_are_one_transaction_selected_scope_and_guarded_recovery() {
    for action in actions() {
        let f = Fixture::new(action);
        let tx = f.execute(|_| Ok(())).unwrap().unwrap();
        let history = journal::diagnose(&f.home.host).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].journal_schema, Some(4));
        assert!(!history[0].blocks_writes());
        if action == Action::Apply {
            assert!(!f.home.host.home.join(".c").is_symlink());
            assert!(
                f.execute(|_| panic!("no-op allocates nothing"))
                    .unwrap()
                    .is_none()
            );
        }
        if matches!(action, Action::Keep | Action::Remove) {
            let manifest =
                Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap()).unwrap();
            assert_eq!(manifest.files.len(), 1);
            assert_eq!(manifest.files[0].id, "c");
        }
        let p = dort_core::recovery::preview(&f.home.host, &f.id, &tx).unwrap();
        assert_eq!(p.journal_schema, 4);
        dort_core::recovery::execute(
            &f.home.host,
            f.lock.as_ref().unwrap(),
            dort_core::recovery::Approval::from_preview(p).unwrap(),
        )
        .unwrap();
        f.restored();
    }
}
#[test]
fn every_forward_inverse_error_boundary_is_restartable_and_blocks_new_writes() {
    for action in actions() {
        for undo in [false, true] {
            let count = event_count(action, undo);
            for stop in 1..=count {
                let f = Fixture::new(action);
                if undo {
                    f.execute(|_| Ok(())).unwrap();
                }
                let mut n = 0;
                let cb = |_: &Event| {
                    n += 1;
                    if n == stop {
                        Err(Error::Invalid("injected stop".into()))
                    } else {
                        Ok(())
                    }
                };
                let result = if undo {
                    f.undo(cb).map(|_| None)
                } else {
                    f.execute(cb)
                };
                assert!(result.is_err(), "{action:?} {undo} {stop}");
                if journal::diagnose(&f.home.host)
                    .unwrap()
                    .iter()
                    .any(|d| d.blocks_writes())
                {
                    assert!(
                        RepositoryRegistry::register(
                            &f.home.host,
                            f.lock.as_ref().unwrap(),
                            &f.repo
                        )
                        .is_err()
                    );
                }
                f.undo(|_| Ok(()))
                    .unwrap_or_else(|e| panic!("{action:?} {undo} {stop}: {e}"));
                f.restored();
            }
        }
    }
}
fn event_count(action: Action, undo: bool) -> usize {
    let f = Fixture::new(action);
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
fn combined_conflicts_blocked_items_acknowledgements_and_stale_approval_fail_closed() {
    let f = Fixture::new(Action::Remove);
    let p = || {
        batch::plan(batch::observe(&f.home.host, &f.repo, &[], request(Action::Remove)).unwrap())
    };
    assert!(Approval::from_plan(p()).is_err());
    for ids in [
        vec!["a".into()],
        vec!["a".into(), "a".into()],
        vec!["a".into(), "b".into(), "c".into()],
    ] {
        assert!(Approval::from_removal_plan(p(), RemovalAcknowledgements::confirm(ids)).is_err());
    }
    let approved = approve(&f.home.host, &f.repo, Action::Remove);
    fs::write(f.repo.join("files/shared/a"), "later edit").unwrap();
    assert!(batch::execute(&f.home.host, f.lock.as_ref().unwrap(), approved, |_| Ok(())).is_err());
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    let f = Fixture::new(Action::Adopt);
    let mut m = mappings();
    m.truncate(2);
    m[1].source = m[0].source.join("child");
    assert!(batch::observe(&f.home.host, &f.repo, &[], Request::Adopt(m)).is_err());
    assert!(
        batch::observe(
            &f.home.host,
            &f.repo,
            &[],
            Request::Adopt(vec![mappings()[0].clone(), mappings()[0].clone()])
        )
        .is_err()
    );
}
#[test]
fn source_or_created_directory_edits_refuse_entire_rollback() {
    for action in actions() {
        let f = Fixture::new(action);
        f.execute(|_| Ok(())).unwrap();
        fs::write(f.repo.join("files/shared/a"), "later editor change").unwrap();
        assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
        assert!(f.undo(|_| Ok(())).is_err());
    }
    let f = Fixture::new(Action::Adopt);
    f.execute(|_| Ok(())).unwrap();
    fs::write(
        f.repo.join("files/shared/unapproved"),
        "later directory entry",
    )
    .unwrap();
    assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
    assert!(f.undo(|_| Ok(())).is_err());
    assert!(f.home.host.home.join(".a").is_symlink());
    assert!(f.repo.join("files/shared/unapproved").is_file());
}
#[test]
fn mixed_noop_watch_and_parent_substitution_refuse_before_inverse_or_next_mutation() {
    let f = Fixture::new(Action::Apply);
    fs::remove_file(f.home.host.home.join(".a")).unwrap();
    symlink(f.repo.join("files/shared/a"), f.home.host.home.join(".a")).unwrap();
    f.execute(|_| Ok(())).unwrap();
    // The unchanged selected mapping is still watched during whole-batch rollback.
    fs::write(f.repo.join("files/shared/a"), "later source edit").unwrap();
    assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
    assert!(f.undo(|_| Ok(())).is_err());
    assert!(f.home.host.home.join(".b").is_symlink());

    let f = Fixture::new(Action::Adopt);
    let mut substituted = false;
    let result = f.execute(|event| {
        if !substituted && matches!(event.boundary, dort_core::adoption::Boundary::Mutation) {
            substituted = true;
            // Replace the just-created parent with a symlink. No next live mutation
            // may traverse it, and recovery must not guess which directory to edit.
            fs::rename(f.repo.join("files"), f.repo.join("retained-files")).unwrap();
            symlink(f.repo.join("retained-files"), f.repo.join("files")).unwrap();
        }
        Ok(())
    });
    assert!(substituted && result.is_err());
    assert!(!f.home.host.home.join(".a").is_symlink());
    assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
    assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
    assert!(f.undo(|_| Ok(())).is_err());
}
#[test]
fn batch_planning_is_pure_content_free_bounded_and_creates_no_state() {
    let home = support::TestHome::new();
    let repo = home.repository("repo");
    for name in ["a", "b"] {
        fs::write(
            home.host.home.join(format!(".{name}")),
            "PRIVATE_BODY_NEVER_IN_REPORT",
        )
        .unwrap();
    }
    let snapshot = batch::observe(&home.host, &repo, &[], request(Action::Adopt)).unwrap();
    let first = batch::plan(snapshot.clone());
    let second = batch::plan(snapshot);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    assert!(!first.human().contains("PRIVATE_BODY_NEVER_IN_REPORT"));
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("PRIVATE_BODY_NEVER_IN_REPORT")
    );
    assert!(!home.host.state_dir.exists() && !home.host.config_dir.exists());
    assert!(batch::observe(&home.host, &repo, &[], Request::Adopt(vec![])).is_err());
    assert!(
        batch::observe(
            &home.host,
            &repo,
            &[],
            Request::Adopt(vec![mappings()[0].clone(); 33])
        )
        .is_err()
    );
    assert!(!home.host.state_dir.exists() && !home.host.config_dir.exists());
    let mut modified = first.clone();
    modified.items[0].files[0].effects.clear();
    assert!(Approval::from_plan(modified).is_err());
}
#[test]
fn malformed_tail_or_backup_blocks_writes_and_recovery_without_ignoring_valid_prefix() {
    for backup in [false, true] {
        let f = Fixture::new(Action::Adopt);
        f.execute(|_| Ok(())).unwrap();
        let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
            .unwrap()
            .join("transactions")
            .join(f.tx());
        if backup {
            fs::write(dir.join("backup-0000"), b"damaged").unwrap();
        } else {
            let next = journal::diagnose(&f.home.host).unwrap()[0].valid_checkpoints;
            fs::write(dir.join(format!("{next:06}.json")), b"{").unwrap();
        }
        let d = journal::diagnose(&f.home.host).unwrap();
        assert!(d[0].blocks_writes() && d[0].problem.is_some());
        assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
        assert!(f.undo(|_| Ok(())).is_err());
        assert!(f.home.host.home.join(".a").is_symlink());
        assert!(
            RepositoryRegistry::register(&f.home.host, f.lock.as_ref().unwrap(), &f.repo).is_err()
        );
    }
}
#[test]
fn valid_checksum_chain_is_not_authorization_for_changed_batch_scope() {
    let f = Fixture::new(Action::Keep);
    f.execute(|_| Ok(())).unwrap();
    let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
        .unwrap()
        .join("transactions")
        .join(f.tx());
    let count = journal::diagnose(&f.home.host).unwrap()[0].valid_checkpoints;
    let mut previous = serde_json::Value::Null;
    use sha2::{Digest, Sha256};
    for i in 0..count {
        let path = dir.join(format!("{i:06}.json"));
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        payload["header"]["profiles"] = serde_json::json!(["base", "undeclared"]);
        payload["previous_sha256"] = previous;
        let payload = serde_json::to_string(&payload).unwrap();
        let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        envelope["payload"] = payload.into();
        envelope["sha256"] = hash.clone().into();
        fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        previous = hash.into();
    }
    assert!(
        journal::diagnose(&f.home.host).unwrap()[0]
            .problem
            .is_none()
    );
    assert!(dort_core::recovery::preview(&f.home.host, &f.id, &f.tx()).is_err());
    assert!(f.undo(|_| Ok(())).is_err());
    assert!(f.home.host.home.join(".a").is_file() && !f.home.host.home.join(".a").is_symlink());
}
#[test]
fn child_crash_probe() {
    let Some(home) = std::env::var_os("DORT_BATCH_HOME") else {
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
    let repo = PathBuf::from(std::env::var_os("DORT_BATCH_REPO").unwrap());
    let action = actions()[std::env::var("DORT_BATCH_ACTION")
        .unwrap()
        .parse::<usize>()
        .unwrap()];
    let lock = WriterLock::acquire(&host).unwrap();
    let stop = std::env::var("DORT_BATCH_STOP")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut n = 0;
    let cb = |_: &Event| {
        n += 1;
        if n == stop {
            std::process::exit(77);
        }
        Ok(())
    };
    if let Ok(tx) = std::env::var("DORT_BATCH_TX") {
        batch::rollback(
            &host,
            &lock,
            &std::env::var("DORT_BATCH_ID").unwrap(),
            &tx,
            cb,
        )
        .unwrap();
    } else {
        batch::execute(&host, &lock, approve(&host, &repo, action), cb).unwrap();
    }
    panic!("boundary not reached");
}
#[test]
fn process_exit_at_every_forward_and_inverse_boundary_releases_lock_and_recovers() {
    for (index, action) in actions().into_iter().enumerate() {
        for undo in [false, true] {
            for stop in 1..=event_count(action, undo) {
                let mut f = Fixture::new(action);
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
                    .env("DORT_BATCH_HOME", &f.home.host.home)
                    .env("DORT_BATCH_REPO", &f.repo)
                    .env("DORT_BATCH_ACTION", index.to_string())
                    .env("DORT_BATCH_STOP", stop.to_string());
                if undo {
                    cmd.env("DORT_BATCH_TX", tx).env("DORT_BATCH_ID", &f.id);
                }
                let out = cmd.output().unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(77),
                    "{action:?} {undo} {stop}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                f.lock = Some(WriterLock::acquire(&f.home.host).unwrap());
                f.undo(|_| Ok(()))
                    .unwrap_or_else(|e| panic!("{action:?} {undo} {stop}: {e}"));
                f.restored();
            }
        }
    }
}
