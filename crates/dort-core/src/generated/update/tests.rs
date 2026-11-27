use super::*;
use crate::{
    generated::Deployment,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    process::Command,
};

const OLD: &str = "original";
const NEXT: &str = "approved";
const LOCAL: &[u8] = b"local \xff\tbytes\r\n";
const CHILD: &str = "generated::update::tests::crash_child";

struct Fixture {
    host: HostContext,
    repo: PathBuf,
    dest: PathBuf,
}

impl Fixture {
    fn paths(root: &Path) -> Self {
        let host = HostContext::new(
            root.join("home"),
            None,
            None,
            Platform::Linux,
            Architecture::X86_64,
        )
        .unwrap();
        let repo = root.join("repo");
        let dest = host.home.join("app/file");
        Self { host, repo, dest }
    }

    fn new(root: &Path) -> Self {
        let f = Self::paths(root);
        fs::create_dir_all(f.repo.join(".git")).unwrap();
        fs::create_dir_all(f.dest.parent().unwrap()).unwrap();
        fs::write(f.repo.join("dort.toml"), "schema = 1\n[[files]]\nid = 'settings'\nmode = 'template'\nsource = 'source'\ntarget = '~/app/file'\n").unwrap();
        {
            let lock = WriterLock::acquire(&f.host).unwrap();
            RepositoryRegistry::register(&f.host, &lock, &f.repo).unwrap();
        }
        fs::write(f.repo.join("source"), OLD).unwrap();
        let deployed = Deployment::prepare(&f.host, None, "settings", &[])
            .unwrap()
            .approve()
            .execute()
            .unwrap();
        assert!(deployed.record_warning.is_none());
        fs::write(f.repo.join("source"), NEXT).unwrap();
        f
    }

    fn approve(&self) -> UpdateApproval {
        Update::prepare(&self.host, None, "settings", &[])
            .unwrap()
            .approve()
    }

    fn prepare(&self, action: UpdateAction) -> Result<Update> {
        Update::prepare_action(&self.host, None, "settings", &[], action)
    }

    fn set_local(&self, action: UpdateAction) {
        match action {
            UpdateAction::Update => (),
            UpdateAction::Replace => {
                fs::write(&self.dest, LOCAL).unwrap();
                fs::set_permissions(&self.dest, fs::Permissions::from_mode(0o640)).unwrap();
            }
            UpdateAction::Recreate => fs::remove_file(&self.dest).unwrap(),
        }
    }

    fn status(&self) -> DriftStatus {
        Preview::prepare(&self.host, &self.repo, "settings")
            .unwrap()
            .drift_status
            .unwrap()
    }

    fn slot(&self) -> Slot {
        read_declaration(&self.host, None, "settings", &[])
            .unwrap()
            .record
    }

    fn assert_prior_receipt(&self) {
        assert!(
            self.slot().load().unwrap().unwrap().deployed
                == Stamp::new(OLD.as_bytes(), Mode(0o600))
        );
    }
}

#[test]
fn an_editor_at_the_exchange_boundary_leaves_its_exact_object_in_the_private_folder() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let editor = f.dest.with_file_name("editor");
    fs::write(&editor, "unreviewed editor bytes").unwrap();
    let inode = fs::metadata(&editor).unwrap().ino();
    let result = f
        .approve()
        .execute_at(|at| {
            if at == Boundary::Exchange {
                fs::rename(&editor, &f.dest).unwrap();
            }
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        &result,
        UpdateOutcome::NeedsRecovery {
            exchange: ExchangeStatus::Completed,
            ..
        }
    ));
    let location = pending_update(&f.host).unwrap().unwrap();
    for _ in 0..2 {
        assert_eq!(fs::read_to_string(&f.dest).unwrap(), NEXT);
        assert_eq!(
            fs::read_to_string(&location.artifact).unwrap(),
            "unreviewed editor bytes"
        );
        assert_eq!(fs::metadata(&location.artifact).unwrap().ino(), inode);
        f.assert_prior_receipt();
        assert!(journal::ensure_writable(&f.host).is_err());
        assert!(matches!(
            recover_update(&f.host).unwrap().unwrap(),
            UpdateOutcome::NeedsRecovery {
                exchange: ExchangeStatus::Unknown,
                ..
            }
        ));
    }
}

#[test]
fn an_open_writer_can_change_the_displaced_file_and_stop_receipt_commit() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let mut writer = fs::OpenOptions::new().append(true).open(&f.dest).unwrap();
    let inode = writer.metadata().unwrap().ino();
    let result = f
        .approve()
        .execute_at(|at| {
            if at == Boundary::Exchanged {
                writer.write_all(b" + local edit").unwrap();
            }
            Ok(())
        })
        .unwrap();
    let UpdateOutcome::NeedsRecovery {
        location,
        exchange: ExchangeStatus::Completed,
        ..
    } = result
    else {
        panic!("expected a blocked receipt commit: {result:?}");
    };
    assert_eq!(fs::read_to_string(&location.destination).unwrap(), NEXT);
    assert_eq!(
        fs::read_to_string(&location.artifact).unwrap(),
        "original + local edit"
    );
    assert_eq!(fs::metadata(&location.artifact).unwrap().ino(), inode);
    assert_eq!(f.status(), DriftStatus::Unverifiable);
    f.assert_prior_receipt();
    assert!(matches!(
        recover_update(&f.host).unwrap().unwrap(),
        UpdateOutcome::NeedsRecovery { .. }
    ));
}

#[test]
fn a_moved_parent_and_substituted_symlink_are_preserved_without_following_them() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let outside = f.host.home.join("outside");
    fs::write(&outside, "unrelated data").unwrap();
    let parent = f.dest.parent().unwrap();
    let moved = parent.with_file_name("moved");
    let result = f
        .approve()
        .execute_at(|at| {
            if at == Boundary::Exchange {
                fs::rename(parent, &moved).unwrap();
                let moved_dest = moved.join("file");
                fs::remove_file(&moved_dest).unwrap();
                symlink(&outside, moved_dest).unwrap();
                fs::create_dir(parent).unwrap();
                fs::write(&f.dest, "new parent content").unwrap();
            }
            Ok(())
        })
        .unwrap();
    let UpdateOutcome::NeedsRecovery {
        exchange: ExchangeStatus::Completed,
        location,
        ..
    } = result
    else {
        panic!("expected an exchange with unresolved artifacts: {result:?}");
    };
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), "new parent content");
    assert_eq!(fs::read_to_string(moved.join("file")).unwrap(), NEXT);
    let stage = moved.join(location.artifact.parent().unwrap().file_name().unwrap());
    assert_eq!(fs::metadata(&stage).unwrap().ino(), location.stage_inode);
    assert_eq!(fs::read_link(stage.join("file")).unwrap(), outside);
    assert_eq!(fs::read_to_string(&outside).unwrap(), "unrelated data");
    f.assert_prior_receipt();
    assert!(matches!(
        recover_update(&f.host).unwrap().unwrap(),
        UpdateOutcome::NeedsRecovery { .. }
    ));
}

#[test]
fn errors_from_exchange_do_not_trigger_a_fallback() {
    for error in [Errno::NOSYS, Errno::NOTSUP, Errno::XDEV] {
        let root = tempfile::tempdir().unwrap();
        let f = Fixture::new(&root.path().canonicalize().unwrap());
        let result = f
            .approve()
            .execute_at(|at| {
                if at == Boundary::Exchange {
                    Err(error)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let UpdateOutcome::NotExchanged { location, .. } = result else {
            panic!("expected refusal: {result:?}");
        };
        assert_eq!(fs::read_to_string(&f.dest).unwrap(), OLD);
        assert_eq!(fs::read_to_string(&location.artifact).unwrap(), NEXT);
        assert_eq!(f.status(), DriftStatus::UpdateAvailable);
        assert!(recover_update(&f.host).unwrap().is_none());
    }
}

#[test]
fn a_failed_receipt_commit_can_be_retried_without_exchange() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let result = f
        .approve()
        .execute_at(|at| {
            if at == Boundary::CommitReceipt {
                Err(Errno::NOSPC)
            } else {
                Ok(())
            }
        })
        .unwrap();
    assert!(matches!(
        result,
        UpdateOutcome::NeedsRecovery {
            exchange: ExchangeStatus::Completed,
            ..
        }
    ));
    let inode = fs::metadata(&f.dest).unwrap().ino();
    f.assert_prior_receipt();
    let reviewed = pending_update(&f.host).unwrap().unwrap();
    let previous = UpdateLocation {
        artifact: reviewed.artifact.with_file_name("different-private-file"),
        ..reviewed
    };
    assert!(recover_reviewed_update(&f.host, &previous).is_err());
    assert!(pending_update(&f.host).unwrap().is_some());
    f.assert_prior_receipt();
    let reviewed = pending_update(&f.host).unwrap().unwrap();
    let Some(UpdateOutcome::Updated { location }) =
        recover_reviewed_update(&f.host, &reviewed).unwrap()
    else {
        panic!("expected recovery to finalize receipt");
    };
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), NEXT);
    assert_eq!(fs::metadata(&f.dest).unwrap().ino(), inode);
    assert_eq!(fs::read_to_string(&location.artifact).unwrap(), OLD);
    assert_eq!(f.status(), DriftStatus::UpToDate);
    assert!(recover_update(&f.host).unwrap().is_none());
}

#[test]
fn damaged_pending_state_blocks_writes_and_leaves_files_in_place() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let pending = f.host.state_dir.join(PENDING);
    let result = f
        .approve()
        .execute_at(|at| {
            if at == Boundary::Exchanged {
                fs::write(&pending, b"damaged intent").unwrap();
                return Err(Errno::IO);
            }
            Ok(())
        })
        .unwrap();
    let UpdateOutcome::NeedsRecovery {
        location,
        exchange: ExchangeStatus::Completed,
        ..
    } = result
    else {
        panic!("expected an exchange with unresolved artifacts: {result:?}");
    };
    assert!(pending_update(&f.host).is_err());
    assert!(recover_update(&f.host).is_err());
    assert!(journal::ensure_writable(&f.host).is_err());
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), NEXT);
    assert_eq!(fs::read_to_string(&location.artifact).unwrap(), OLD);
    assert_eq!(fs::read_to_string(pending).unwrap(), "damaged intent");
    f.assert_prior_receipt();
}

#[test]
fn local_replacement_uses_the_reviewed_bytes_modes_and_retains_the_local_inode() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    f.set_local(UpdateAction::Replace);
    let inode = fs::metadata(&f.dest).unwrap().ino();
    let proposed = b"next \t\r\nlast line";
    fs::write(f.repo.join("source"), proposed).unwrap();
    let manifest = f.repo.join("dort.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap() + "permissions = '0644'\n",
    )
    .unwrap();
    let update = f.prepare(UpdateAction::Replace).unwrap();
    let view = update.review();
    assert_eq!(
        view.detected,
        DriftStatus::LocallyEditedAndPermissionsChanged
    );
    assert_eq!(view.deployed, LOCAL);
    assert_eq!(view.deployed_permissions.as_deref(), Some("0640"));
    assert_eq!(view.recorded_permissions, "0600");
    assert_eq!(view.proposed.as_bytes(), proposed);
    assert_eq!(view.proposed_permissions, "0644");
    let retained = view.retained.unwrap();
    assert!(!retained.exists());
    assert!(f.prepare(UpdateAction::Update).is_err());
    assert!(f.prepare(UpdateAction::Recreate).is_err());
    let UpdateOutcome::Updated { location } = update.approve().execute().unwrap() else {
        panic!("replacement did not complete");
    };
    assert_eq!(location.action, UpdateAction::Replace);
    assert_eq!(location.artifact, retained);
    assert_eq!(fs::read(&retained).unwrap(), LOCAL);
    assert_eq!(fs::metadata(&retained).unwrap().ino(), inode);
    assert_eq!(fs::metadata(&retained).unwrap().mode() & 0o7777, 0o640);
    assert_eq!(fs::read(&f.dest).unwrap(), proposed);
    assert_eq!(fs::metadata(&f.dest).unwrap().mode() & 0o7777, 0o644);
    assert_eq!(f.status(), DriftStatus::UpToDate);
    assert!(f.prepare(UpdateAction::Replace).is_err());
}

#[test]
fn local_actions_refuse_stale_approvals_and_changes_after_intent() {
    for action in [UpdateAction::Replace, UpdateAction::Recreate] {
        let root = tempfile::tempdir().unwrap();
        let f = Fixture::new(&root.path().canonicalize().unwrap());
        f.set_local(action);
        let approval = f.prepare(action).unwrap().approve();
        fs::write(f.repo.join("source"), "proposal changed").unwrap();
        assert!(matches!(approval.execute(), Err(Error::Invalid(_))));
        assert_eq!(
            fs::read_dir(f.dest.parent().unwrap()).unwrap().count(),
            usize::from(action == UpdateAction::Replace)
        );
        let approval = f.prepare(action).unwrap().approve();
        let outcome = approval
            .execute_at(|point| {
                if point == Boundary::Intent {
                    fs::write(&f.dest, "changed after reviewing").unwrap();
                }
                Ok(())
            })
            .unwrap();
        assert!(matches!(outcome, UpdateOutcome::NotExchanged { .. }));
        assert_eq!(
            fs::read_to_string(&f.dest).unwrap(),
            "changed after reviewing"
        );
        f.assert_prior_receipt();
        assert!(recover_update(&f.host).unwrap().is_none());
    }
}

#[test]
fn late_arrival_at_a_recreation_destination_is_not_replaced_even_with_identical_bytes() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    f.set_local(UpdateAction::Recreate);
    let mut arrival = None;
    let outcome = f
        .prepare(UpdateAction::Recreate)
        .unwrap()
        .approve()
        .execute_at(|point| {
            if point == Boundary::Exchange {
                fs::write(&f.dest, NEXT).unwrap();
                fs::set_permissions(&f.dest, fs::Permissions::from_mode(0o600)).unwrap();
                arrival = Some(fs::metadata(&f.dest).unwrap().ino());
            }
            Ok(())
        })
        .unwrap();
    let UpdateOutcome::NotExchanged { location, .. } = outcome else {
        panic!("late arrival was not handled: {outcome:?}");
    };
    assert_eq!(Some(fs::metadata(&f.dest).unwrap().ino()), arrival);
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), NEXT);
    assert_eq!(fs::read_to_string(&location.artifact).unwrap(), NEXT);
    assert_eq!(f.status(), DriftStatus::LocallyEdited);
    f.assert_prior_receipt();
    assert!(recover_update(&f.host).unwrap().is_none());
}

#[test]
fn recovery_does_not_report_success_if_files_changed_after_saving_the_receipt() {
    for action in [UpdateAction::Replace, UpdateAction::Recreate] {
        let root = tempfile::tempdir().unwrap();
        let f = Fixture::new(&root.path().canonicalize().unwrap());
        f.set_local(action);
        let outcome = f
            .prepare(action)
            .unwrap()
            .approve()
            .execute_at(|point| {
                if point == Boundary::Receipt {
                    Err(Errno::IO)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let UpdateOutcome::NeedsRecovery {
            location,
            exchange: ExchangeStatus::Completed,
            ..
        } = outcome
        else {
            panic!("expected pending record: {outcome:?}");
        };
        let changed = if action == UpdateAction::Recreate {
            &location.destination
        } else {
            &location.artifact
        };
        fs::write(changed, "edited after publication").unwrap();
        for _ in 0..2 {
            assert!(matches!(
                recover_reviewed_update(&f.host, &location).unwrap(),
                Some(UpdateOutcome::NeedsRecovery {
                    exchange: ExchangeStatus::Unknown,
                    ..
                })
            ));
            assert_eq!(fs::read(changed).unwrap(), b"edited after publication");
            assert!(WriterLock::acquire(&f.host).is_err());
        }
    }
}

#[test]
fn crash_child() {
    let Some(root) = std::env::var_os("DORT_GENERATED_CRASH_ROOT") else {
        return;
    };
    let point = match std::env::var("DORT_GENERATED_CRASH_POINT")
        .unwrap()
        .as_str()
    {
        "intent" => Boundary::Intent,
        "exchange" => Boundary::Exchanged,
        "receipt" => Boundary::Receipt,
        point => panic!("unknown crash point {point}"),
    };
    let action = match std::env::var("DORT_GENERATED_CRASH_ACTION")
        .unwrap()
        .as_str()
    {
        "Update" => UpdateAction::Update,
        "Replace" => UpdateAction::Replace,
        "Recreate" => UpdateAction::Recreate,
        action => panic!("unknown action {action}"),
    };
    let f = Fixture::paths(Path::new(&root));
    let result = f.prepare(action).unwrap().approve().execute_at(|at| {
        if at == point {
            std::process::exit(71);
        }
        Ok(())
    });
    panic!("expected exit: {result:?}");
}

#[test]
fn recovery_after_process_exit_observes_names_and_never_publishes_again() {
    for action in [
        UpdateAction::Update,
        UpdateAction::Replace,
        UpdateAction::Recreate,
    ] {
        for point in ["intent", "exchange", "receipt"] {
            recover_after_crash(action, point);
        }
    }
}

fn recover_after_crash(action: UpdateAction, point: &str) {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let f = Fixture::new(&root);
    f.set_local(action);
    let candidate = if action == UpdateAction::Recreate {
        OLD
    } else {
        NEXT
    };
    fs::write(f.repo.join("source"), candidate).unwrap();
    let old_bytes = fs::read(&f.dest).ok();
    let old_inode = fs::metadata(&f.dest).ok().map(|meta| meta.ino());
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD])
        .env("DORT_GENERATED_CRASH_ROOT", &root)
        .env("DORT_GENERATED_CRASH_POINT", point)
        .env("DORT_GENERATED_CRASH_ACTION", format!("{action:?}"))
        .output()
        .unwrap();
    assert_eq!(
        status.status.code(),
        Some(71),
        "{} {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let where_ = pending_update(&f.host).unwrap().unwrap();
    assert_eq!(where_.action, action);
    let before = point == "intent";
    let (old_name, candidate_name) = if before {
        (&f.dest, &where_.artifact)
    } else {
        (&where_.artifact, &f.dest)
    };
    assert_eq!(fs::read(old_name).ok(), old_bytes);
    assert_eq!(fs::read(candidate_name).unwrap(), candidate.as_bytes());
    let inode_at = |path: &Path| fs::metadata(path).ok().map(|meta| meta.ino());
    let candidate_inode = inode_at(candidate_name);
    assert_eq!(inode_at(old_name), old_inode);
    assert_eq!(fs::metadata(candidate_name).unwrap().mode() & 0o7777, 0o600);
    if point != "receipt" {
        f.assert_prior_receipt();
    }
    assert!(journal::ensure_writable(&f.host).is_err());
    assert!(WriterLock::acquire(&f.host).is_err());
    {
        let _lock = WriterLock::acquire_for_generated_recovery(&f.host).unwrap();
        assert!(matches!(
            recover_reviewed_update(&f.host, &where_),
            Err(Error::Locked)
        ));
    }
    match recover_reviewed_update(&f.host, &where_).unwrap().unwrap() {
        UpdateOutcome::NotExchanged { location, .. } if before => assert_eq!(location, where_),
        UpdateOutcome::Updated { location } if !before => assert_eq!(location, where_),
        result => panic!("wrong recovery for {point}: {result:?}"),
    }
    assert!(recover_update(&f.host).unwrap().is_none());
    assert_eq!(
        f.status(),
        if before {
            match action {
                UpdateAction::Update => DriftStatus::UpdateAvailable,
                UpdateAction::Replace => DriftStatus::LocallyEditedAndPermissionsChanged,
                UpdateAction::Recreate => DriftStatus::LocallyDeleted,
            }
        } else {
            DriftStatus::UpToDate
        }
    );
    assert_eq!(inode_at(old_name), old_inode);
    assert_eq!(inode_at(candidate_name), candidate_inode);
    journal::ensure_writable(&f.host).unwrap();
    assert!(WriterLock::acquire(&f.host).is_ok());
}

#[test]
fn update_rejects_changed_host_input_with_the_same_output() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let manifest = f.repo.join("dort.toml");
    fs::write(
        &manifest,
        format!(
            "{}syntax = 'jinja'\nhost_inputs = ['arch']\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    let mut update = Update::prepare(&f.host, None, "settings", &[]).unwrap();
    assert_eq!(update.review().proposed, NEXT);
    assert_eq!(update.review().resolved_inputs["host.arch"].value, "x86_64");
    update.host.architecture = Architecture::Aarch64;
    let error = update.approve().execute().unwrap_err();
    assert!(error.to_string().contains("review"));
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), OLD);
    assert!(pending_update(&f.host).unwrap().is_none());
    f.assert_prior_receipt();
}

#[test]
fn recovery_keeps_the_approved_render_after_context_and_source_change() {
    let root = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&root.path().canonicalize().unwrap());
    let manifest = f.repo.join("dort.toml");
    fs::write(
        &manifest,
        format!(
            "{}syntax = 'jinja'\nhost_inputs = ['arch']\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fs::write(f.repo.join("source"), "{{ host.arch }}").unwrap();
    let approved = Update::prepare(&f.host, None, "settings", &[]).unwrap();
    assert_eq!(approved.review().proposed, "x86_64");
    let outcome = approved
        .approve()
        .execute_at(|at| {
            if at == Boundary::Exchanged {
                Err(Errno::NOSPC)
            } else {
                Ok(())
            }
        })
        .unwrap();
    assert!(matches!(
        outcome,
        UpdateOutcome::NeedsRecovery {
            exchange: ExchangeStatus::Completed,
            ..
        }
    ));
    f.host.architecture = Architecture::Aarch64;
    fs::write(f.repo.join("source"), "changed {{ host.arch }}").unwrap();
    assert!(matches!(
        recover_update(&f.host).unwrap(),
        Some(UpdateOutcome::Updated { .. })
    ));
    assert_eq!(fs::read_to_string(&f.dest).unwrap(), "x86_64");
    assert!(f.slot().load().unwrap().unwrap().deployed == Stamp::new(b"x86_64", Mode(0o600)));
    assert_eq!(f.status(), DriftStatus::UpdateAvailable);
}
