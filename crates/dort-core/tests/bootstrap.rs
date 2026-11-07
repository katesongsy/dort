mod support;
use dort_core::{
    Error,
    bootstrap::{self, RecoveryAction},
    context::HostContext,
    journal,
    local::LocalConfig,
    state::WriterLock,
};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
use support::TestHome;

fn git() -> Option<PathBuf> {
    [
        "/usr/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}
fn destination(t: &TestHome) -> PathBuf {
    t.sandbox
        .path()
        .canonicalize()
        .unwrap()
        .join("new-repository")
}
fn approval(t: &TestHome, git: &Path) -> bootstrap::Approval {
    bootstrap::Approval::from_preview(bootstrap::preview(&t.host, &destination(t), git).unwrap())
        .unwrap()
}
fn recover(t: &TestHome, rid: &str, tid: &str, action: RecoveryAction) {
    let p = bootstrap::recovery_preview(&t.host, rid, tid, action).unwrap();
    let a = bootstrap::RecoveryApproval::from_preview(p).unwrap();
    bootstrap::recover(&t.host, &WriterLock::acquire(&t.host).unwrap(), a, |_| {
        Ok(())
    })
    .unwrap();
}
fn assert_complete(t: &TestHome) {
    let root = destination(t);
    assert_eq!(
        std::fs::read(root.join("dort.toml")).unwrap(),
        b"schema = 1\n"
    );
    assert!(root.join(".git/HEAD").is_file());
    assert!(
        !root.join(".git/hooks").exists(),
        "templates must be disabled"
    );
    let c = LocalConfig::load(&t.host).unwrap();
    assert_eq!(
        c.repositories[c.active_repository.as_ref().unwrap()].root,
        root
    );
    assert!(
        journal::diagnose(&t.host)
            .unwrap()
            .iter()
            .all(|d| !d.blocks_writes())
    );
}
#[test]
fn preview_has_no_effects_and_creation_then_attach_preserves_sources() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let t = TestHome::new();
    let root = destination(&t);
    let p = bootstrap::preview(&t.host, &root, &git).unwrap();
    assert!(p.create);
    assert!(!t.host.state_dir.exists());
    assert!(!t.host.config_dir.exists());
    assert!(!root.exists());
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        bootstrap::Approval::from_preview(p).unwrap(),
        &git,
        |_| Ok(()),
    )
    .unwrap();
    assert_complete(&t);
    let manifest = std::fs::metadata(root.join("dort.toml")).unwrap();
    let p = bootstrap::preview(&t.host, &root, &git).unwrap();
    assert!(!p.create);
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        bootstrap::Approval::from_preview(p).unwrap(),
        &git,
        |_| Ok(()),
    )
    .unwrap();
    assert_complete(&t);
    assert_eq!(
        manifest.ino(),
        std::fs::metadata(root.join("dort.toml")).unwrap().ino()
    );
    assert_eq!(
        std::fs::metadata(t.host.local_config_path())
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}
#[test]
fn every_bootstrap_boundary_recovers_explicitly_and_blocks_new_writes() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let baseline = TestHome::new();
    let mut count = 0;
    bootstrap::execute(
        &baseline.host,
        &WriterLock::acquire(&baseline.host).unwrap(),
        approval(&baseline, &git),
        &git,
        |_| {
            count += 1;
            Ok(())
        },
    )
    .unwrap();
    assert!(count >= 7);
    for stop in 0..count {
        let t = TestHome::new();
        let a = approval(&t, &git);
        let mut at = 0;
        let result = bootstrap::execute(
            &t.host,
            &WriterLock::acquire(&t.host).unwrap(),
            a,
            &git,
            |_| {
                let fail = at == stop;
                at += 1;
                if fail {
                    Err(Error::Invalid("injected".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        let history = journal::diagnose(&t.host).unwrap();
        assert_eq!(history.len(), 1);
        let d = &history[0];
        assert!(d.problem.is_none(), "stop={stop}: {:?}", d.problem);
        if !d.blocks_writes() {
            assert_complete(&t);
            continue;
        }
        // No startup replay or lock creation during diagnosis/preview.
        let fresh = TestHome::new();
        let unrelated = fresh.sandbox.path().canonicalize().unwrap().join("other");
        let a = bootstrap::Approval::from_preview(
            bootstrap::preview(&t.host, &unrelated, &git).unwrap(),
        )
        .unwrap();
        assert!(
            bootstrap::execute(
                &t.host,
                &WriterLock::acquire(&t.host).unwrap(),
                a,
                &git,
                |_| Ok(())
            )
            .is_err()
        );
        if d.phase == Some(journal::Phase::Preparing) {
            assert!(
                bootstrap::recovery_preview(
                    &t.host,
                    &d.repository_id,
                    &d.transaction_id,
                    RecoveryAction::Resume
                )
                .is_err()
            );
            recover(
                &t,
                &d.repository_id,
                &d.transaction_id,
                RecoveryAction::Abandon,
            );
            assert!(!destination(&t).exists());
        } else {
            recover(
                &t,
                &d.repository_id,
                &d.transaction_id,
                RecoveryAction::Resume,
            );
            assert_complete(&t);
        }
    }
}
#[test]
fn approval_tampering_stale_inputs_unsafe_paths_and_missing_git_are_refused() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let t = TestHome::new();
    let mut p = bootstrap::preview(&t.host, &destination(&t), &git).unwrap();
    p.effects.clear();
    assert!(bootstrap::Approval::from_preview(p).is_err());
    let a = approval(&t, &git);
    std::fs::create_dir(destination(&t)).unwrap();
    assert!(
        bootstrap::execute(
            &t.host,
            &WriterLock::acquire(&t.host).unwrap(),
            a,
            &git,
            |_| Ok(())
        )
        .is_err()
    );
    assert!(journal::diagnose(&t.host).unwrap().is_empty());
    assert!(bootstrap::preview(&t.host, &t.host.home, &git).is_err());
    assert!(bootstrap::preview(&t.host, &destination(&t).join("missing/child"), &git).is_err());
    let unsafe_parent = t.host.home.join("writable");
    std::fs::create_dir(&unsafe_parent).unwrap();
    std::fs::set_permissions(&unsafe_parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(bootstrap::preview(&t.host, &unsafe_parent.join("repo"), &git).is_err());
    let alias = t.host.home.join("alias");
    std::os::unix::fs::symlink(t.sandbox.path().canonicalize().unwrap(), &alias).unwrap();
    assert!(bootstrap::preview(&t.host, &alias.join("repo"), &git).is_err());
    let t = TestHome::new();
    let a = approval(&t, &git);
    assert!(
        bootstrap::execute(
            &t.host,
            &WriterLock::acquire(&t.host).unwrap(),
            a,
            Path::new("/no-such-git"),
            |_| Ok(())
        )
        .is_err()
    );
    let h = journal::diagnose(&t.host).unwrap();
    assert!(h[0].blocks_writes());
    recover(
        &t,
        &h[0].repository_id,
        &h[0].transaction_id,
        RecoveryAction::Abandon,
    );
}
fn interrupted_published(t: &TestHome, git: &Path) -> (String, String) {
    let a = approval(t, git);
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        a,
        git,
        |e| {
            if e.boundary == dort_core::adoption::Boundary::Mutation {
                Err(Error::Invalid("stop".into()))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    let d = journal::diagnose(&t.host).unwrap().remove(0);
    (d.repository_id, d.transaction_id)
}
#[test]
fn published_repository_is_never_deleted_and_edits_block_recovery() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let t = TestHome::new();
    let (rid, tid) = interrupted_published(&t, &git);
    let ino = std::fs::metadata(destination(&t)).unwrap().ino();
    recover(&t, &rid, &tid, RecoveryAction::Abandon);
    assert_eq!(ino, std::fs::metadata(destination(&t)).unwrap().ino());
    assert!(destination(&t).join("dort.toml").exists());
    assert!(LocalConfig::load(&t.host).unwrap().repositories.is_empty());
    let t = TestHome::new();
    let (rid, tid) = interrupted_published(&t, &git);
    let p = bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).unwrap();
    let a = bootstrap::RecoveryApproval::from_preview(p).unwrap();
    std::fs::write(destination(&t).join("dort.toml"), "schema=1\n# edited\n").unwrap();
    assert!(
        bootstrap::recover(&t.host, &WriterLock::acquire(&t.host).unwrap(), a, |_| Ok(
            ()
        ))
        .is_err()
    );
    assert!(bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Abandon).is_err());
    assert_eq!(
        std::fs::read_to_string(destination(&t).join("dort.toml")).unwrap(),
        "schema=1\n# edited\n"
    );
}
#[test]
fn config_values_stay_private_and_backup_corruption_or_parent_substitution_blocks_resume() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let t = TestHome::new();
    // First register, then prepare a second creation against an existing configuration.
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        approval(&t, &git),
        &git,
        |_| Ok(()),
    )
    .unwrap();
    let config = t.host.local_config_path();
    let text = std::fs::read_to_string(&config).unwrap().replace(
        "[variables]",
        "[variables]\nprivate_value = 'DO-NOT-REPORT-THIS'",
    );
    std::fs::write(&config, text).unwrap();
    let root = t.host.home.join("second");
    let p = bootstrap::preview(&t.host, &root, &git).unwrap();
    assert!(!serde_json::to_string(&p).unwrap().contains("DO-NOT-REPORT"));
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        bootstrap::Approval::from_preview(p).unwrap(),
        &git,
        |e| {
            if e.boundary == dort_core::adoption::Boundary::Mutation {
                Err(Error::Invalid("stop".into()))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    let ds = journal::diagnose(&t.host).unwrap();
    let d = ds.iter().find(|d| d.blocks_writes()).unwrap();
    let tx = t
        .host
        .state_dir
        .join("repositories")
        .join(&d.repository_id)
        .join("transactions")
        .join(&d.transaction_id);
    for entry in std::fs::read_dir(&tx).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|s| s == "json") {
            assert!(
                !std::fs::read_to_string(p)
                    .unwrap()
                    .contains("DO-NOT-REPORT")
            );
        }
    }
    std::fs::write(tx.join("config.backup"), b"corrupt").unwrap();
    assert!(
        bootstrap::recovery_preview(
            &t.host,
            &d.repository_id,
            &d.transaction_id,
            RecoveryAction::Resume
        )
        .is_err()
    );
    assert!(
        journal::diagnose(&t.host)
            .unwrap()
            .iter()
            .any(|d| d.problem.is_some())
    );
}

#[test]
fn existing_config_exchange_preserves_values_and_retains_original_inode() {
    let Some(git) = git() else {
        return;
    };
    let t = TestHome::new();
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        approval(&t, &git),
        &git,
        |_| Ok(()),
    )
    .unwrap();
    let config = t.host.local_config_path();
    let text = std::fs::read_to_string(&config).unwrap().replace(
        "[variables]",
        "[variables]\nprivate_value = 'DO-NOT-REPORT-THIS'",
    );
    std::fs::write(&config, &text).unwrap();
    let inode = std::fs::metadata(&config).unwrap().ino();
    let root = t.host.home.join("another");
    let a = bootstrap::Approval::from_preview(bootstrap::preview(&t.host, &root, &git).unwrap())
        .unwrap();
    let tid = bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        a,
        &git,
        |_| Ok(()),
    )
    .unwrap();
    let c = LocalConfig::load(&t.host).unwrap();
    assert_eq!(
        c.variables["private_value"].as_str(),
        Some("DO-NOT-REPORT-THIS")
    );
    let rid = c.active_repository.as_ref().unwrap();
    let tx = t
        .host
        .state_dir
        .join("repositories")
        .join(rid)
        .join("transactions")
        .join(tid);
    assert_eq!(
        inode,
        std::fs::metadata(tx.join("config.new")).unwrap().ino()
    );
    assert_eq!(
        std::fs::read_to_string(tx.join("config.backup")).unwrap(),
        text
    );
    assert_ne!(inode, std::fs::metadata(config).unwrap().ino());
}
#[test]
fn bootstrap_recovery_guards_config_parent_and_journal_tail() {
    let Some(git) = git() else {
        return;
    };
    let t = TestHome::new();
    let (rid, tid) = interrupted_published(&t, &git);
    let config = t.host.config_dir.clone();
    let moved = config.with_file_name("retained-config");
    std::fs::rename(&config, &moved).unwrap();
    std::fs::create_dir(&config).unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).is_err());
    std::fs::remove_dir(&config).unwrap();
    std::fs::rename(&moved, &config).unwrap();
    let tx = t
        .host
        .state_dir
        .join("repositories")
        .join(&rid)
        .join("transactions")
        .join(&tid);
    let private_parent = tx.parent().unwrap();
    std::fs::set_permissions(private_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).is_err());
    std::fs::set_permissions(private_parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let unknown = tx.join("unknown");
    std::fs::write(&unknown, b"DO-NOT-REPORT").unwrap();
    let d = journal::diagnose(&t.host).unwrap().remove(0);
    assert!(d.problem.is_some());
    assert!(!serde_json::to_string(&d).unwrap().contains("DO-NOT-REPORT"));
    assert!(bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).is_err());
    std::fs::remove_file(unknown).unwrap();
    let cps = std::fs::read_dir(&tx)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "json")
        })
        .count();
    let torn = tx.join(format!("{cps:06}.json"));
    std::fs::write(&torn, b"torn DO-NOT-REPORT").unwrap();
    std::fs::set_permissions(&torn, std::fs::Permissions::from_mode(0o600)).unwrap();
    let d = journal::diagnose(&t.host).unwrap().remove(0);
    assert!(d.problem.is_some());
    assert_eq!(d.valid_checkpoints, cps);
    assert_eq!(d.journal_schema, Some(3));
    assert!(bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Abandon).is_err());
}
#[test]
fn published_checkpoint_cannot_be_treated_as_unpublished_preparation() {
    let Some(git) = git() else {
        return;
    };
    let t = TestHome::new();
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        approval(&t, &git),
        &git,
        |e| {
            if e.boundary == dort_core::adoption::Boundary::Checkpoint && e.sequence == 2 {
                Err(Error::Invalid("stop".into()))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    let d = journal::diagnose(&t.host).unwrap().remove(0);
    let tx = t
        .host
        .state_dir
        .join("repositories")
        .join(&d.repository_id)
        .join("transactions")
        .join(&d.transaction_id);
    std::fs::rename(destination(&t), tx.join("repository")).unwrap();
    assert!(
        bootstrap::recovery_preview(
            &t.host,
            &d.repository_id,
            &d.transaction_id,
            RecoveryAction::Resume
        )
        .is_err()
    );
    assert!(!t.host.local_config_path().exists());
}

#[test]
fn every_resumed_bootstrap_boundary_is_restartable_and_approval_is_head_bound() {
    let Some(git) = git() else {
        return;
    };
    let t = TestHome::new();
    let (rid, tid) = interrupted_published(&t, &git);
    let a = bootstrap::RecoveryApproval::from_preview(
        bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).unwrap(),
    )
    .unwrap();
    let mut count = 0;
    bootstrap::recover(&t.host, &WriterLock::acquire(&t.host).unwrap(), a, |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    for stop in 0..count {
        let t = TestHome::new();
        let (rid, tid) = interrupted_published(&t, &git);
        let stale = bootstrap::RecoveryApproval::from_preview(
            bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Abandon).unwrap(),
        )
        .unwrap();
        let a = bootstrap::RecoveryApproval::from_preview(
            bootstrap::recovery_preview(&t.host, &rid, &tid, RecoveryAction::Resume).unwrap(),
        )
        .unwrap();
        let mut at = 0;
        bootstrap::recover(&t.host, &WriterLock::acquire(&t.host).unwrap(), a, |_| {
            let fail = at == stop;
            at += 1;
            if fail {
                Err(Error::Invalid("stop".into()))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(
            bootstrap::recover(
                &t.host,
                &WriterLock::acquire(&t.host).unwrap(),
                stale,
                |_| Ok(())
            )
            .is_err()
        );
        if journal::diagnose(&t.host).unwrap()[0].blocks_writes() {
            recover(&t, &rid, &tid, RecoveryAction::Resume);
        }
        assert_complete(&t);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn cross_device_bootstrap_refuses_before_repository_or_registration_publication() {
    let Some(git) = git() else {
        eprintln!("SKIP cross-device case: Git unavailable");
        return;
    };
    let mut t = TestHome::new();
    let Some(other) = support::cross_device_dir() else {
        return;
    };
    let other_path = other.path().canonicalize().unwrap();
    if std::fs::metadata(&other_path).unwrap().dev()
        == std::fs::metadata(t.sandbox.path()).unwrap().dev()
    {
        eprintln!("SKIP cross-device case: fixture devices are identical");
        return;
    }
    t.host.state_dir = other_path.join("dort");
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        approval(&t, &git),
        &git,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(!destination(&t).exists());
    assert!(!t.host.local_config_path().exists());
    let d = journal::diagnose(&t.host).unwrap().remove(0);
    assert_eq!(d.phase, Some(journal::Phase::Preparing));
    assert!(d.blocks_writes());
    recover(
        &t,
        &d.repository_id,
        &d.transaction_id,
        RecoveryAction::Abandon,
    );
}

// Isolated process helper: never changes this test process's HOME/XDG.
#[test]
fn bootstrap_crash_helper() {
    let Ok(root) = std::env::var("DORT_BOOTSTRAP_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let stop: usize = std::env::var("DORT_BOOTSTRAP_TEST_STOP")
        .unwrap()
        .parse()
        .unwrap();
    let host = HostContext::new(
        root.join("home"),
        None,
        None,
        dort_core::platform::Platform::Ubuntu,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let git = PathBuf::from(std::env::var_os("DORT_BOOTSTRAP_TEST_GIT").unwrap());
    let a = bootstrap::Approval::from_preview(
        bootstrap::preview(&host, &root.join("new-repository"), &git).unwrap(),
    )
    .unwrap();
    let mut at = 0;
    bootstrap::execute(&host, &WriterLock::acquire(&host).unwrap(), a, &git, |_| {
        if at == stop {
            std::process::exit(77)
        }
        at += 1;
        Ok(())
    })
    .unwrap();
}
#[test]
fn process_exit_at_every_bootstrap_boundary_requires_explicit_recovery() {
    let Some(git) = git() else {
        eprintln!("SKIP: Git unavailable");
        return;
    };
    let t = TestHome::new();
    let mut count = 0;
    bootstrap::execute(
        &t.host,
        &WriterLock::acquire(&t.host).unwrap(),
        approval(&t, &git),
        &git,
        |_| {
            count += 1;
            Ok(())
        },
    )
    .unwrap();
    for stop in 0..count {
        let t = TestHome::new();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bootstrap_crash_helper", "--nocapture"])
            .env_clear()
            .env(
                "DORT_BOOTSTRAP_TEST_ROOT",
                t.sandbox.path().canonicalize().unwrap(),
            )
            .env("DORT_BOOTSTRAP_TEST_GIT", &git)
            .env("DORT_BOOTSTRAP_TEST_STOP", stop.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(77), "boundary {stop}");
        let d = journal::diagnose(&t.host).unwrap().remove(0);
        assert!(d.problem.is_none());
        let _lock = WriterLock::acquire(&t.host).unwrap();
        drop(_lock);
        if d.blocks_writes() {
            let action = if d.phase == Some(journal::Phase::Preparing) {
                RecoveryAction::Abandon
            } else {
                RecoveryAction::Resume
            };
            recover(&t, &d.repository_id, &d.transaction_id, action);
        }
        if destination(&t).exists() {
            assert_complete(&t);
        }
    }
}
