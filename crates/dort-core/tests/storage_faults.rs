//! Opt-in kernel ENOSPC checks, never part of ordinary tests. See scripts/test-storage-vm.py.
#![cfg(target_os = "linux")]

use dort_core::{
    Error,
    adoption::{self, Boundary, Event},
    batch,
    bootstrap::{self, RecoveryAction},
    context::HostContext,
    journal,
    lifecycle::{self, RemovalAcknowledgement},
    local::{RepositoryRegistry, repository_state_path},
    model::{FileMode, ManagedFile},
    planner::{self, Request, UnmanageMode},
    platform::{Architecture, Platform},
    recovery,
    state::WriterLock,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

const LIMIT: u64 = 128 * 1024 * 1024;
const MARKER: &str = "dort-disposable-ext4-128MiB-v1\n";

#[derive(Clone, Copy, Debug)]
enum Action {
    Adopt,
    Apply,
    Keep,
    Remove,
    Attach,
    Create,
    BatchAdopt,
    BatchApply,
    BatchKeep,
    BatchRemove,
}
impl Action {
    fn bootstrap(self) -> bool {
        matches!(self, Self::Attach | Self::Create)
    }
    fn adopt(self) -> bool {
        matches!(self, Self::Adopt | Self::BatchAdopt)
    }
    fn linked(self) -> bool {
        matches!(
            self,
            Self::Keep | Self::Remove | Self::BatchKeep | Self::BatchRemove
        )
    }
    fn batch(self) -> bool {
        matches!(
            self,
            Self::BatchAdopt | Self::BatchApply | Self::BatchKeep | Self::BatchRemove
        )
    }
}
#[derive(Clone, Copy, Debug)]
enum At {
    Allocation,
    Backup,
    Intent,
    PostMutation,
    PartialCheckpoint,
}
struct Fixture {
    _root: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
    id: String,
    action: Action,
    lock: WriterLock,
}
impl Fixture {
    fn new(volume: &Path, action: Action) -> Self {
        let root = tempfile::tempdir_in(volume).unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let home = root.path().join("home");
        fs::create_dir(&home).unwrap();
        let architecture = if cfg!(target_arch = "aarch64") {
            Architecture::Aarch64
        } else {
            Architecture::X86_64
        };
        let host = HostContext::new(home, None, None, Platform::Ubuntu, architecture).unwrap();
        let repo = root.path().join("repository");
        if !matches!(action, Action::Create) {
            fs::create_dir(&repo).unwrap();
            fs::set_permissions(&repo, fs::Permissions::from_mode(0o700)).unwrap();
        }
        if matches!(action, Action::Attach) {
            assert!(
                Command::new("/usr/bin/git")
                    .env_clear()
                    .env("HOME", "/no-external-programs")
                    .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                    .arg(&repo)
                    .status()
                    .unwrap()
                    .success()
            );
        } else if !matches!(action, Action::Create) {
            fs::create_dir(repo.join(".git")).unwrap();
        }
        if !matches!(action, Action::Create) {
            fs::create_dir(repo.join("files")).unwrap();
        }
        // Large enough to require actual data blocks, without sensitivity-looking values.
        let bytes = b"ordinary editor setting\n".repeat(3000);
        if !action.adopt() && !matches!(action, Action::Create) {
            fs::write(repo.join("files/editor"), &bytes).unwrap();
        }
        let mut manifest = if action.adopt() || action.bootstrap() {
            "schema = 1\n"
        } else {
            "schema = 1\n[[files]]\nid = 'editor'\nsource = 'files/editor'\ntarget = '~/.editor'\nmode = 'link'\n"
        }.to_owned();
        if action.batch() {
            if !action.adopt() {
                manifest.push_str("[[files]]\nid='second'\nsource='files/second'\ntarget='~/.second'\nmode='link'\n");
                fs::write(repo.join("files/second"), &bytes).unwrap();
            }
            if action.linked() {
                symlink(repo.join("files/second"), host.home.join(".second")).unwrap();
            } else {
                fs::write(host.home.join(".second"), &bytes).unwrap();
            }
        }
        if !matches!(action, Action::Create) {
            fs::write(repo.join("dort.toml"), manifest).unwrap();
        }
        if action.linked() {
            symlink(repo.join("files/editor"), host.home.join(".editor")).unwrap();
        } else {
            fs::write(host.home.join(".editor"), bytes).unwrap();
        }
        let lock = WriterLock::acquire(&host).unwrap();
        let id = if action.bootstrap() {
            String::new()
        } else {
            RepositoryRegistry::register(&host, &lock, &repo).unwrap()
        };
        Self {
            _root: root,
            host,
            repo,
            id,
            action,
            lock,
        }
    }
    fn execute(
        &self,
        observer: impl FnMut(&Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<()> {
        if self.action.bootstrap() {
            let git = Path::new("/usr/bin/git");
            let p = bootstrap::preview(&self.host, &self.repo, git)?;
            return bootstrap::execute(
                &self.host,
                &self.lock,
                bootstrap::Approval::from_preview(p)?,
                git,
                observer,
            )
            .map(|_| ());
        }
        if self.action.batch() {
            let ids = vec!["editor".into(), "second".into()];
            let request = match self.action {
                Action::BatchAdopt => {
                    batch::Request::Adopt(["editor", "second"].into_iter().map(mapping).collect())
                }
                Action::BatchApply => batch::Request::Apply(ids),
                Action::BatchKeep => batch::Request::Keep(ids),
                Action::BatchRemove => batch::Request::Remove(ids),
                _ => unreachable!(),
            };
            let plan = batch::plan(batch::observe(&self.host, &self.repo, &[], request)?);
            let approval = if matches!(self.action, Action::BatchRemove) {
                batch::Approval::from_removal_plan(
                    plan,
                    batch::RemovalAcknowledgements::confirm(vec!["editor".into(), "second".into()]),
                )?
            } else {
                batch::Approval::from_plan(plan)?
            };
            return batch::execute(&self.host, &self.lock, approval, observer).map(|_| ());
        }
        let request = match self.action {
            Action::Adopt => Request::Adopt {
                mapping: ManagedFile {
                    id: "editor".into(),
                    source: "files/editor".into(),
                    target: "~/.editor".into(),
                    mode: FileMode::Link,
                    profiles: vec!["base".into()],
                    requires: vec![],
                    overrides: BTreeMap::new(),
                    permissions: None,
                },
            },
            Action::Apply => Request::Apply,
            Action::Keep | Action::Remove => Request::Unmanage {
                id: "editor".into(),
                mode: if matches!(self.action, Action::Keep) {
                    UnmanageMode::Keep
                } else {
                    UnmanageMode::Remove
                },
            },
            _ => unreachable!(),
        };
        let plan = planner::plan(planner::observe(&self.host, &self.repo, &[], request)?);
        if matches!(self.action, Action::Adopt) {
            adoption::execute(
                &self.host,
                &self.lock,
                adoption::Approval::from_plan(plan)?,
                observer,
            )
            .map(|_| ())
        } else {
            let approval = if matches!(self.action, Action::Remove) {
                lifecycle::Approval::from_removal_plan(
                    plan,
                    RemovalAcknowledgement::confirm("editor"),
                )?
            } else {
                lifecycle::Approval::from_plan(plan)?
            };
            lifecycle::execute(&self.host, &self.lock, approval, observer).map(|_| ())
        }
    }
    fn paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.repo.join("dort.toml"),
            self.repo.join("files/editor"),
            self.host.home.join(".editor"),
            self.host.local_config_path(),
        ];
        if self.action.batch() {
            paths.extend([
                self.repo.join("files/second"),
                self.host.home.join(".second"),
            ]);
        }
        paths
    }
    fn txdir(&self, event: &Event) -> PathBuf {
        let id = if self.id.is_empty() {
            format!(
                "{:x}",
                Sha256::digest(self.repo.as_os_str().as_encoded_bytes())
            )
        } else {
            self.id.clone()
        };
        repository_state_path(&self.host, &id)
            .unwrap()
            .join("transactions")
            .join(&event.transaction_id)
    }
}

fn mapping(id: &str) -> ManagedFile {
    ManagedFile {
        id: id.into(),
        source: format!("files/{id}").into(),
        target: format!("~/.{id}").into(),
        mode: FileMode::Link,
        profiles: vec!["base".into()],
        requires: vec![],
        overrides: BTreeMap::new(),
        permissions: None,
    }
}
// Metadata/hashes only; never display fixture bytes on failure.
fn identity(path: &Path) -> Option<(u64, u64, u32, Vec<u8>)> {
    let m = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => panic!("inspect fixture: {e}"),
    };
    let bytes = if m.is_symlink() {
        fs::read_link(path)
            .unwrap()
            .as_os_str()
            .as_encoded_bytes()
            .to_vec()
    } else {
        fs::read(path).unwrap()
    };
    Some((m.dev(), m.ino(), m.mode(), Sha256::digest(bytes).to_vec()))
}
fn checkpoints(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            let hash = Sha256::digest(fs::read(&p).unwrap()).to_vec();
            (p, hash)
        })
        .collect()
}
struct Filler(PathBuf);
impl Filler {
    fn fill(volume: &Path) -> Self {
        let path = volume.join("harness-owned-filler");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let buffer = vec![0u8; 1024 * 1024];
        let mut written = 0u64;
        loop {
            match file.write(&buffer) {
                Ok(0) => panic!("unexpected zero-length filler write"),
                Ok(n) => {
                    written += n as u64;
                    assert!(written <= LIMIT, "refuse to fill more than 128 MiB");
                }
                Err(e) if e.raw_os_error() == Some(28) => break,
                Err(e) => panic!("unexpected filler error: {e}"),
            }
        }
        // Report the actual write error above, not a synthetic observer failure.
        let _ = file.sync_all();
        println!("filler: kernel ENOSPC after {written} bytes (bounded by {LIMIT})");
        Self(path)
    }
}
impl Drop for Filler {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}

#[test]
#[ignore = "requires explicitly approved isolated 128 MiB ext4 VM volume; scripts/test-storage-vm.py"]
fn kernel_enospc_preserves_intent_and_refuses_uncertain_recovery() {
    assert!(
        !rustix::process::geteuid().is_root(),
        "run fault probe as guest user"
    );
    let volume =
        PathBuf::from(std::env::var_os("DORT_TEST_STORAGE_VOLUME").expect("explicit volume"));
    assert!(volume.is_absolute());
    assert_eq!(volume.canonicalize().unwrap(), volume);
    assert_eq!(
        fs::read_to_string(volume.join(".dort-storage-volume")).unwrap(),
        MARKER
    );
    assert_ne!(
        fs::metadata(&volume).unwrap().dev(),
        fs::metadata("/").unwrap().dev()
    );
    let stat = rustix::fs::statvfs(&volume).unwrap();
    assert_eq!(
        stat.f_frsize, 4096,
        "launcher must select known ext4 block geometry"
    );
    assert!(stat.f_blocks * stat.f_frsize <= LIMIT);
    assert!(stat.f_blocks * stat.f_frsize >= LIMIT / 2);
    // The launcher independently checks the mount's ext4 type and exact loopback image.
    for action in [
        Action::Adopt,
        Action::Apply,
        Action::Keep,
        Action::Remove,
        Action::Attach,
        Action::Create,
        Action::BatchAdopt,
        Action::BatchApply,
        Action::BatchKeep,
        Action::BatchRemove,
    ] {
        let baseline = Fixture::new(&volume, action);
        let mut previous_checkpoint = None;
        let mut first_intent = None;
        let mut first_postcheckpoint = None;
        baseline
            .execute(|e| {
                if e.boundary == Boundary::Checkpoint {
                    previous_checkpoint = Some(e.sequence);
                } else if first_intent.is_none() {
                    first_intent = previous_checkpoint;
                    first_postcheckpoint = Some(
                        baseline
                            .txdir(e)
                            .join(format!("{:06}.json", e.sequence + 1)),
                    );
                }
                Ok(())
            })
            .unwrap();
        let intent = first_intent.expect("baseline must mutate");
        let checkpoint_bytes = fs::metadata(first_postcheckpoint.unwrap()).unwrap().len();
        assert!(
            checkpoint_bytes > 8192,
            "partial probe needs a checkpoint larger than released space"
        );
        println!("{action:?}: baseline post-mutation checkpoint {checkpoint_bytes} bytes");
        drop(baseline);
        for at in [
            At::Allocation,
            At::Backup,
            At::Intent,
            At::PostMutation,
            At::PartialCheckpoint,
        ] {
            let f = Fixture::new(&volume, action);
            let paths = f.paths();
            let before: Vec<_> = paths.iter().map(|p| identity(p)).collect();
            let mut durable = BTreeMap::new();
            let mut filler = if matches!(at, At::Allocation) {
                Some(Filler::fill(&volume))
            } else {
                None
            };
            let mut mutations = 0;
            let result = f.execute(|e| {
                if e.boundary == Boundary::Mutation {
                    mutations += 1;
                }
                let trigger = match at {
                    At::Allocation => false,
                    At::Backup => e.boundary == Boundary::Checkpoint && e.sequence == 0,
                    At::Intent => e.boundary == Boundary::Checkpoint && e.sequence == intent,
                    At::PostMutation | At::PartialCheckpoint => e.boundary == Boundary::Mutation,
                };
                if trigger && filler.is_none() {
                    durable = checkpoints(&f.txdir(e));
                    filler = Some(Filler::fill(&volume));
                    if matches!(at, At::PartialCheckpoint) {
                        let file = OpenOptions::new()
                            .write(true)
                            .open(&filler.as_ref().unwrap().0)
                            .unwrap();
                        // Reclaim two blocks of OUR filler: ext4 needs metadata headroom
                        // as well as data. The baseline checkpoint is larger than this.
                        file.set_len(file.metadata().unwrap().len() - 8192).unwrap();
                        file.sync_all().unwrap();
                        rustix::fs::syncfs(&file).unwrap();
                    }
                }
                Ok(())
            });
            assert!(
                filler.is_some(),
                "fault boundary not reached: {action:?}/{at:?}"
            );
            drop(filler); // Free only this harness's filler; never truncate/remove a checkpoint.
            let error = result.expect_err("full filesystem must fail");
            let failed_operation = match error {
                Error::Io {
                    operation, source, ..
                } => {
                    assert_eq!(source.raw_os_error(), Some(28));
                    operation
                }
                _ => panic!("expected real executor ENOSPC: {action:?}/{at:?}"),
            };
            println!("{action:?}/{at:?}: kernel errno=28; failed operation={failed_operation}");
            for (p, hash) in &durable {
                assert_eq!(&Sha256::digest(fs::read(p).unwrap()).to_vec(), hash);
            }
            if matches!(at, At::Allocation | At::Backup) {
                assert_eq!(mutations, 0);
                assert!(
                    paths.iter().zip(&before).all(|(p, b)| identity(p) == *b),
                    "live input changed before publication"
                );
            }
            if !action.adopt() {
                assert!(
                    identity(&paths[1]) == before[1],
                    "watched repository source changed"
                );
                if action.batch() {
                    assert!(
                        identity(&paths[4]) == before[4],
                        "second watched source changed"
                    );
                }
            }
            if matches!(at, At::PartialCheckpoint) {
                let txdir = durable.keys().next().unwrap().parent().unwrap();
                let partial = fs::read_dir(txdir)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .filter(|p| {
                        p.extension().is_some_and(|e| e == "json") && !durable.contains_key(p)
                    })
                    .any(|p| {
                        let bytes = fs::read(p).unwrap();
                        !bytes.is_empty()
                            && serde_json::from_slice::<serde_json::Value>(&bytes).is_err()
                    });
                assert!(
                    partial,
                    "must observe a genuinely partial nonempty checkpoint"
                );
            }
            let diagnosis = journal::diagnose(&f.host).unwrap();
            assert!(diagnosis.iter().all(|d| d.blocks_writes()));
            if let Some(d) = diagnosis.first() {
                assert_eq!(diagnosis.len(), 1);
                assert!(RepositoryRegistry::register(&f.host, &f.lock, &f.repo).is_err());
                let recovered = if action.bootstrap() {
                    bootstrap::recovery_preview(
                        &f.host,
                        &d.repository_id,
                        &d.transaction_id,
                        RecoveryAction::Abandon,
                    )
                    .and_then(bootstrap::RecoveryApproval::from_preview)
                    .and_then(|a| bootstrap::recover(&f.host, &f.lock, a, |_| Ok(())).map(|_| ()))
                } else {
                    recovery::preview(&f.host, &d.repository_id, &d.transaction_id)
                        .and_then(recovery::Approval::from_preview)
                        .and_then(|a| recovery::execute(&f.host, &f.lock, a))
                };
                if d.problem.is_some() {
                    assert!(
                        recovered.is_err(),
                        "malformed immutable tail must refuse recovery"
                    );
                    assert!(journal::diagnose(&f.host).unwrap()[0].blocks_writes());
                    for (p, hash) in &durable {
                        assert_eq!(&Sha256::digest(fs::read(p).unwrap()).to_vec(), hash);
                    }
                } else {
                    recovered.unwrap();
                    assert!(
                        journal::diagnose(&f.host)
                            .unwrap()
                            .iter()
                            .all(|d| !d.blocks_writes())
                    );
                    if !action.bootstrap() {
                        assert!(
                            paths.iter().zip(&before).all(|(p, b)| identity(p) == *b),
                            "explicit rollback did not restore live identities"
                        );
                    }
                }
                println!(
                    "{action:?}/{at:?}: executor ENOSPC; mutations={mutations}; valid checkpoints={}; recovery={}",
                    d.valid_checkpoints,
                    if d.problem.is_some() {
                        "refused malformed tail"
                    } else {
                        "explicit recovery completed"
                    }
                );
            } else {
                assert!(matches!(at, At::Allocation));
                println!(
                    "{action:?}/{at:?}: executor ENOSPC before transaction allocation; live inputs unchanged"
                );
            }
        }
    }
    inverse_enospc(&volume);
    // Catch a delayed allocation error from the volume while space is available again.
    File::open(&volume).unwrap().sync_all().unwrap();
}

fn rollback(
    f: &Fixture,
    observer: impl FnMut(&Event) -> dort_core::Result<()>,
) -> dort_core::Result<()> {
    let diagnosis = journal::diagnose(&f.host)?;
    let d = diagnosis.first().expect("executed transaction");
    let approval = recovery::Approval::from_preview(recovery::preview(
        &f.host,
        &d.repository_id,
        &d.transaction_id,
    )?)?;
    recovery::execute_with_observer(&f.host, &f.lock, approval, observer)
}

fn inverse_enospc(volume: &Path) {
    for action in [
        Action::Adopt,
        Action::Apply,
        Action::Keep,
        Action::Remove,
        Action::BatchAdopt,
        Action::BatchApply,
        Action::BatchKeep,
        Action::BatchRemove,
    ] {
        let baseline = Fixture::new(volume, action);
        baseline.execute(|_| Ok(())).unwrap();
        let mut previous = None;
        let mut intent = None;
        let mut postcheckpoint = None;
        rollback(&baseline, |e| {
            if e.boundary == Boundary::Checkpoint {
                previous = Some(e.sequence);
            } else if intent.is_none() {
                intent = previous;
                postcheckpoint = Some(
                    baseline
                        .txdir(e)
                        .join(format!("{:06}.json", e.sequence + 1)),
                );
            }
            Ok(())
        })
        .unwrap();
        let intent = intent.expect("rollback must mutate");
        assert!(fs::metadata(postcheckpoint.unwrap()).unwrap().len() > 8192);
        drop(baseline);
        for at in [
            At::Allocation,
            At::Intent,
            At::PostMutation,
            At::PartialCheckpoint,
        ] {
            let f = Fixture::new(volume, action);
            let paths = f.paths();
            let original: Vec<_> = paths.iter().map(|p| identity(p)).collect();
            f.execute(|_| Ok(())).unwrap();
            let applied: Vec<_> = paths.iter().map(|p| identity(p)).collect();
            let d = journal::diagnose(&f.host).unwrap().remove(0);
            let txdir = repository_state_path(&f.host, &d.repository_id)
                .unwrap()
                .join("transactions")
                .join(&d.transaction_id);
            let mut durable = checkpoints(&txdir);
            let mut filler = if matches!(at, At::Allocation) {
                Some(Filler::fill(volume))
            } else {
                None
            };
            let mut mutations = 0;
            let result = rollback(&f, |e| {
                if e.boundary == Boundary::Mutation {
                    mutations += 1;
                }
                let trigger = match at {
                    At::Allocation => false,
                    At::Intent => e.boundary == Boundary::Checkpoint && e.sequence == intent,
                    At::PostMutation | At::PartialCheckpoint => e.boundary == Boundary::Mutation,
                    At::Backup => unreachable!(),
                };
                if trigger && filler.is_none() {
                    durable = checkpoints(&txdir);
                    filler = Some(Filler::fill(volume));
                    if matches!(at, At::PartialCheckpoint) {
                        let file = OpenOptions::new()
                            .write(true)
                            .open(&filler.as_ref().unwrap().0)
                            .unwrap();
                        file.set_len(file.metadata().unwrap().len() - 8192).unwrap();
                        file.sync_all().unwrap();
                        rustix::fs::syncfs(&file).unwrap();
                    }
                }
                Ok(())
            });
            assert!(
                filler.is_some(),
                "inverse fault boundary not reached: {action:?}/{at:?}"
            );
            drop(filler);
            let operation = match result.expect_err("full volume must interrupt rollback") {
                Error::Io {
                    operation, source, ..
                } => {
                    assert_eq!(source.raw_os_error(), Some(28));
                    operation
                }
                _ => panic!("expected real inverse ENOSPC: {action:?}/{at:?}"),
            };
            for (p, hash) in &durable {
                assert_eq!(&Sha256::digest(fs::read(p).unwrap()).to_vec(), hash);
            }
            if matches!(at, At::Allocation) {
                assert_eq!(mutations, 0);
                assert!(paths.iter().zip(&applied).all(|(p, b)| identity(p) == *b));
            }
            if matches!(at, At::PartialCheckpoint) {
                assert!(
                    fs::read_dir(&txdir)
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .any(|p| {
                            p.extension().is_some_and(|e| e == "json")
                                && !durable.contains_key(&p)
                                && {
                                    let bytes = fs::read(p).unwrap();
                                    !bytes.is_empty()
                                        && serde_json::from_slice::<serde_json::Value>(&bytes)
                                            .is_err()
                                }
                        }),
                    "inverse checkpoint must be genuinely partial and nonempty"
                );
            }
            let d = journal::diagnose(&f.host).unwrap().remove(0);
            assert!(d.blocks_writes());
            assert!(RepositoryRegistry::register(&f.host, &f.lock, &f.repo).is_err());
            let retried = rollback(&f, |_| Ok(())); // Fresh preview/approval, never replay the old one.
            if d.problem.is_some() {
                assert!(
                    retried.is_err(),
                    "malformed inverse tail must refuse recovery"
                );
                assert!(journal::diagnose(&f.host).unwrap()[0].blocks_writes());
            } else {
                retried.unwrap();
                assert!(paths.iter().zip(&original).all(|(p, b)| identity(p) == *b));
                assert!(
                    journal::diagnose(&f.host)
                        .unwrap()
                        .iter()
                        .all(|d| !d.blocks_writes())
                );
            }
            for (p, hash) in &durable {
                assert_eq!(&Sha256::digest(fs::read(p).unwrap()).to_vec(), hash);
            }
            println!(
                "{action:?}/inverse/{at:?}: kernel errno=28; failed operation={operation}; mutations={mutations}; malformed={}",
                d.problem.is_some()
            );
        }
    }
}
