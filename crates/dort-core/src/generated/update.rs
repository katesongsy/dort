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
    transaction: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAction {
    Update,
    Replace,
    Recreate,
}

impl UpdateAction {
    pub fn for_drift(status: DriftStatus) -> Option<Self> {
        match status {
            DriftStatus::UpdateAvailable => Some(Self::Update),
            DriftStatus::LocallyEdited
            | DriftStatus::LocalPermissionsChanged
            | DriftStatus::LocallyEditedAndPermissionsChanged => Some(Self::Replace),
            DriftStatus::LocallyDeleted => Some(Self::Recreate),
            _ => None,
        }
    }
}

#[derive(Serialize)]
pub struct UpdateReview<'a> {
    pub repository: &'a Path,
    pub active_profiles: &'a BTreeSet<String>,
    pub id: &'a str,
    pub source: Option<&'a Path>,
    pub destination: &'a Path,
    pub deployed: &'a [u8],
    pub deployed_permissions: Option<String>,
    pub recorded_permissions: String,
    pub proposed: &'a str,
    pub proposed_permissions: String,
    pub action: UpdateAction,
    pub detected: DriftStatus,
    pub retained: Option<PathBuf>,
}

pub struct UpdateApproval(Update);

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct UpdateLocation {
    pub destination: PathBuf,
    pub artifact: PathBuf,
    pub stage_device: u64,
    pub stage_inode: u64,
    pub action: UpdateAction,
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
    old: Preimage,
    bytes: Vec<u8>,
    detected: DriftStatus,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum Preimage {
    Local {
        fingerprint: Fingerprint,
        stamp: Stamp,
    },
    Tracked(Fingerprint),
    Missing,
}

impl Preimage {
    fn action(&self) -> UpdateAction {
        match self {
            Self::Tracked(_) => UpdateAction::Update,
            Self::Local { .. } => UpdateAction::Replace,
            Self::Missing => UpdateAction::Recreate,
        }
    }

    fn schema(&self) -> u32 {
        match self {
            Self::Tracked(_) => 1,
            _ => 2,
        }
    }

    fn file<'a>(&'a self, prior: &'a Stamp) -> Option<(&'a Fingerprint, &'a Stamp)> {
        match self {
            Self::Tracked(fp) => Some((fp, prior)),
            Self::Local { fingerprint, stamp } => Some((fingerprint, stamp)),
            Self::Missing => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    identity: Identity,
    id: String,
    scope: Scope,
    stage: Fingerprint,
    old: Preimage,
    candidate: Fingerprint,
    prior: DeploymentRecord,
    proposed: Stamp,
}

#[derive(PartialEq, Eq)]
enum Placement {
    Before,
    Published,
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
        Self::prepare_action(host, explicit, id, temporary_profiles, UpdateAction::Update)
    }

    pub fn prepare_action(
        host: &HostContext,
        explicit: Option<&Path>,
        id: &str,
        temporary_profiles: &[String],
        action: UpdateAction,
    ) -> Result<Self> {
        ensure_no_pending_update(host)?;
        Ok(Self {
            host: host.clone(),
            explicit: explicit.map(Path::to_owned),
            temporary_profiles: temporary_profiles.to_vec(),
            prepared: prepare(host, explicit, id, temporary_profiles, action)?,
            transaction: adoption::transaction_id(),
        })
    }

    pub fn action(&self) -> UpdateAction {
        self.prepared.old.action()
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
            deployed_permissions: p
                .old
                .file(&p.prior.deployed)
                .map(|(_, stamp)| Mode(stamp.mode).label()),
            recorded_permissions: Mode(p.prior.deployed.mode).label(),
            proposed: &p.preview.rendered,
            proposed_permissions: p.mode.label(),
            action: self.action(),
            detected: p.detected,
            retained: (self.action() != UpdateAction::Recreate)
                .then(|| artifact_path(&p.preview.destination, &self.transaction)),
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
            self.action(),
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
    pub fn action(&self) -> UpdateAction {
        self.0.action()
    }

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
        let id = update.transaction.clone();
        let stage_name = format!("{STAGE_PREFIX}{id}");
        let (stage, stage_fp, candidate) = stage(&handle, &stage_name, destination, p)?;
        let intent = Intent {
            schema: p.old.schema(),
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
                "Could not save the generated file plan. Dort did not change the destination. Check {} and {}. {error}",
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
        let recreate = update.action() == UpdateAction::Recreate;
        let (flags, operation) = if recreate {
            (
                fs::RenameFlags::NOREPLACE,
                "Publication without replacement",
            )
        } else {
            (fs::RenameFlags::EXCHANGE, "Atomic exchange")
        };
        if let Err(error) = at(Boundary::Exchange).and_then(|()| {
            fs::renameat_with(&stage, ARTIFACT, &handle.directory, &handle.name, flags)
        }) {
            let reason = format!(
                "{operation} failed ({error}). Check both file locations and filesystem support."
            );
            return Ok(match intent.placement(&handle, &stage) {
                Placement::Before => intent.retire_before(&state, reason),
                Placement::Published => intent.needs_recovery(ExchangeStatus::Completed, reason),
                Placement::Conflict
                    if recreate && error == Errno::EXIST && intent.staged_candidate(&stage) =>
                {
                    intent.retire_before(
                        &state,
                        "A file appeared at the destination. Dort left it alone.".into(),
                    )
                }
                Placement::Conflict => intent.needs_recovery(ExchangeStatus::Unknown, reason),
            });
        }
        if let Err(error) = at(Boundary::Exchanged) {
            return Ok(intent.needs_recovery(ExchangeStatus::Completed, error.to_string()));
        }
        Ok(intent.finish_publication(&state, &p.slot, &handle, &stage, at))
    }
}

fn prepare(
    host: &HostContext,
    explicit: Option<&Path>,
    id: &str,
    profiles: &[String],
    action: UpdateAction,
) -> Result<Prepared> {
    let declaration = read_declaration(host, explicit, id, profiles)?;
    let Some(prior) = declaration.record.load().map_err(|_| record::invalid())? else {
        return Err(Error::Invalid(
            "No deployment record for this generated file.".into(),
        ));
    };
    if !declaration.mode.permits_update() {
        return Err(Error::Invalid("Set permissions that let the owner read the file and stop group and others from writing it. Then review again.".into()));
    }
    let path = &declaration.destination;
    let handle = &declaration.handle;
    if handle.missing {
        return Err(ineligible(DriftStatus::MissingParent));
    }
    let stat = match fs::statat(
        &handle.directory,
        &handle.name,
        fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Ok(stat) => Some(stat),
        Err(Errno::NOENT) => None,
        Err(error) => return Err(io_error("inspect deployed file", path, error)),
    };
    if overlaps_secret_target(
        host,
        declaration.repository(),
        &declaration.manifest,
        path,
        stat.as_ref(),
    ) {
        return Err(ineligible(DriftStatus::Unsafe));
    }
    let file = if let Some(stat) = stat {
        let (fingerprint, bytes) = record::read_tracked(path, handle, &stat).map_err(ineligible)?;
        Some((fingerprint, bytes))
    } else {
        None
    };
    let (scope, slot, preview, mode) = declaration.render(host)?;
    let (old, bytes, detected) = match file {
        Some((fingerprint, bytes)) if preview.destination_status == DestinationStatus::Existing => {
            let stamp = Stamp::new(&bytes, Mode(fingerprint.mode & 0o7777));
            let proposed = Stamp::new(preview.rendered.as_bytes(), mode);
            let detected = prior.deployed.compare(&stamp, &proposed);
            let old = if stamp == prior.deployed {
                Preimage::Tracked(fingerprint)
            } else {
                Preimage::Local { fingerprint, stamp }
            };
            (old, bytes, detected)
        }
        None if preview.destination_status == DestinationStatus::Missing => {
            (Preimage::Missing, Vec::new(), DriftStatus::LocallyDeleted)
        }
        _ => return Err(stale()),
    };
    if UpdateAction::for_drift(detected) != Some(action) {
        return Err(ineligible(detected));
    }
    Ok(Prepared {
        scope,
        slot,
        preview,
        mode,
        prior,
        old,
        bytes,
        detected,
    })
}

fn ineligible(status: DriftStatus) -> Error {
    Error::Invalid(format!("{}. {}", status.label(), status.guidance()))
}

fn stale() -> Error {
    Error::Invalid(
        "The review changed. Open a new review. Dort did not change the destination.".into(),
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

fn missing(dir: &File, name: &OsStr) -> bool {
    matches!(
        fs::statat(dir, name, fs::AtFlags::SYMLINK_NOFOLLOW),
        Err(Errno::NOENT)
    )
}

fn artifact_path(destination: &Path, transaction: &str) -> PathBuf {
    destination
        .with_file_name(format!("{STAGE_PREFIX}{transaction}"))
        .join(ARTIFACT)
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
        || record::check_file_access(dir, name, path, &fp).is_err()
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
            artifact: artifact_path(self.destination(), &self.id),
            stage_device: self.stage.device,
            stage_inode: self.stage.inode,
            action: self.old.action(),
        }
    }

    fn terminal_receipt(&self) -> DeploymentRecord {
        let recreate = self.old.action() == UpdateAction::Recreate;
        DeploymentRecord {
            deployed: self.proposed.clone(),
            retained: (!recreate).then(|| self.location().artifact),
            recreation: recreate.then(|| self.id.clone()),
        }
    }

    fn valid_preimage(&self) -> bool {
        let prior = &self.prior.deployed;
        match &self.old {
            Preimage::Tracked(fp) => {
                fp.mode & 0o7777 == prior.mode
                    && Mode(prior.mode).permits_update()
                    && self.proposed != *prior
            }
            Preimage::Local { fingerprint, stamp } => {
                stamp.valid()
                    && Mode(stamp.mode).permits_update()
                    && fingerprint.mode & 0o7777 == stamp.mode
                    && stamp != prior
            }
            Preimage::Missing => true,
        }
    }

    fn staged_candidate(&self, stage: &File) -> bool {
        self.stage_reachable()
            && matches_file(
                stage,
                OsStr::new(ARTIFACT),
                &self.location().artifact,
                &self.candidate,
                &self.proposed,
                false,
            )
    }

    fn stage_reachable(&self) -> bool {
        open_destination(self.destination(), &self.scope.destination_parents)
            .and_then(|current| self.open_stage(&current))
            .is_ok()
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
        let (before, published) = match self.old.file(&self.prior.deployed) {
            Some((fp, stamp)) => (live(fp, stamp, false), private(fp, stamp, true)),
            None => (
                missing(&handle.directory, &handle.name),
                missing(stage, OsStr::new(ARTIFACT)),
            ),
        };
        if before && private(&self.candidate, &self.proposed, false) && self.stage_reachable() {
            Placement::Before
        } else if published && live(&self.candidate, &self.proposed, true) && self.stage_reachable()
        {
            Placement::Published
        } else {
            Placement::Conflict
        }
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

    fn finish_publication(
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
                || self.placement(handle, stage) != Placement::Published
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
    Error::Invalid("The generated file, private update folder, or deployment record changed. Check both locations and keep any files. Recovery does not change the destination. If the destination folder moved, find the private folder by its device and inode.".into())
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
    if intent.schema != intent.old.schema()
        || intent.identity != slot.identity
        || intent.scope.repository_id != intent.identity.repository_id
        || !record::transaction_id_valid(&intent.id)
        || !intent.proposed.valid()
        || !Mode(intent.proposed.mode).permits_update()
        || !intent.prior.valid()
        || !intent.valid_preimage()
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
            "A generated file change for {} needs recovery before Dort can write again. Check the private path at {}. Open Recovery or run `dort recovery recover-update --dry-run`.",
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
            "A different generated file change now needs recovery. Open a new recovery review."
                .into(),
        ));
    }
    let outcome = (|| {
        let receipt = slot.load()?;
        let handle = open_destination(intent.destination(), &intent.scope.destination_parents)?;
        let stage = intent.open_stage(&handle)?;
        let placement = intent.placement(&handle, &stage);
        if receipt.as_ref() == Some(&intent.terminal_receipt()) {
            if placement != Placement::Published {
                return Err(conflict());
            }
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
        match placement {
            Placement::Before => Ok(intent.retire_before(
                &state,
                "The action stopped before changing the destination. Open a new review to try again.".into(),
            )),
            Placement::Published => {
                Ok(intent.finish_publication(&state, &slot, &handle, &stage, |_| Ok(())))
            }
            Placement::Conflict if intent.old.action() == UpdateAction::Recreate
                && intent.staged_candidate(&stage) => Ok(intent.retire_before(
                    &state,
                    "The staged file was not published. Dort left the destination alone.".into(),
                )),
            Placement::Conflict => Err(conflict()),
        }
    })()
    .unwrap_or_else(|error| intent.needs_recovery(ExchangeStatus::Unknown, error.to_string()));
    Ok(Some(outcome))
}
