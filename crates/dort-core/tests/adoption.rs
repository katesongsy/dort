mod support;
use dort_core::{
    Error, Manifest,
    adoption::{self, Approval, Boundary},
    context::HostContext,
    journal::{self, Phase},
    local::{RepositoryRegistry, repository_state_path},
    model::{FileMode, ManagedFile},
    planner::{self, Request},
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

const ORIGINAL: &str = "# comments must survive\nschema = 1 # inline too\n";
const CONTENT: &[u8] = b"set editor-mode safe\n";
fn mapping() -> ManagedFile {
    ManagedFile {
        id: "editor".into(),
        source: "files/common/editorrc".into(),
        target: "~/.editorrc".into(),
        mode: FileMode::Link,
        profiles: vec!["base".into()],
        requires: vec![],
        overrides: BTreeMap::new(),
        permissions: None,
    }
}
fn approval(host: &HostContext, repo: &Path) -> Approval {
    Approval::from_plan(planner::plan(
        planner::observe(host, repo, &[], Request::Adopt { mapping: mapping() }).unwrap(),
    ))
    .unwrap()
}
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    id: String,
    lock: WriterLock,
}
impl Fixture {
    fn new() -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repository");
        fs::write(repo.join("dort.toml"), ORIGINAL).unwrap();
        fs::write(home.host.home.join(".editorrc"), CONTENT).unwrap();
        fs::set_permissions(
            home.host.home.join(".editorrc"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        let lock = WriterLock::acquire(&home.host).unwrap();
        let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            id,
            lock,
        }
    }
    fn transaction(&self) -> String {
        journal::diagnose(&self.home.host).unwrap()[0]
            .transaction_id
            .clone()
    }
    fn txpath(&self) -> PathBuf {
        repository_state_path(&self.home.host, &self.id)
            .unwrap()
            .join("transactions")
            .join(self.transaction())
    }
    fn restored(&self) {
        assert_eq!(
            fs::read(self.home.host.home.join(".editorrc")).unwrap(),
            CONTENT
        );
        assert!(
            !fs::symlink_metadata(self.home.host.home.join(".editorrc"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            ORIGINAL
        );
        assert!(!self.repo.join("files").exists());
        assert!(
            journal::diagnose(&self.home.host)
                .unwrap()
                .iter()
                .all(|d| !d.blocks_writes())
        );
    }
}

#[test]
fn complete_adoption_retains_comments_modes_inodes_backups_and_explicitly_rolls_back() {
    let f = Fixture::new();
    let host = &f.home.host;
    let manifest_inode = fs::metadata(f.repo.join("dort.toml")).unwrap().ino();
    let destination_inode = fs::metadata(host.home.join(".editorrc")).unwrap().ino();
    let digest = approval(host, &f.repo).sha256().to_owned();
    let tx = adoption::execute(host, &f.lock, approval(host, &f.repo), |_| Ok(())).unwrap();
    assert_eq!(
        fs::read_link(host.home.join(".editorrc")).unwrap(),
        f.repo.join("files/common/editorrc")
    );
    assert_eq!(fs::read(host.home.join(".editorrc")).unwrap(), CONTENT);
    assert_eq!(
        fs::metadata(f.repo.join("files/common/editorrc"))
            .unwrap()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::metadata(f.repo.join("files/common")).unwrap().mode() & 0o777,
        0o700
    );
    let manifest = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
    assert!(manifest.starts_with(ORIGINAL));
    assert_eq!(Manifest::parse(&manifest).unwrap().files.len(), 1);
    let report = journal::diagnose(host).unwrap();
    assert_eq!(report[0].phase, Some(Phase::Completed));
    assert!(!report[0].blocks_writes());
    assert_eq!(
        fs::read(f.txpath().join("destination.backup")).unwrap(),
        CONTENT
    );
    assert_eq!(
        fs::metadata(f.txpath().join("destination.backup"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    let records: Vec<_> = fs::read_dir(f.txpath())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    assert!(records.len() > 10);
    for path in records {
        let record = fs::read_to_string(path).unwrap();
        assert!(!record.contains("set editor-mode safe"));
        assert!(record.contains(&digest));
    }
    adoption::rollback(host, &f.lock, &f.id, &tx, |_| Ok(())).unwrap();
    f.restored();
    assert_eq!(
        fs::metadata(f.repo.join("dort.toml")).unwrap().ino(),
        manifest_inode
    );
    assert_eq!(
        fs::metadata(host.home.join(".editorrc")).unwrap().ino(),
        destination_inode
    );
    adoption::rollback(host, &f.lock, &f.id, &tx, |_| Ok(())).unwrap();
}

fn successful_event_count() -> usize {
    let f = Fixture::new();
    let mut count = 0;
    adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |_| {
            count += 1;
            Ok(())
        },
    )
    .unwrap();
    count
}

#[test]
fn every_durable_checkpoint_and_post_mutation_error_can_be_explicitly_rolled_back() {
    for stop in 1..=successful_event_count() {
        let f = Fixture::new();
        let mut events = 0;
        let result = adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| {
                events += 1;
                if events == stop {
                    Err(Error::Invalid("injected failure".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        let tx = f.transaction();
        if journal::diagnose(&f.home.host).unwrap()[0].phase != Some(Phase::Completed) {
            assert!(RepositoryRegistry::register(&f.home.host, &f.lock, &f.repo).is_err());
        }
        adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(()))
            .unwrap_or_else(|e| panic!("boundary {stop}: {e}"));
        f.restored();
    }
}

#[test]
fn recovery_itself_is_restartable_at_every_checkpoint_and_mutation() {
    let f = Fixture::new();
    let mut count = 0;
    let tx = adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |_| Ok(()),
    )
    .unwrap();
    adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    for stop in 1..=count {
        let f = Fixture::new();
        let mut events = 0;
        let tx = adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| Ok(()),
        )
        .unwrap();
        assert!(
            adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| {
                events += 1;
                if events == stop {
                    Err(Error::Invalid("interrupted recovery".into()))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(()))
            .unwrap_or_else(|e| panic!("recovery boundary {stop}: {e}"));
        f.restored();
    }
}

#[test]
fn child_crash_probe() {
    let Some(home) = std::env::var_os("DORT_TEST_ADOPT_HOME") else {
        return;
    };
    let repo = PathBuf::from(std::env::var_os("DORT_TEST_ADOPT_REPO").unwrap());
    let stop: usize = std::env::var("DORT_TEST_ADOPT_STOP")
        .unwrap()
        .parse()
        .unwrap();
    let host = HostContext::new(
        home.into(),
        None,
        None,
        Platform::Ubuntu,
        Architecture::X86_64,
    )
    .unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let mut events = 0;
    adoption::execute(&host, &lock, approval(&host, &repo), |_| {
        events += 1;
        if events == stop {
            std::process::exit(77);
        }
        Ok(())
    })
    .unwrap();
    panic!("crash boundary never reached");
}

#[test]
fn process_exit_at_every_adoption_boundary_retains_recoverable_intent_and_releases_lock() {
    for stop in 1..=successful_event_count() {
        let f = Fixture::new();
        let home = f.home;
        let repo = f.repo;
        let id = f.id;
        drop(f.lock);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_crash_probe", "--nocapture"])
            .env_clear()
            .env("HOME", &home.host.home)
            .env("PATH", "/no-external-programs")
            .env("DORT_TEST_ADOPT_HOME", &home.host.home)
            .env("DORT_TEST_ADOPT_REPO", &repo)
            .env("DORT_TEST_ADOPT_STOP", stop.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(77),
            "boundary {stop}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let lock = WriterLock::acquire(&home.host).unwrap();
        let f = Fixture {
            home,
            repo,
            id,
            lock,
        };
        adoption::rollback(&f.home.host, &f.lock, &f.id, &f.transaction(), |_| Ok(()))
            .unwrap_or_else(|e| panic!("process boundary {stop}: {e}"));
        f.restored();
    }
}

#[test]
fn extra_user_files_in_created_directories_block_recovery_before_any_inverse() {
    let f = Fixture::new();
    let tx = adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |_| Ok(()),
    )
    .unwrap();
    fs::write(f.repo.join("files/common/user-note"), b"keep this").unwrap();
    let manifest = fs::read(f.repo.join("dort.toml")).unwrap();
    assert!(adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).is_err());
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), manifest);
    assert!(
        fs::symlink_metadata(f.home.host.home.join(".editorrc"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(f.repo.join("files/common/user-note")).unwrap(),
        b"keep this"
    );
}

#[test]
fn substituted_backup_links_are_not_followed_or_overwritten() {
    for hard_link in [false, true] {
        let f = Fixture::new();
        let tx = adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| Ok(()),
        )
        .unwrap();
        let foreign = f.repo.join("foreign-backup");
        fs::write(&foreign, b"foreign content").unwrap();
        let backup = f.txpath().join("destination.backup");
        fs::remove_file(&backup).unwrap();
        if hard_link {
            fs::hard_link(&foreign, &backup).unwrap();
        } else {
            symlink(&foreign, &backup).unwrap();
        }
        assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
        assert!(adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).is_err());
        assert_eq!(fs::read(foreign).unwrap(), b"foreign content");
        assert!(
            fs::symlink_metadata(f.home.host.home.join(".editorrc"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[test]
fn source_edits_block_rollback_before_any_other_effect() {
    let f = Fixture::new();
    let tx = adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |_| Ok(()),
    )
    .unwrap();
    fs::write(
        f.home.host.home.join(".editorrc"),
        b"user edits through live link",
    )
    .unwrap();
    let manifest = fs::read(f.repo.join("dort.toml")).unwrap();
    assert!(adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).is_err());
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), manifest);
    assert!(
        fs::symlink_metadata(f.home.host.home.join(".editorrc"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(f.repo.join("files/common/editorrc")).unwrap(),
        b"user edits through live link"
    );
}

#[test]
fn manifest_destination_and_parent_edits_are_all_guarded() {
    for case in 0..4 {
        let f = Fixture::new();
        let tx = adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| Ok(()),
        )
        .unwrap();
        match case {
            0 => {
                fs::write(f.repo.join("dort.toml"), "schema=1\n# user edit\n").unwrap();
            }
            1 => {
                fs::remove_file(f.home.host.home.join(".editorrc")).unwrap();
                fs::write(f.home.host.home.join(".editorrc"), b"foreign").unwrap();
            }
            2 => {
                fs::rename(f.repo.join("files"), f.repo.join("moved")).unwrap();
                symlink(f.repo.join("moved"), f.repo.join("files")).unwrap();
            }
            _ => {
                fs::set_permissions(
                    f.repo.join("files/common"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
        }
        assert!(adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).is_err());
        assert!(f.repo.join("files/common/editorrc").exists());
    }
}

#[test]
fn stale_approvals_and_parent_substitution_cannot_write() {
    for case in 0..4 {
        let f = Fixture::new();
        let approved = approval(&f.home.host, &f.repo);
        match case {
            0 => fs::write(f.home.host.home.join(".editorrc"), b"edited").unwrap(),
            1 => fs::write(f.repo.join("dort.toml"), "schema=1\n# changed\n").unwrap(),
            2 => {
                fs::create_dir(f.repo.join("files")).unwrap();
                symlink(f.home.host.home.clone(), f.repo.join("files/common")).unwrap();
            }
            _ => {
                fs::write(f.home.host.local_config_path(), "schema=99\n").unwrap();
            }
        }
        assert!(adoption::execute(&f.home.host, &f.lock, approved, |_| Ok(())).is_err());
        assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
        assert!(
            !fs::symlink_metadata(f.home.host.home.join(".editorrc"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[test]
fn malformed_tail_preserves_earlier_intent_and_blocks_registry_and_recovery() {
    let f = Fixture::new();
    let mut events = 0;
    assert!(
        adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| {
                events += 1;
                if events == 2 {
                    Err(Error::Invalid("stop".into()))
                } else {
                    Ok(())
                }
            }
        )
        .is_err()
    );
    let first = fs::read(f.txpath().join("000000.json")).unwrap();
    fs::write(f.txpath().join("000002.json"), b"{truncated").unwrap();
    fs::set_permissions(
        f.txpath().join("000002.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let reports = journal::diagnose(&f.home.host).unwrap();
    assert_eq!(reports[0].valid_checkpoints, 2);
    assert!(reports[0].problem.is_some());
    assert!(reports[0].blocks_writes());
    assert_eq!(fs::read(f.txpath().join("000000.json")).unwrap(), first);
    assert!(RepositoryRegistry::register(&f.home.host, &f.lock, &f.repo).is_err());
    assert!(
        adoption::rollback(&f.home.host, &f.lock, &f.id, &f.transaction(), |_| Ok(())).is_err()
    );
    assert!(!f.repo.join("files").exists());
}

#[test]
fn backup_corruption_refuses_rollback_and_does_not_touch_live_files() {
    let f = Fixture::new();
    let tx = adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |_| Ok(()),
    )
    .unwrap();
    fs::write(f.txpath().join("destination.backup"), b"tampered").unwrap();
    assert!(adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).is_err());
    assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
    assert!(RepositoryRegistry::register(&f.home.host, &f.lock, &f.repo).is_err());
    assert_eq!(
        fs::read(f.home.host.home.join(".editorrc")).unwrap(),
        CONTENT
    );
    assert!(
        fs::symlink_metadata(f.home.host.home.join(".editorrc"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn partial_private_preparation_can_be_abandoned_without_live_changes() {
    let f = Fixture::new();
    assert!(
        adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |event| {
                assert_eq!(event.boundary, Boundary::Checkpoint);
                Err(Error::Invalid("preparing".into()))
            }
        )
        .is_err()
    );
    fs::write(f.txpath().join("destination.backup"), b"partial").unwrap();
    fs::set_permissions(
        f.txpath().join("destination.backup"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    adoption::rollback(&f.home.host, &f.lock, &f.id, &f.transaction(), |_| Ok(())).unwrap();
    f.restored();
}

#[test]
fn unknown_schema_checksum_mismatch_gaps_and_unknown_artifacts_fail_closed() {
    use sha2::{Digest, Sha256};
    for case in 0..5 {
        let f = Fixture::new();
        adoption::execute(
            &f.home.host,
            &f.lock,
            approval(&f.home.host, &f.repo),
            |_| Ok(()),
        )
        .unwrap();
        let mut paths: Vec<_> = fs::read_dir(f.txpath())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        match case {
            0 | 1 | 4 => {
                let path = paths.last().unwrap();
                let mut envelope: serde_json::Value =
                    serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                if case == 1 {
                    envelope["sha256"] = "0".repeat(64).into();
                } else {
                    let mut payload: serde_json::Value =
                        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
                    if case == 0 {
                        payload["schema"] = 99.into();
                    } else {
                        payload["operations"][0]["progress"] = "pending".into();
                    }
                    let payload = serde_json::to_string(&payload).unwrap();
                    envelope["sha256"] = format!("{:x}", Sha256::digest(payload.as_bytes())).into();
                    envelope["payload"] = payload.into();
                }
                fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
            }
            2 => fs::remove_file(&paths[1]).unwrap(),
            _ => fs::write(f.txpath().join("unknown.tmp"), b"unrecognized").unwrap(),
        }
        let reports = journal::diagnose(&f.home.host).unwrap();
        assert!(reports[0].blocks_writes());
        assert!(reports[0].problem.is_some());
        assert!(RepositoryRegistry::register(&f.home.host, &f.lock, &f.repo).is_err());
        assert!(
            adoption::rollback(&f.home.host, &f.lock, &f.id, &f.transaction(), |_| Ok(())).is_err()
        );
        assert_eq!(
            fs::read(f.home.host.home.join(".editorrc")).unwrap(),
            CONTENT
        );
    }
}

#[test]
fn non_base_profiles_and_preexisting_source_parents_are_supported() {
    let f = Fixture::new();
    fs::write(
        f.repo.join("dort.toml"),
        "schema=1\n[profiles]\navailable=['dev']\n",
    )
    .unwrap();
    fs::create_dir_all(f.repo.join("files/common")).unwrap();
    let mut m = mapping();
    m.profiles = vec!["dev".into()];
    let plan = planner::plan(
        planner::observe(
            &f.home.host,
            &f.repo,
            &["dev".into()],
            Request::Adopt { mapping: m },
        )
        .unwrap(),
    );
    let tx = adoption::execute(
        &f.home.host,
        &f.lock,
        Approval::from_plan(plan).unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    adoption::rollback(&f.home.host, &f.lock, &f.id, &tx, |_| Ok(())).unwrap();
    assert!(f.repo.join("files/common").is_dir());
    assert!(!f.repo.join("files/common/editorrc").exists());
    assert_eq!(
        fs::read(f.home.host.home.join(".editorrc")).unwrap(),
        CONTENT
    );
}

#[test]
fn callback_edits_after_intent_are_rechecked_before_the_next_mutation() {
    let f = Fixture::new();
    let mut checkpoints = 0;
    let result = adoption::execute(
        &f.home.host,
        &f.lock,
        approval(&f.home.host, &f.repo),
        |event| {
            if event.boundary == Boundary::Checkpoint {
                checkpoints += 1;
            }
            if checkpoints == 3 {
                fs::write(f.home.host.home.join(".editorrc"), b"edited at intent").unwrap();
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!f.repo.join("files").exists());
    assert_eq!(
        fs::read(f.home.host.home.join(".editorrc")).unwrap(),
        b"edited at intent"
    );
}

#[test]
fn another_context_lock_or_an_unregistered_repository_cannot_authorize_adoption() {
    let f = support::TestHome::new();
    let repo = f.repository("repository");
    fs::write(f.host.home.join(".editorrc"), CONTENT).unwrap();
    let other = support::TestHome::new();
    let other_lock = WriterLock::acquire(&other.host).unwrap();
    assert!(adoption::execute(&f.host, &other_lock, approval(&f.host, &repo), |_| Ok(())).is_err());
    let lock = WriterLock::acquire(&f.host).unwrap();
    assert!(adoption::execute(&f.host, &lock, approval(&f.host, &repo), |_| Ok(())).is_err());
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn different_filesystem_state_has_no_cross_device_copy_fallback() {
    let Ok(state) = tempfile::tempdir_in("/dev/shm") else {
        return;
    };
    let f = support::TestHome::new();
    let repo = f.repository("repository");
    if fs::metadata(state.path()).unwrap().dev() == fs::metadata(&repo).unwrap().dev() {
        return;
    }
    let host = HostContext::new(
        f.host.home.clone(),
        None,
        Some(state.path().canonicalize().unwrap()),
        Platform::Ubuntu,
        Architecture::X86_64,
    )
    .unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let id = RepositoryRegistry::register(&host, &lock, &repo).unwrap();
    fs::write(host.home.join(".editorrc"), CONTENT).unwrap();
    assert!(adoption::execute(&host, &lock, approval(&host, &repo), |_| Ok(())).is_err());
    assert!(!repo.join("files").exists());
    let tx = journal::diagnose(&host).unwrap()[0].transaction_id.clone();
    adoption::rollback(&host, &lock, &id, &tx, |_| Ok(())).unwrap();
    assert_eq!(fs::read(host.home.join(".editorrc")).unwrap(), CONTENT);
}

#[test]
fn read_only_diagnosis_does_not_create_state() {
    let f = support::TestHome::new();
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
    assert!(!f.host.state_dir.exists());
}
