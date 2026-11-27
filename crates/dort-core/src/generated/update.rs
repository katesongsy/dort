use super::{
    DestinationStatus, DriftStatus, Mode, OUTPUT_LIMIT, Preview,
    deploy::{Scope, checked_parent, read_declaration},
    overlaps_secret_target,
    record::{self, DeploymentRecord, Identity, Slot, Stamp},
};
use crate::{
    Error, Result, adoption,
    context::HostContext,
    error::io_error,
    journal,
    observation::{self, Fingerprint, Parent, ParentHandle},
    state::{PrivateDir, WriterLock},
};
use rustix::{fs, io::Errno};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

pub(super) const STAGE_PREFIX: &str = ".dort-generated-update-";
const ARTIFACT: &str = "file";
const PENDING: &str = "generated-update.json";
const INTENT_LIMIT: u64 = 256 * 1024;

pub struct Update {
    host: HostContext,
    explicit: Option<PathBuf>,
    temporary_profiles: Vec<String>,
    prepared: Prepared,
}

#[derive(Serialize)]
pub struct UpdateReview<'a> {
    pub repository: &'a Path,
    pub active_profiles: &'a BTreeSet<String>,
    pub id: &'a str,
    pub source: Option<&'a Path>,
    pub destination: &'a Path,
    pub deployed: &'a [u8],
    pub deployed_permissions: String,
    pub proposed: &'a str,
    pub proposed_permissions: String,
}

pub struct UpdateApproval(Update);

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct UpdateLocation {
    pub destination: PathBuf,
    pub artifact: PathBuf,
    pub stage_device: u64,
    pub stage_inode: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeStatus {
    Completed,
    NotPerformed,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[must_use]
pub enum UpdateOutcome {
    Updated {
        location: UpdateLocation,
    },
    NotExchanged {
        location: UpdateLocation,
        reason: String,
    },
    NeedsRecovery {
        location: UpdateLocation,
        exchange: ExchangeStatus,
        reason: String,
    },
}

struct Prepared {
    scope: Scope,
    slot: Slot,
    preview: Preview,
    mode: Mode,
    prior: DeploymentRecord,
    old: Fingerprint,
    bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    identity: Identity,
    id: String,
    scope: Scope,
    stage: Fingerprint,
    old: Fingerprint,
    candidate: Fingerprint,
    prior: DeploymentRecord,
    proposed: Stamp,
}

#[derive(PartialEq, Eq)]
enum Placement {
    Before,
    Exchanged,
    Conflict,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Intent,
    Exchange,
    Exchanged,
    CommitReceipt,
    Receipt,
}

impl Update {
    pub fn prepare(
        host: &HostContext,
        explicit: Option<&Path>,
        id: &str,
        temporary_profiles: &[String],
    ) -> Result<Self> {
        ensure_no_pending_update(host)?;
        Ok(Self {
            host: host.clone(),
            explicit: explicit.map(Path::to_owned),
            temporary_profiles: temporary_profiles.to_vec(),
            prepared: prepare(host, explicit, id, temporary_profiles)?,
        })
    }

    pub fn review(&self) -> UpdateReview<'_> {
        let p = &self.prepared;
        UpdateReview {
            repository: p.scope.manifest.path.parent().expect("manifest parent"),
            active_profiles: &p.scope.profiles,
            id: &p.preview.id,
            source: p.preview.source.as_deref(),
            destination: &p.preview.destination,
            deployed: &p.bytes,
            deployed_permissions: Mode(p.prior.deployed.mode).label(),
            proposed: &p.preview.rendered,
            proposed_permissions: p.mode.label(),
        }
    }

    pub fn approve(self) -> UpdateApproval {
        UpdateApproval(self)
    }

    fn verify(&self) -> Result<()> {
        let p = &self.prepared;
        let now = prepare(
            &self.host,
            self.explicit.as_deref(),
            &p.preview.id,
            &self.temporary_profiles,
        )
        .map_err(|_| stale())?;
        if now.scope != p.scope
            || now.slot.identity != p.slot.identity
            || now.mode != p.mode
            || now.preview.rendered != p.preview.rendered
            || now.prior != p.prior
            || now.old != p.old
            || now.bytes != p.bytes
        {
            return Err(stale());
        }
        Ok(())
    }
}

impl UpdateApproval {
    pub fn execute(self) -> Result<UpdateOutcome> {
        self.execute_at(|_| Ok(()))
    }

    fn execute_at(
        self,
        mut at: impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<UpdateOutcome> {
        let update = self.0;
        let p = &update.prepared;
        let _lock = WriterLock::acquire(&update.host)?;
        journal::ensure_writable(&update.host)?;
        update.verify()?;
        let state = open_locked_state(&update.host)?;
        let destination = Path::new(&p.slot.identity.destination);
        let handle = open_destination(destination, &p.scope.destination_parents)?;
        let id = adoption::transaction_id();
        let stage_name = format!("{STAGE_PREFIX}{id}");
        let (stage, stage_fp, candidate) = stage(&handle, &stage_name, destination, p)?;
        let intent = Intent {
            schema: 1,
            identity: p.slot.identity.clone(),
            id,
            scope: p.scope.clone(),
            stage: stage_fp,
            old: p.old.clone(),
            candidate,
            prior: p.prior.clone(),
            proposed: Stamp::new(p.preview.rendered.as_bytes(), p.mode),
        };
        if let Err(error) = intent.persist(&state) {
            return Err(Error::Invalid(format!(
                "Could not save the update plan. Dort did not exchange the file. Check {} and {}. {error}",
                state.path.join(PENDING).display(),
                intent.location().artifact.display(),
            )));
        }
        if let Err(error) = at(Boundary::Intent) {
            return Ok(intent.needs_recovery(ExchangeStatus::NotPerformed, error.to_string()));
        }
        if update.verify().is_err() || intent.placement(&handle, &stage) != Placement::Before {
            return Ok(
                intent.retire_before(&state, "The review changed. Open a new review.".into())
            );
        }
        let exchange = at(Boundary::Exchange).and_then(|()| {
            fs::renameat_with(
                &stage,
                ARTIFACT,
                &handle.directory,
                &handle.name,
                fs::RenameFlags::EXCHANGE,
            )
        });
        if let Err(error) = exchange {
            let reason = format!(
                "Atomic exchange returned an error ({error}). Check both file locations and whether the filesystem supports atomic exchange."
            );
            return Ok(match intent.placement(&handle, &stage) {
                Placement::Before => intent.retire_before(&state, reason),
                Placement::Exchanged => intent.needs_recovery(ExchangeStatus::Completed, reason),
                Placement::Conflict => intent.needs_recovery(ExchangeStatus::Unknown, reason),
            });
        }
        if let Err(error) = at(Boundary::Exchanged) {
            return Ok(intent.needs_recovery(ExchangeStatus::Completed, error.to_string()));
        }
        Ok(intent.finish_after_exchange(&state, &p.slot, &handle, &stage, at))
    }
}

fn prepare(
    host: &HostContext,
    explicit: Option<&Path>,
    id: &str,
    profiles: &[String],
) -> Result<Prepared> {
    let declaration = read_declaration(host, explicit, id, profiles)?;
    let Some(prior) = declaration.record.load().map_err(|_| record::invalid())? else {
        return Err(Error::Invalid(
            "Only files with a deployment record can be updated.".into(),
        ));
    };
    if !declaration.mode.permits_update() {
        return Err(Error::Invalid("Set permissions that let the owner read the file and stop group and others from writing it. Then review again.".into()));
    }
    let path = &declaration.destination;
    let handle = &declaration.handle;
    let stat = if handle.missing {
        Err(Errno::NOENT)
    } else {
        fs::statat(
            &handle.directory,
            &handle.name,
            fs::AtFlags::SYMLINK_NOFOLLOW,
        )
    };
    let stat = stat.map_err(|error| match error {
        Errno::NOENT => ineligible(DriftStatus::LocallyDeleted),
        _ => io_error("inspect deployed file", path, error),
    })?;
    if overlaps_secret_target(
        host,
        declaration.repository(),
        &declaration.manifest,
        path,
        &stat,
    ) {
        return Err(ineligible(DriftStatus::Unsafe));
    }
    let (old, bytes) = record::read_tracked(path, handle, &stat).map_err(ineligible)?;
    check_file_access(&handle.directory, &handle.name, path, &old)?;
    let (scope, slot, preview, mode) = declaration.render(host)?;
    if preview.destination_status != DestinationStatus::Existing {
        return Err(stale());
    }
    let proposed = Stamp::new(preview.rendered.as_bytes(), mode);
    let status = prior
        .deployed
        .compare(&Stamp::new(&bytes, Mode(old.mode & 0o7777)), &proposed);
    if status != DriftStatus::UpdateAvailable {
        return Err(ineligible(status));
    }
    Ok(Prepared {
        scope,
        slot,
        preview,
        mode,
        prior,
        old,
        bytes,
    })
}

fn ineligible(status: DriftStatus) -> Error {
    Error::Invalid(format!("{}. {}", status.label(), status.guidance()))
}

fn stale() -> Error {
    Error::Invalid(
        "Update changed. Open a new review. Dort did not exchange the generated file.".into(),
    )
}

fn open_destination(path: &Path, parents: &[Parent]) -> Result<ParentHandle> {
    let handle = checked_parent(path)?;
    if handle.missing || !observation::same_parent_identity_and_access(&handle.parents, parents) {
        return Err(Error::Invalid(format!(
            "The reviewed destination folder moved or its access changed. Check {}.",
            path.display()
        )));
    }
    Ok(handle)
}

fn open_stage(directory: &File, name: &str, path: &Path) -> Result<File> {
    let file = fs::openat(
        directory,
        name,
        fs::OFlags::DIRECTORY | fs::OFlags::RDONLY | fs::OFlags::NOFOLLOW | fs::OFlags::CLOEXEC,
        fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(|error| io_error("open private update folder", path, error))?;
    #[cfg(target_os = "macos")]
    super::deploy::acl::require_private_stage(&file, path)?;
    Ok(file)
}

fn stage(
    handle: &ParentHandle,
    name: &str,
    path: &Path,
    p: &Prepared,
) -> Result<(File, Fingerprint, Fingerprint)> {
    let folder = path.with_file_name(name);
    let artifact = folder.join(ARTIFACT);
    fs::mkdirat(&handle.directory, name, fs::Mode::RWXU)
        .map_err(|error| io_error("create private update folder", &folder, error))?;
    let dir = open_stage(&handle.directory, name, &folder)?;
    let stage_fp = stat_file(&dir, &folder)?;
    if stage_fp.uid != rustix::process::geteuid().as_raw() || stage_fp.mode & 0o7777 != 0o700 {
        return Err(Error::Invalid(format!(
            "Update folder {} must have mode 0700 and be owned by you.",
            folder.display()
        )));
    }
    let mut file = fs::openat(
        &dir,
        ARTIFACT,
        fs::OFlags::WRONLY
            | fs::OFlags::CREATE
            | fs::OFlags::EXCL
            | fs::OFlags::NOFOLLOW
            | fs::OFlags::CLOEXEC,
        fs::Mode::RUSR | fs::Mode::WUSR,
    )
    .map(File::from)
    .map_err(|error| io_error("stage generated update", &artifact, error))?;
    #[cfg(target_os = "macos")]
    super::deploy::acl::require_private_stage(&file, &artifact)?;
    file.write_all(p.preview.rendered.as_bytes())
        .map_err(|error| io_error("write staged update", &artifact, error))?;
    fs::fchmod(&file, fs::Mode::from_bits_truncate(p.mode.0 as _))
        .map_err(|error| io_error("set staged update permissions", &artifact, error))?;
    sync(&file, &artifact)?;
    let candidate = stat_file(&file, &artifact)?;
    sync(&dir, &folder)?;
    sync(
        &handle.directory,
        path.parent().expect("destination parent"),
    )?;
    Ok((dir, stage_fp, candidate))
}

fn stat_file(file: &File, path: &Path) -> Result<Fingerprint> {
    fs::fstat(file)
        .map(|stat| Fingerprint::from_stat(&stat))
        .map_err(|error| io_error("inspect generated update object", path, error))
}

fn sync(file: &File, path: &Path) -> Result<()> {
    file.sync_all()
        .map_err(|error| io_error("sync generated update object", path, error))
}

fn check_file_access(dir: &File, name: &OsStr, path: &Path, fp: &Fingerprint) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let file = fs::openat(
            dir,
            name,
            fs::OFlags::RDONLY | fs::OFlags::NOFOLLOW | fs::OFlags::CLOEXEC | fs::OFlags::NONBLOCK,
            fs::Mode::empty(),
        )
        .map(File::from)
        .map_err(|err| io_error("check generated file access", path, err))?;
        if &stat_file(&file, path)? != fp {
            return Err(ineligible(DriftStatus::Unverifiable));
        }
        super::deploy::acl::require_private_stage(&file, path)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (dir, name, path, fp);
    Ok(())
}

fn matches_file(
    dir: &File,
    name: &OsStr,
    path: &Path,
    expected: &Fingerprint,
    stamp: &Stamp,
    exchanged: bool,
) -> bool {
    let Ok(stat) = fs::statat(dir, name, fs::AtFlags::SYMLINK_NOFOLLOW) else {
        return false;
    };
    let fp = Fingerprint::from_stat(&stat);
    let mut comparable = fp.clone();
    if exchanged {
        comparable.changed = expected.changed;
    }
    if &comparable != expected
        || !record::owned_regular(&stat, &fp)
        || fp.mode & 0o400 == 0
        || check_file_access(dir, name, path, &fp).is_err()
    {
        return false;
    }
    observation::read_at_checked(dir, name, path, &fp, OUTPUT_LIMIT as u64).is_ok_and(|bytes| {
        Stamp::new(&bytes, Mode(fp.mode & 0o7777)) == *stamp
            && !observation::sensitive_content(&bytes)
    })
}

impl Intent {
    fn persist(&self, state: &PrivateDir) -> Result<()> {
        let text = serde_json::to_string(self).map_err(|_| invalid_intent(state))?;
        if text.len() as u64 > INTENT_LIMIT {
            return Err(invalid_intent(state));
        }
        state.replace(PENDING, &text)
    }

    fn destination(&self) -> &Path {
        Path::new(&self.identity.destination)
    }

    fn stage_name(&self) -> String {
        format!("{STAGE_PREFIX}{}", self.id)
    }

    fn location(&self) -> UpdateLocation {
        UpdateLocation {
            destination: self.destination().to_owned(),
            artifact: self
                .destination()
                .with_file_name(self.stage_name())
                .join(ARTIFACT),
            stage_device: self.stage.device,
            stage_inode: self.stage.inode,
        }
    }

    fn terminal_receipt(&self) -> DeploymentRecord {
        DeploymentRecord {
            deployed: self.proposed.clone(),
            retained: Some(self.location().artifact),
        }
    }

    fn placement(&self, handle: &ParentHandle, stage: &File) -> Placement {
        let location = self.location();
        let live = |fp, stamp, exchanged| {
            matches_file(
                &handle.directory,
                &handle.name,
                self.destination(),
                fp,
                stamp,
                exchanged,
            )
        };
        let private = |fp, stamp, exchanged| {
            matches_file(
                stage,
                OsStr::new(ARTIFACT),
                &location.artifact,
                fp,
                stamp,
                exchanged,
            )
        };
        let placement = if live(&self.old, &self.prior.deployed, false)
            && private(&self.candidate, &self.proposed, false)
        {
            Placement::Before
        } else if private(&self.old, &self.prior.deployed, true)
            && live(&self.candidate, &self.proposed, true)
        {
            Placement::Exchanged
        } else {
            return Placement::Conflict;
        };
        let reachable = open_destination(self.destination(), &self.scope.destination_parents)
            .and_then(|current| self.open_stage(&current));
        if reachable.is_err() {
            return Placement::Conflict;
        }
        placement
    }

    fn open_stage(&self, handle: &ParentHandle) -> Result<File> {
        let folder = self.destination().with_file_name(self.stage_name());
        let stage = open_stage(&handle.directory, &self.stage_name(), &folder)?;
        let fp = stat_file(&stage, &folder)?;
        let expected = &self.stage;
        if (fp.device, fp.inode, fp.mode, fp.uid, fp.gid)
            != (
                expected.device,
                expected.inode,
                expected.mode,
                expected.uid,
                expected.gid,
            )
        {
            return Err(Error::Invalid(format!(
                "The private update folder changed. Check {}.",
                folder.display()
            )));
        }
        Ok(stage)
    }

    fn needs_recovery(&self, exchange: ExchangeStatus, reason: String) -> UpdateOutcome {
        UpdateOutcome::NeedsRecovery {
            location: self.location(),
            exchange,
            reason,
        }
    }

    fn retire_before(&self, state: &PrivateDir, reason: String) -> UpdateOutcome {
        match clear_pending(state) {
            Ok(()) => UpdateOutcome::NotExchanged {
                location: self.location(),
                reason,
            },
            Err(error) => {
                self.needs_recovery(ExchangeStatus::NotPerformed, format!("{reason} {error}"))
            }
        }
    }

    fn finish_after_exchange(
        &self,
        state: &PrivateDir,
        slot: &Slot,
        handle: &ParentHandle,
        stage: &File,
        mut at: impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> UpdateOutcome {
        let result = (|| {
            let location = self.location();
            sync(
                &handle.directory,
                location.destination.parent().expect("destination parent"),
            )?;
            sync(stage, location.artifact.parent().expect("artifact parent"))?;
            if slot.load()?.as_ref() != Some(&self.prior)
                || self.placement(handle, stage) != Placement::Exchanged
            {
                return Err(conflict());
            }
            at(Boundary::CommitReceipt).map_err(|error| {
                io_error(
                    "commit generated deployment record",
                    &location.destination,
                    error,
                )
            })?;
            slot.replace(self.terminal_receipt())?;
            at(Boundary::Receipt).map_err(|error| {
                io_error(
                    "check committed generated update",
                    &location.destination,
                    error,
                )
            })?;
            clear_pending(state)
        })();
        match result {
            Ok(()) => UpdateOutcome::Updated {
                location: self.location(),
            },
            Err(error) => self.needs_recovery(ExchangeStatus::Completed, error.to_string()),
        }
    }
}

fn conflict() -> Error {
    Error::Invalid("The generated file, private update folder, or deployment record changed. Check both file locations and keep their contents. Dort will not exchange them again. If the destination folder moved, find the private folder by its device and inode.".into())
}

fn clear_pending(state: &PrivateDir) -> Result<()> {
    fs::unlinkat(&state.file, PENDING, fs::AtFlags::empty()).map_err(|error| {
        io_error(
            "clear pending generated update",
            state.path.join(PENDING),
            error,
        )
    })?;
    state.sync()
}

fn invalid_intent(state: &PrivateDir) -> Error {
    Error::Invalid(format!(
        "Cannot verify the pending generated update. Keep {} and the private update folder for inspection.",
        state.path.join(PENDING).display()
    ))
}

fn load_pending(state: &PrivateDir, host: &HostContext) -> Result<Option<(Intent, Slot)>> {
    let path = state.path.join(PENDING);
    let stat = match fs::statat(&state.file, PENDING, fs::AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(io_error("inspect pending generated update", path, error)),
    };
    let fp = Fingerprint::from_stat(&stat);
    if !record::owned_regular(&stat, &fp) || fp.mode & 0o7077 != 0 {
        return Err(invalid_intent(state));
    }
    let bytes =
        observation::read_at_checked(&state.file, OsStr::new(PENDING), &path, &fp, INTENT_LIMIT)?;
    let intent: Intent = serde_json::from_slice(&bytes).map_err(|_| invalid_intent(state))?;
    let slot = Slot::new(
        host,
        &intent.identity.repository_id,
        &intent.identity.mapping_id,
        intent.destination(),
    )
    .map_err(|_| invalid_intent(state))?;
    if intent.schema != 1
        || intent.identity != slot.identity
        || intent.scope.repository_id != intent.identity.repository_id
        || intent.id.len() != 64
        || !intent
            .id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || !intent.proposed.valid()
        || !Mode(intent.proposed.mode).permits_update()
        || !intent.prior.deployed.valid()
        || !Mode(intent.prior.deployed.mode).permits_update()
        || intent.proposed == intent.prior.deployed
    {
        return Err(invalid_intent(state));
    }
    Ok(Some((intent, slot)))
}

pub fn pending_update(host: &HostContext) -> Result<Option<UpdateLocation>> {
    let Some(state) = PrivateDir::open(&host.state_dir, false)? else {
        return Ok(None);
    };
    Ok(load_pending(&state, host)?.map(|(intent, _)| intent.location()))
}

pub(crate) fn ensure_no_pending_update(host: &HostContext) -> Result<()> {
    if let Some(location) = pending_update(host)? {
        return Err(Error::Invalid(format!(
            "A generated update for {} needs recovery before Dort can write again. Its private file is at {} if the folder has not moved. Open Recovery or run `dort recovery recover-update --dry-run`.",
            location.destination.display(),
            location.artifact.display(),
        )));
    }
    Ok(())
}

fn open_locked_state(host: &HostContext) -> Result<PrivateDir> {
    PrivateDir::open(&host.state_dir, false)?.ok_or_else(|| {
        Error::Invalid(format!(
            "Dort's state folder moved. Check {}.",
            host.state_dir.display()
        ))
    })
}

pub fn recover_update(host: &HostContext) -> Result<Option<UpdateOutcome>> {
    recover_pending(host, None)
}
pub fn recover_reviewed_update(
    host: &HostContext,
    reviewed: &UpdateLocation,
) -> Result<Option<UpdateOutcome>> {
    recover_pending(host, Some(reviewed))
}
fn recover_pending(
    host: &HostContext,
    reviewed: Option<&UpdateLocation>,
) -> Result<Option<UpdateOutcome>> {
    let _lock = WriterLock::acquire_for_generated_recovery(host)?;
    let state = open_locked_state(host)?;
    let Some((intent, slot)) = load_pending(&state, host)? else {
        return Ok(None);
    };
    if reviewed.is_some_and(|location| *location != intent.location()) {
        return Err(Error::Invalid(
            "A different generated update now needs recovery. Open a new recovery review.".into(),
        ));
    }
    let outcome = (|| {
        let receipt = slot.load()?;
        if receipt.as_ref() == Some(&intent.terminal_receipt()) {
            return Ok(match slot.sync().and_then(|()| clear_pending(&state)) {
                Ok(()) => UpdateOutcome::Updated {
                    location: intent.location(),
                },
                Err(error) => intent.needs_recovery(ExchangeStatus::Completed, error.to_string()),
            });
        }
        if receipt.as_ref() != Some(&intent.prior) {
            return Err(conflict());
        }
        let handle = open_destination(intent.destination(), &intent.scope.destination_parents)?;
        let stage = intent.open_stage(&handle)?;
        match intent.placement(&handle, &stage) {
            Placement::Before => Ok(intent.retire_before(
                &state,
                "The update stopped before exchange. Open a new review to try again.".into(),
            )),
            Placement::Exchanged => {
                Ok(intent.finish_after_exchange(&state, &slot, &handle, &stage, |_| Ok(())))
            }
            Placement::Conflict => Err(conflict()),
        }
    })()
    .unwrap_or_else(|error| intent.needs_recovery(ExchangeStatus::Unknown, error.to_string()));
    Ok(Some(outcome))
}
