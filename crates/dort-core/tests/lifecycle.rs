mod support;
use dort_core::{
    Error, Manifest, adoption,
    context::HostContext,
    journal::{self, Phase},
    lifecycle::{self, Approval, Boundary, RemovalAcknowledgement},
    local::{RepositoryRegistry, repository_state_path},
    planner::{self, Request, UnmanageMode},
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

const ORIGINAL: &str = r#"# retain root comment
schema = 1 # retain inline comment

[[files]] # first mapping
id = "a"
source = "files/a"
target = "~/.a"
mode = "link"

[[files]] # removed only by unmanage b
id = "b"
source = "files/b"
target = "~/.config/editor/b"
mode = "link"

[[files]] # last mapping and comments retained
id = "c"
source = "files/c"
target = "~/.config/editor/c"
mode = "link"
"#;
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    id: String,
    lock: Option<WriterLock>,
    unmanage: Option<UnmanageMode>,
}
fn request(mode: Option<UnmanageMode>) -> Request {
    match mode {
        None => Request::Apply,
        Some(mode) => Request::Unmanage {
            id: "b".into(),
            mode,
        },
    }
}
fn approve(host: &HostContext, repo: &Path, mode: Option<UnmanageMode>) -> Approval {
    let plan = planner::plan(planner::observe(host, repo, &[], request(mode)).unwrap());
    if matches!(mode, Some(UnmanageMode::Remove)) {
        Approval::from_removal_plan(plan, RemovalAcknowledgement::confirm("b")).unwrap()
    } else {
        Approval::from_plan(plan).unwrap()
    }
}
impl Fixture {
    fn new(mode: Option<UnmanageMode>) -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), ORIGINAL).unwrap();
        fs::create_dir(repo.join("files")).unwrap();
        for name in ["a", "b", "c"] {
            fs::write(
                repo.join("files").join(name),
                format!("ordinary editor setting {name}\n"),
            )
            .unwrap();
        }
        fs::set_permissions(repo.join("files/b"), fs::Permissions::from_mode(0o640)).unwrap();
        fs::write(home.host.home.join(".a"), "original ordinary destination\n").unwrap();
        if mode.is_some() {
            fs::create_dir_all(home.host.home.join(".config/editor")).unwrap();
            // Relative correct links are accepted and restored exactly, not rewritten.
            symlink(
                repo.join("files/b"),
                home.host.home.join(".config/editor/b"),
            )
            .unwrap();
        }
        let lock = WriterLock::acquire(&home.host).unwrap();
        let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            id,
            lock: Some(lock),
            unmanage: mode,
        }
    }
    fn execute(
        &self,
        observer: impl FnMut(&lifecycle::Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<Option<String>> {
        lifecycle::execute(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            approve(&self.home.host, &self.repo, self.unmanage),
            observer,
        )
    }
    fn tx(&self) -> String {
        journal::diagnose(&self.home.host).unwrap()[0]
            .transaction_id
            .clone()
    }
    fn txpath(&self) -> PathBuf {
        repository_state_path(&self.home.host, &self.id)
            .unwrap()
            .join("transactions")
            .join(self.tx())
    }
    fn rollback(
        &self,
        observer: impl FnMut(&lifecycle::Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<()> {
        lifecycle::rollback(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            &self.id,
            &self.tx(),
            observer,
        )
    }
    fn restored(&self) {
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            ORIGINAL
        );
        assert_eq!(
            fs::read_to_string(self.home.host.home.join(".a")).unwrap(),
            "original ordinary destination\n"
        );
        let target = self.home.host.home.join(".config/editor/b");
        if self.unmanage.is_some() {
            assert_eq!(fs::read_link(target).unwrap(), self.repo.join("files/b"));
        } else {
            assert!(!self.home.host.home.join(".config/editor").exists());
        }
        assert!(!self.home.host.home.join(".config/editor/c").exists());
        for name in ["a", "b", "c"] {
            assert_eq!(
                fs::read_to_string(self.repo.join("files").join(name)).unwrap(),
                format!("ordinary editor setting {name}\n")
            );
        }
        assert!(
            journal::diagnose(&self.home.host)
                .unwrap()
                .iter()
                .all(|d| !d.blocks_writes())
        );
    }
}
fn modes() -> [Option<UnmanageMode>; 3] {
    [None, Some(UnmanageMode::Keep), Some(UnmanageMode::Remove)]
}
fn event_count(mode: Option<UnmanageMode>, rollback: bool) -> usize {
    let f = Fixture::new(mode);
    let mut count = 0;
    f.execute(|_| {
        if !rollback {
            count += 1;
        }
        Ok(())
    })
    .unwrap();
    if rollback {
        f.rollback(|_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    }
    count
}

#[test]
fn backup_write_failure_keeps_apply_keep_and_remove_live_inputs_unchanged() {
    for mode in modes() {
        for deny_writes in [false, true] {
            if deny_writes && rustix::process::geteuid().is_root() {
                eprintln!("SKIP permission-denied backup case: root bypasses mode bits");
                continue;
            }
            let f = Fixture::new(mode);
            let destination = f.home.host.home.join(".a");
            let inode = fs::metadata(&destination).unwrap().ino();
            let mut mutations = 0;
            let result = f.execute(|event| {
                if event.boundary == Boundary::Mutation {
                    mutations += 1;
                } else if event.sequence == 0 {
                    if deny_writes {
                        fs::set_permissions(f.txpath(), fs::Permissions::from_mode(0o500)).unwrap();
                    } else {
                        let path = f.txpath().join("backup-0000");
                        fs::write(&path, b"partial-private-backup").unwrap();
                        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
                    }
                }
                Ok(())
            });
            fs::set_permissions(f.txpath(), fs::Permissions::from_mode(0o700)).unwrap();
            assert!(matches!(result, Err(Error::Io { .. })), "{result:?}");
            assert_eq!(mutations, 0);
            assert_eq!(fs::metadata(destination).unwrap().ino(), inode);
            assert_eq!(
                fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
                ORIGINAL
            );
            if !deny_writes {
                assert_eq!(
                    fs::read(f.txpath().join("backup-0000")).unwrap(),
                    b"partial-private-backup"
                );
            }
            let diagnosis = journal::diagnose(&f.home.host).unwrap();
            assert_eq!(diagnosis[0].phase, Some(Phase::Preparing));
            assert!(diagnosis[0].blocks_writes());
            f.rollback(|_| Ok(())).unwrap();
            f.restored();
        }
    }
}

#[test]
fn apply_batches_shared_parents_backups_and_repeated_apply_is_a_true_noop() {
    let f = Fixture::new(None);
    let old_inode = fs::metadata(f.home.host.home.join(".a")).unwrap().ino();
    let source_inode = fs::metadata(f.repo.join("files/b")).unwrap().ino();
    let tx = f.execute(|_| Ok(())).unwrap().unwrap();
    let history = lifecycle::history(&f.home.host).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].transaction_id, tx);
    assert_eq!(history[0].phase, Some(Phase::Completed));
    assert_eq!(
        fs::read_link(f.home.host.home.join(".a")).unwrap(),
        f.repo.join("files/a")
    );
    assert_eq!(
        fs::metadata(f.home.host.home.join(".config/editor"))
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::read(f.txpath().join("backup-0000")).unwrap(),
        b"original ordinary destination\n"
    );
    assert_eq!(
        fs::metadata(f.txpath().join("backup-0000")).unwrap().mode() & 0o777,
        0o600
    );
    let names: Vec<_> = fs::read_dir(f.txpath())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        f.execute(|_| panic!("no-op must not emit mutation/checkpoint events"))
            .unwrap()
            .is_none()
    );
    assert_eq!(lifecycle::history(&f.home.host).unwrap().len(), 1);
    assert_eq!(fs::read_dir(f.txpath()).unwrap().count(), names.len());
    f.rollback(|_| Ok(())).unwrap();
    f.restored();
    assert_eq!(
        fs::metadata(f.home.host.home.join(".a")).unwrap().ino(),
        old_inode
    );
    assert_eq!(
        fs::metadata(f.repo.join("files/b")).unwrap().ino(),
        source_inode
    );
    f.rollback(|_| Ok(())).unwrap();
}

#[test]
fn keep_and_remove_order_destination_before_manifest_and_never_change_source() {
    for mode in [UnmanageMode::Keep, UnmanageMode::Remove] {
        let f = Fixture::new(Some(mode));
        let source = f.repo.join("files/b");
        let source_meta = fs::metadata(&source).unwrap();
        let manifest_ino = fs::metadata(f.repo.join("dort.toml")).unwrap().ino();
        let link_ino = fs::symlink_metadata(f.home.host.home.join(".config/editor/b"))
            .unwrap()
            .ino();
        f.execute(|event| {
            if event.boundary == Boundary::Mutation {
                let text = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
                if text == ORIGINAL {
                    let target = f.home.host.home.join(".config/editor/b");
                    match mode {
                        UnmanageMode::Keep => {
                            assert!(fs::symlink_metadata(&target).unwrap().is_file());
                            assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o640);
                        }
                        UnmanageMode::Remove => assert!(fs::symlink_metadata(target).is_err()),
                    }
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::metadata(&source).unwrap().ino(), source_meta.ino());
        assert_eq!(
            fs::metadata(&source).unwrap().ctime_nsec(),
            source_meta.ctime_nsec()
        );
        let text = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
        assert!(text.starts_with("# retain root comment\nschema = 1 # retain inline comment"));
        assert!(text.contains("# last mapping and comments retained"));
        assert_eq!(
            Manifest::parse(&text)
                .unwrap()
                .files
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        f.rollback(|_| Ok(())).unwrap();
        f.restored();
        assert_eq!(
            fs::metadata(f.repo.join("dort.toml")).unwrap().ino(),
            manifest_ino
        );
        assert_eq!(
            fs::symlink_metadata(f.home.host.home.join(".config/editor/b"))
                .unwrap()
                .ino(),
            link_ino
        );
    }
}

#[test]
fn removal_requires_separate_matching_acknowledgement_and_tampered_previews_are_rejected() {
    let f = Fixture::new(Some(UnmanageMode::Remove));
    let plan =
        planner::plan(planner::observe(&f.home.host, &f.repo, &[], request(f.unmanage)).unwrap());
    assert!(Approval::from_plan(plan.clone()).is_err());
    assert!(
        Approval::from_removal_plan(plan.clone(), RemovalAcknowledgement::confirm("a")).is_err()
    );
    let mut changed = plan.clone();
    changed.files[0].effects.clear();
    assert!(Approval::from_removal_plan(changed, RemovalAcknowledgement::confirm("b")).is_err());
    assert!(Approval::from_removal_plan(plan, RemovalAcknowledgement::confirm("b")).is_ok());
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
}

#[test]
fn every_forward_boundary_failure_is_diagnosable_blocks_writes_and_can_be_rolled_back() {
    for mode in modes() {
        for stop in 1..=event_count(mode, false) {
            let f = Fixture::new(mode);
            let mut count = 0;
            assert!(
                f.execute(|_| {
                    count += 1;
                    if count == stop {
                        Err(Error::Invalid("injected boundary".into()))
                    } else {
                        Ok(())
                    }
                })
                .is_err(),
                "mode {mode:?} stop {stop}"
            );
            assert_eq!(count, stop);
            if journal::diagnose(&f.home.host).unwrap()[0].blocks_writes() {
                assert!(
                    RepositoryRegistry::register(&f.home.host, f.lock.as_ref().unwrap(), &f.repo)
                        .is_err()
                );
            }
            f.rollback(|_| Ok(()))
                .unwrap_or_else(|e| panic!("mode {mode:?}, stop {stop}: {e}"));
            f.restored();
        }
    }
}
#[test]
fn every_rollback_boundary_failure_is_restartable() {
    for mode in modes() {
        for stop in 1..=event_count(mode, true) {
            let f = Fixture::new(mode);
            f.execute(|_| Ok(())).unwrap();
            let mut count = 0;
            assert!(
                f.rollback(|_| {
                    count += 1;
                    if count == stop {
                        Err(Error::Invalid("injected inverse".into()))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            f.rollback(|_| Ok(()))
                .unwrap_or_else(|e| panic!("mode {mode:?}, inverse {stop}: {e}"));
            f.restored();
        }
    }
}

#[test]
fn edits_to_any_watched_source_or_unchanged_mapping_refuse_the_whole_rollback() {
    for mode in modes() {
        let f = Fixture::new(mode);
        f.execute(|_| Ok(())).unwrap();
        let manifest = fs::read(f.repo.join("dort.toml")).unwrap();
        let checkpoints = fs::read_dir(f.txpath()).unwrap().count();
        let edited = if mode.is_none() {
            f.home.host.home.join(".config/editor/b")
        } else {
            f.repo.join("files/b")
        };
        fs::write(edited, "later user editor settings\n").unwrap();
        assert!(f.rollback(|_| Ok(())).is_err());
        assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), manifest);
        assert_eq!(fs::read_dir(f.txpath()).unwrap().count(), checkpoints);
    }
    let f = Fixture::new(None);
    fs::remove_file(f.home.host.home.join(".a")).unwrap();
    symlink(f.repo.join("files/a"), f.home.host.home.join(".a")).unwrap();
    f.execute(|_| Ok(())).unwrap();
    fs::remove_file(f.home.host.home.join(".a")).unwrap();
    fs::write(f.home.host.home.join(".a"), "later independent edit").unwrap();
    assert!(f.rollback(|_| Ok(())).is_err());
    assert!(fs::read_link(f.home.host.home.join(".config/editor/b")).is_ok());
}

#[test]
fn stale_approvals_and_callback_substitution_stop_before_any_live_mutation() {
    for which in ["source", "manifest", "destination", "config", "parent"] {
        let f = Fixture::new(Some(UnmanageMode::Keep));
        let approved = approve(&f.home.host, &f.repo, f.unmanage);
        match which {
            "source" => fs::write(f.repo.join("files/b"), "later source").unwrap(),
            "manifest" => fs::write(
                f.repo.join("dort.toml"),
                format!("{ORIGINAL}\n# user edit\n"),
            )
            .unwrap(),
            "destination" => {
                fs::remove_file(f.home.host.home.join(".config/editor/b")).unwrap();
                fs::write(f.home.host.home.join(".config/editor/b"), "user file").unwrap();
            }
            "config" => {
                let p = f.home.host.local_config_path();
                let bytes = fs::read(&p).unwrap();
                fs::write(p, bytes).unwrap();
            }
            "parent" => {
                let p = f.home.host.home.join(".config/editor");
                fs::rename(&p, p.with_file_name("saved")).unwrap();
                symlink(p.with_file_name("saved"), &p).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            lifecycle::execute(&f.home.host, f.lock.as_ref().unwrap(), approved, |_| Ok(()))
                .is_err(),
            "{which}"
        );
        assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    }
    let f = Fixture::new(None);
    let mut mutations = 0;
    assert!(
        f.execute(|e| {
            if e.boundary == Boundary::Checkpoint && e.sequence == 2 {
                fs::write(f.repo.join("files/c"), "user edit after intent").unwrap();
            }
            if e.boundary == Boundary::Mutation {
                mutations += 1;
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(mutations, 0);
    assert_eq!(
        fs::read_to_string(f.home.host.home.join(".a")).unwrap(),
        "original ordinary destination\n"
    );
    assert!(f.rollback(|_| Ok(())).is_err());
}

#[test]
fn later_files_in_created_dirs_or_materialized_file_edits_are_not_overwritten() {
    let f = Fixture::new(None);
    f.execute(|_| Ok(())).unwrap();
    fs::write(
        f.home.host.home.join(".config/editor/unrelated"),
        "later unrelated edit",
    )
    .unwrap();
    assert!(f.rollback(|_| Ok(())).is_err());
    assert!(fs::read_link(f.home.host.home.join(".a")).is_ok());
    assert_eq!(
        fs::read_to_string(f.home.host.home.join(".config/editor/unrelated")).unwrap(),
        "later unrelated edit"
    );
    for mode in [UnmanageMode::Keep, UnmanageMode::Remove] {
        let f = Fixture::new(Some(mode));
        f.execute(|_| Ok(())).unwrap();
        fs::write(
            f.home.host.home.join(".config/editor/b"),
            "user replacement content",
        )
        .unwrap();
        assert!(f.rollback(|_| Ok(())).is_err());
        assert!(
            !Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap())
                .unwrap()
                .files
                .iter()
                .any(|m| m.id == "b")
        );
    }
}

#[test]
fn malformed_checkpoint_backup_and_unknown_artifact_fail_closed_without_touching_live_files() {
    for corruption in [
        "truncated",
        "checksum",
        "schema",
        "gap",
        "progress",
        "artifact",
        "backup",
        "backup-symlink",
        "backup-hardlink",
    ] {
        let f = Fixture::new(None);
        f.execute(|_| Ok(())).unwrap();
        let path = f.txpath();
        let before = fs::read_link(f.home.host.home.join(".a")).unwrap();
        let mut records: Vec<_> = fs::read_dir(&path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        records.sort();
        let last = records.last().unwrap();
        match corruption {
            "truncated" => fs::write(last, "{").unwrap(),
            "gap" => fs::remove_file(&records[1]).unwrap(),
            "checksum" | "schema" | "progress" => {
                let mut env: serde_json::Value =
                    serde_json::from_slice(&fs::read(last).unwrap()).unwrap();
                if corruption == "checksum" {
                    env["sha256"] = serde_json::json!("0".repeat(64));
                } else {
                    let mut payload: serde_json::Value =
                        serde_json::from_str(env["payload"].as_str().unwrap()).unwrap();
                    if corruption == "schema" {
                        payload["schema"] = serde_json::json!(999);
                    } else {
                        payload["operations"][0]["progress"] = serde_json::json!("pending");
                    }
                    let encoded = serde_json::to_string(&payload).unwrap();
                    use sha2::{Digest, Sha256};
                    env["sha256"] =
                        serde_json::json!(format!("{:x}", Sha256::digest(encoded.as_bytes())));
                    env["payload"] = serde_json::json!(encoded);
                }
                fs::write(last, serde_json::to_vec(&env).unwrap()).unwrap();
            }
            "artifact" => fs::write(path.join("unexpected"), "untrusted").unwrap(),
            "backup" => fs::write(path.join("backup-0000"), "damaged").unwrap(),
            "backup-symlink" => {
                fs::remove_file(path.join("backup-0000")).unwrap();
                symlink(f.home.host.home.join(".a"), path.join("backup-0000")).unwrap();
            }
            "backup-hardlink" => {
                fs::hard_link(path.join("backup-0000"), path.join("copy")).unwrap()
            }
            _ => unreachable!(),
        }
        assert!(
            journal::diagnose(&f.home.host).unwrap()[0].blocks_writes(),
            "{corruption}"
        );
        assert!(f.rollback(|_| Ok(())).is_err(), "{corruption}");
        assert!(
            RepositoryRegistry::register(&f.home.host, f.lock.as_ref().unwrap(), &f.repo).is_err()
        );
        assert_eq!(fs::read_link(f.home.host.home.join(".a")).unwrap(), before);
    }
}

#[test]
fn ordinary_unrelated_edits_survive_successful_rollback() {
    for mode in modes() {
        let f = Fixture::new(mode);
        f.execute(|_| Ok(())).unwrap();
        fs::write(f.home.host.home.join("unrelated"), "later unrelated data").unwrap();
        f.rollback(|_| Ok(())).unwrap();
        f.restored();
        assert_eq!(
            fs::read_to_string(f.home.host.home.join("unrelated")).unwrap(),
            "later unrelated data"
        );
    }
}

#[test]
fn inline_manifest_files_array_is_focused_and_source_aliases_are_preserved() {
    let f = Fixture::new(Some(UnmanageMode::Keep));
    let text = "schema=1 # keep\nfiles = [ # inline mappings\n {id='b',source='files/b',target='~/.config/editor/b',mode='link'},\n {id='other',source='files/b',target='~/.other',mode='link'},\n]\n";
    fs::write(f.repo.join("dort.toml"), text).unwrap();
    f.execute(|_| Ok(())).unwrap();
    let new = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
    assert!(new.contains("# inline mappings"));
    assert_eq!(Manifest::parse(&new).unwrap().files[0].id, "other");
    f.rollback(|_| Ok(())).unwrap();
    assert_eq!(fs::read_to_string(f.repo.join("dort.toml")).unwrap(), text);
}

#[test]
fn schema1_adoption_history_and_rollback_remain_compatible() {
    let f = Fixture::new(None);
    let mut mapping = Manifest::parse(ORIGINAL).unwrap().files[0].clone();
    mapping.id = "new".into();
    mapping.source = "files/new".into();
    mapping.target = "~/.new".into();
    fs::write(f.home.host.home.join(".new"), "ordinary adopted content").unwrap();
    let p = adoption::Approval::from_plan(planner::plan(
        planner::observe(&f.home.host, &f.repo, &[], Request::Adopt { mapping }).unwrap(),
    ))
    .unwrap();
    let tx = adoption::execute(&f.home.host, f.lock.as_ref().unwrap(), p, |_| Ok(())).unwrap();
    f.execute(|_| Ok(())).unwrap();
    let h = lifecycle::history(&f.home.host).unwrap();
    assert_eq!(h.len(), 2);
    assert!(h.iter().all(|d| !d.blocks_writes()));
    let apply = h.iter().find(|d| d.transaction_id != tx).unwrap();
    lifecycle::rollback(
        &f.home.host,
        f.lock.as_ref().unwrap(),
        &f.id,
        &apply.transaction_id,
        |_| Ok(()),
    )
    .unwrap();
    adoption::rollback(&f.home.host, f.lock.as_ref().unwrap(), &f.id, &tx, |_| {
        Ok(())
    })
    .unwrap();
    f.restored();
}

#[test]
fn empty_and_already_correct_apply_create_no_transaction_storage() {
    for empty in [false, true] {
        let f = Fixture::new(None);
        if empty {
            fs::write(f.repo.join("dort.toml"), "schema=1\n").unwrap();
        } else {
            fs::remove_file(f.home.host.home.join(".a")).unwrap();
            fs::create_dir_all(f.home.host.home.join(".config/editor")).unwrap();
            let relative = f
                .repo
                .strip_prefix(f.home.host.home.parent().unwrap())
                .unwrap()
                .join("files/a");
            symlink(
                PathBuf::from("..").join(relative),
                f.home.host.home.join(".a"),
            )
            .unwrap();
            for name in ["b", "c"] {
                symlink(
                    f.repo.join("files").join(name),
                    f.home.host.home.join(".config/editor").join(name),
                )
                .unwrap();
            }
        }
        let state = repository_state_path(&f.home.host, &f.id).unwrap();
        assert!(!state.exists());
        assert!(f.execute(|_| panic!("no-op event")).unwrap().is_none());
        assert!(!state.exists());
    }
}

#[test]
fn profile_and_platform_sources_are_resolved_before_execution_and_retained() {
    let f = Fixture::new(None);
    let text = ORIGINAL
        .replace(
            "schema = 1 # retain inline comment",
            "schema = 1\n[profiles]\navailable=['dev']\ndefault=['dev']",
        )
        .replace(
            "source = \"files/b\"",
            "source = \"files/b\"\nprofiles=['dev']\noverrides={ubuntu='files/c'}",
        );
    fs::write(f.repo.join("dort.toml"), &text).unwrap();
    f.execute(|_| Ok(())).unwrap();
    assert_eq!(
        fs::read_link(f.home.host.home.join(".config/editor/b")).unwrap(),
        f.repo.join("files/c")
    );
    f.rollback(|_| Ok(())).unwrap();
    assert_eq!(fs::read_to_string(f.repo.join("dort.toml")).unwrap(), text);
    assert_eq!(
        fs::read_to_string(f.repo.join("files/c")).unwrap(),
        "ordinary editor setting c\n"
    );
}

#[test]
fn unsupported_sensitive_and_unsafe_inputs_never_reach_approval() {
    for condition in ["template", "sensitive", "hardlink", "foreign-link"] {
        let f = Fixture::new(None);
        match condition {
            "template" => fs::write(
                f.repo.join("dort.toml"),
                ORIGINAL.replace("mode = \"link\"", "mode = \"template\""),
            )
            .unwrap(),
            "sensitive" => {
                fs::write(f.repo.join("files/b"), "password=not-a-real-password\n").unwrap()
            }
            "hardlink" => fs::hard_link(f.repo.join("files/b"), f.repo.join("other-link")).unwrap(),
            "foreign-link" => {
                symlink("/no/such/foreign/file", f.home.host.home.join(".foreign")).unwrap()
            }
            _ => unreachable!(),
        }
        if condition == "foreign-link" {
            fs::remove_file(f.home.host.home.join(".a")).unwrap();
            symlink(".foreign", f.home.host.home.join(".a")).unwrap();
        }
        let plan =
            planner::plan(planner::observe(&f.home.host, &f.repo, &[], Request::Apply).unwrap());
        assert!(Approval::from_plan(plan).is_err(), "{condition}");
        assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    }
}

#[test]
fn substitutions_after_intent_and_source_edits_during_rollback_are_guarded() {
    for condition in ["parent", "stage", "backup"] {
        let f = Fixture::new(Some(UnmanageMode::Keep));
        let mut mutations = 0;
        let result = f.execute(|e| {
            if e.boundary == Boundary::Checkpoint && e.sequence == 2 {
                match condition {
                    "parent" => {
                        let parent = f.home.host.home.join(".config/editor");
                        fs::rename(&parent, parent.with_file_name("saved")).unwrap();
                        symlink(parent.with_file_name("saved"), &parent).unwrap();
                    }
                    "stage" => {
                        fs::remove_file(f.txpath().join("op-0000")).unwrap();
                        symlink(f.repo.join("files/b"), f.txpath().join("op-0000")).unwrap();
                    }
                    "backup" => fs::write(f.txpath().join("backup-0000"), "damaged").unwrap(),
                    _ => unreachable!(),
                }
            }
            if e.boundary == Boundary::Mutation {
                mutations += 1;
            }
            Ok(())
        });
        assert!(result.is_err(), "{condition}");
        assert_eq!(mutations, 0);
        assert_eq!(
            fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
            ORIGINAL
        );
        assert!(f.rollback(|_| Ok(())).is_err());
    }
    let f = Fixture::new(None);
    f.execute(|_| Ok(())).unwrap();
    let mut mutations = 0;
    assert!(
        f.rollback(|e| {
            if e.boundary == Boundary::Checkpoint {
                fs::write(f.repo.join("files/b"), "edited during rollback").unwrap();
            }
            if e.boundary == Boundary::Mutation {
                mutations += 1;
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(mutations, 0);
    assert!(fs::read_link(f.home.host.home.join(".a")).is_ok());
}

#[test]
fn checkpoints_and_history_never_embed_file_contents_and_history_never_creates_state() {
    let bare = support::TestHome::new();
    assert!(lifecycle::history(&bare.host).unwrap().is_empty());
    assert!(!bare.host.state_dir.exists());
    let f = Fixture::new(None);
    f.execute(|_| Ok(())).unwrap();
    for file in fs::read_dir(f.txpath()).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = fs::read_to_string(path).unwrap();
            assert!(!text.contains("ordinary editor setting"));
            assert!(!text.contains("original ordinary destination"));
        }
    }
    let count = fs::read_dir(f.txpath()).unwrap().count();
    fs::write(f.repo.join("files/b"), "password=not-a-real-password\n").unwrap();
    let history = lifecycle::history(&f.home.host).unwrap();
    assert_eq!(history[0].journal_schema, Some(2));
    assert!(!history[0].blocks_writes()); // history diagnoses journals, not live drift
    assert!(
        !serde_json::to_string(&history)
            .unwrap()
            .contains("password=")
    );
    assert_eq!(fs::read_dir(f.txpath()).unwrap().count(), count);
}

#[test]
fn wrong_lock_and_unregistered_repository_are_refused_before_journalling() {
    let f = Fixture::new(None);
    let other = support::TestHome::new();
    let wrong_lock = WriterLock::acquire(&other.host).unwrap();
    assert!(
        lifecycle::execute(
            &f.home.host,
            &wrong_lock,
            approve(&f.home.host, &f.repo, None),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    let unregistered = f.home.repository("unregistered");
    fs::write(unregistered.join("dort.toml"), ORIGINAL).unwrap();
    fs::create_dir(unregistered.join("files")).unwrap();
    for name in ["a", "b", "c"] {
        fs::write(
            unregistered.join("files").join(name),
            "ordinary unregistered content",
        )
        .unwrap();
    }
    assert!(
        lifecycle::execute(
            &f.home.host,
            f.lock.as_ref().unwrap(),
            approve(&f.home.host, &unregistered, None),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn separate_filesystems_are_refused_without_live_mutations_when_available() {
    let Some(other) = support::cross_device_dir() else {
        return;
    };
    let home = support::TestHome::new();
    let repo = other.path().canonicalize().unwrap().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    if fs::metadata(&repo).unwrap().dev() == fs::metadata(&home.host.home).unwrap().dev() {
        eprintln!("SKIP cross-device case: fixture devices are identical");
        return;
    }
    fs::write(
        repo.join("dort.toml"),
        "schema=1\n[[files]]\nid='a'\nsource='a'\ntarget='~/.a'\nmode='link'\n",
    )
    .unwrap();
    fs::write(repo.join("a"), "ordinary source").unwrap();
    let lock = WriterLock::acquire(&home.host).unwrap();
    let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
    assert!(
        lifecycle::execute(&home.host, &lock, approve(&home.host, &repo, None), |_| Ok(
            ()
        ))
        .is_err()
    );
    assert!(!home.host.home.join(".a").exists());
    let history = lifecycle::history(&home.host).unwrap();
    assert_eq!(history[0].phase, Some(Phase::Preparing));
    lifecycle::rollback(&home.host, &lock, &id, &history[0].transaction_id, |_| {
        Ok(())
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("a")).unwrap(),
        "ordinary source"
    );
}

#[test]
fn child_crash_probe() {
    let Some(home) = std::env::var_os("DORT_LIFECYCLE_HOME") else {
        return;
    };
    let host = HostContext::new(
        PathBuf::from(home),
        None,
        None,
        Platform::Ubuntu,
        Architecture::X86_64,
    )
    .unwrap();
    let repo = PathBuf::from(std::env::var_os("DORT_LIFECYCLE_REPO").unwrap());
    let mode = match std::env::var("DORT_LIFECYCLE_MODE").unwrap().as_str() {
        "apply" => None,
        "keep" => Some(UnmanageMode::Keep),
        _ => Some(UnmanageMode::Remove),
    };
    let lock = WriterLock::acquire(&host).unwrap();
    let stop = std::env::var("DORT_LIFECYCLE_STOP")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut count = 0;
    let callback = |_: &lifecycle::Event| {
        count += 1;
        if count == stop {
            std::process::exit(77);
        }
        Ok(())
    };
    if std::env::var_os("DORT_LIFECYCLE_UNDO").is_some() {
        lifecycle::rollback(
            &host,
            &lock,
            &std::env::var("DORT_LIFECYCLE_ID").unwrap(),
            &std::env::var("DORT_LIFECYCLE_TX").unwrap(),
            callback,
        )
        .unwrap();
    } else {
        lifecycle::execute(&host, &lock, approve(&host, &repo, mode), callback).unwrap();
    }
    panic!("did not reach process interruption");
}
#[test]
fn process_exit_at_every_forward_and_inverse_boundary_releases_lock_and_recovers() {
    for mode in modes() {
        for undo in [false, true] {
            for stop in 1..=event_count(mode, undo) {
                let mut f = Fixture::new(mode);
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
                    .env("DORT_LIFECYCLE_HOME", &f.home.host.home)
                    .env("DORT_LIFECYCLE_REPO", &f.repo)
                    .env(
                        "DORT_LIFECYCLE_MODE",
                        match mode {
                            None => "apply",
                            Some(UnmanageMode::Keep) => "keep",
                            _ => "remove",
                        },
                    )
                    .env("DORT_LIFECYCLE_STOP", stop.to_string());
                if undo {
                    cmd.env("DORT_LIFECYCLE_UNDO", "1")
                        .env("DORT_LIFECYCLE_ID", &f.id)
                        .env("DORT_LIFECYCLE_TX", &tx);
                }
                let out = cmd.output().unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(77),
                    "{mode:?} undo {undo} stop {stop}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                f.lock = Some(WriterLock::acquire(&f.home.host).unwrap());
                f.rollback(|_| Ok(()))
                    .unwrap_or_else(|e| panic!("{mode:?} undo {undo} stop {stop}: {e}"));
                f.restored();
            }
        }
    }
}
