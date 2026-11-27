use super::*;
use crate::{
    generated::Deployment,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, symlink},
    process::Command,
};

const OLD: &str = "original";
const NEXT: &str = "approved";
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
    let Some(UpdateOutcome::Updated { location }) = recover_update(&f.host).unwrap() else {
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
    let f = Fixture::paths(Path::new(&root));
    let result = f.approve().execute_at(|at| {
        if at == point {
            std::process::exit(71);
        }
        Ok(())
    });
    panic!("expected exit: {result:?}");
}

#[test]
fn recovery_after_process_exit_observes_names_and_never_exchanges_again() {
    for point in ["intent", "exchange", "receipt"] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let f = Fixture::new(&root);
        let old_inode = fs::metadata(&f.dest).unwrap().ino();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD])
            .env("DORT_GENERATED_CRASH_ROOT", &root)
            .env("DORT_GENERATED_CRASH_POINT", point)
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
        let before = point == "intent";
        assert_eq!(
            fs::read_to_string(&f.dest).unwrap(),
            if before { OLD } else { NEXT }
        );
        assert_eq!(
            fs::read_to_string(&where_.artifact).unwrap(),
            if before { NEXT } else { OLD }
        );
        let (live_inode, private_inode) = (
            fs::metadata(&f.dest).unwrap().ino(),
            fs::metadata(&where_.artifact).unwrap().ino(),
        );
        assert_eq!(if before { live_inode } else { private_inode }, old_inode);
        if point != "receipt" {
            f.assert_prior_receipt();
        }
        assert!(journal::ensure_writable(&f.host).is_err());
        match recover_update(&f.host).unwrap().unwrap() {
            UpdateOutcome::NotExchanged { location, .. } if before => assert_eq!(location, where_),
            UpdateOutcome::Updated { location } if !before => assert_eq!(location, where_),
            result => panic!("wrong recovery for {point}: {result:?}"),
        }
        assert!(recover_update(&f.host).unwrap().is_none());
        assert_eq!(
            f.status(),
            if before {
                DriftStatus::UpdateAvailable
            } else {
                DriftStatus::UpToDate
            }
        );
        assert_eq!(fs::metadata(&f.dest).unwrap().ino(), live_inode);
        assert_eq!(fs::metadata(&where_.artifact).unwrap().ino(), private_inode);
        journal::ensure_writable(&f.host).unwrap();
    }
}
