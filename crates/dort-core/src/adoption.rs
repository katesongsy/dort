//! Recoverable ONE-file regular-file adoption. No package work, apply or unmanage executor.
//! Caller approval is opaque, fresh observation is mandatory, recovery is explicit.
use crate::{
    Error, Manifest, Result,
    context::HostContext,
    error::io_error,
    journal::{self, Header, Journal, Operation, Phase, Progress, Role},
    model::ManagedFile,
    observation::{self, Entry, Fingerprint, Kind, Parent},
    planner::{self, Plan, Request},
    state::WriterLock,
};
use rustix::fs::{self, Mode, OFlags, RenameFlags};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Not deserializable. A preview JSON document can never be passed directly to execution.
pub struct Approval {
    plan: Plan,
    digest: String,
}

impl Approval {
    /// An explicit caller approval boundary, not an interactive prompt. The caller must
    /// privately review the destination content and present the preview before calling.
    pub fn from_plan(plan: Plan) -> Result<Self> {
        if !matches!(plan.inputs.request, Request::Adopt { .. })
            || plan.files.len() != 1
            || plan.has_blockers()
            || canonical(&plan)? != canonical(&planner::plan(plan.inputs.clone()))?
        {
            return Err(Error::Invalid(
                "approval requires an unmodified, unblocked one-file adoption plan".into(),
            ));
        }
        let digest = journal::digest(canonical(&plan)?.as_bytes());
        Ok(Self { plan, digest })
    }
    pub fn sha256(&self) -> &str {
        &self.digest
    }
}

// Directory timestamps/link counts change when state or siblings are created. Bind their
// identities, ownership and permissions, not unrelated directory churn.
pub(crate) fn canonical(plan: &Plan) -> Result<String> {
    let mut value =
        serde_json::to_value(plan).map_err(|_| Error::Invalid("cannot encode approval".into()))?;
    fn normalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "parents" {
                        if let Some(parents) = value.as_array_mut() {
                            for parent in parents {
                                if let Some(fp) = parent
                                    .get_mut("fingerprint")
                                    .and_then(|f| f.as_object_mut())
                                {
                                    for field in ["size", "modified", "changed", "links"] {
                                        fp.remove(field);
                                    }
                                }
                            }
                        }
                    } else {
                        normalize(value);
                    }
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(normalize),
            _ => {}
        }
    }
    normalize(&mut value);
    journal::encode(&value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    Checkpoint,
    Mutation,
}

/// Progress callbacks also support deterministic error/crash testing without environment
/// switches in production code. Errors leave durable intent; nothing auto-rolls back.
#[derive(Debug)]
pub struct Event {
    pub transaction_id: String,
    pub boundary: Boundary,
    pub sequence: usize,
}

fn notify(
    journal: &Journal,
    boundary: Boundary,
    observer: &mut impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    observer(&Event {
        transaction_id: journal.record.header.transaction_id.clone(),
        boundary,
        sequence: journal.record.sequence,
    })
}
fn checkpoint(
    journal: &mut Journal,
    observer: &mut impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    journal.save()?;
    notify(journal, Boundary::Checkpoint, observer)
}

pub(crate) fn transaction_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    journal::digest(
        format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    )
}

pub(crate) fn parent_identity(a: &Fingerprint, b: &Fingerprint) -> bool {
    a.device == b.device
        && a.inode == b.inode
        && a.mode == b.mode
        && a.uid == b.uid
        && a.gid == b.gid
}

pub(crate) fn same(a: &Entry, b: &Entry, strict: bool) -> bool {
    if a.kind != b.kind || a.link_target != b.link_target || a.sha256 != b.sha256 {
        return false;
    }
    match (&a.fingerprint, &b.fingerprint) {
        (None, None) => true,
        (Some(a), Some(b)) if a.mode & 0o170000 == 0o040000 => parent_identity(a, b),
        (Some(a), Some(b)) => {
            let mut a = a.clone();
            let mut b = b.clone();
            if !strict {
                a.changed = (0, 0);
                b.changed = (0, 0);
            }
            a == b
        }
        _ => false,
    }
}

fn inspect(path: &Path, role: Role) -> Result<Entry> {
    if role == Role::Manifest && observation::inspect(path, false)?.kind == Kind::Regular {
        observation::read_manifest(path).map(|(_, entry)| entry)
    } else {
        observation::inspect(path, true)
    }
}

pub(crate) fn checked_bytes(entry: &Entry) -> Result<Vec<u8>> {
    let handle = observation::parent_handle(&entry.path)?;
    let bytes = observation::read_checked(
        &handle,
        &entry.path,
        entry
            .fingerprint
            .as_ref()
            .ok_or_else(|| Error::Invalid("missing regular-file fingerprint".into()))?,
    )?;
    if entry.sha256.as_deref() != Some(&journal::digest(&bytes)) {
        return Err(Error::Invalid("approved content changed".into()));
    }
    Ok(bytes)
}

fn stage_name(id: usize) -> String {
    format!("op-{id:04}")
}
fn stage_path(journal: &Journal, id: usize) -> PathBuf {
    journal.directory.path.join(stage_name(id))
}

fn stage_file(journal: &Journal, id: usize, bytes: &[u8], mode: u32, gid: u32) -> Result<()> {
    let name = stage_name(id);
    journal.directory.create(&name, bytes)?;
    let file = File::from(
        fs::openat(
            &journal.directory.file,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open prepared file", &journal.directory.path, e))?,
    );
    fs::fchown(&file, None, Some(rustix::process::Gid::from_raw(gid)))
        .map_err(|e| io_error("preserve prepared file group", &journal.directory.path, e))?;
    fs::fchmod(&file, Mode::from_bits_truncate(mode as _))
        .map_err(|e| io_error("set prepared file permissions", &journal.directory.path, e))?;
    file.sync_all()
        .map_err(|e| io_error("sync prepared file", &journal.directory.path, e))?;
    journal.directory.sync()
}

pub(crate) fn anchors(entries: &[&Entry]) -> Vec<Parent> {
    let mut all = BTreeMap::new();
    for entry in entries {
        for parent in &entry.parents {
            all.insert(parent.path.clone(), parent.clone());
        }
    }
    all.into_values().collect()
}

fn check_parents(journal: &Journal, entry: &Entry) -> Result<()> {
    for actual in &entry.parents {
        let expected = journal
            .record
            .header
            .anchors
            .iter()
            .find(|p| p.path == actual.path)
            .map(|p| &p.fingerprint)
            .or_else(|| {
                journal
                    .record
                    .operations
                    .iter()
                    .find(|op| op.role == Role::Directory && op.before.path == actual.path)
                    .and_then(|op| op.staged.fingerprint.as_ref())
            });
        if expected.is_none_or(|expected| !parent_identity(expected, &actual.fingerprint)) {
            return Err(Error::Invalid(
                "parent identity/permissions changed; transaction refused".into(),
            ));
        }
    }
    Ok(())
}

fn check_configuration(host: &HostContext, journal: &Journal) -> Result<()> {
    journal::registered_root(
        host,
        &journal.record.header.repository_id,
        &journal.record.header.repository,
    )?;
    let current = observation::inspect(&host.local_config_path(), false)?;
    if !same(&journal.record.header.local_config, &current, true) {
        return Err(Error::Invalid(
            "local configuration changed since approval".into(),
        ));
    }
    check_parents(journal, &current)
}

/// Requires a registered repository and the application lock. Only a freshly revalidated,
/// explicitly approved adoption is accepted. Returns a retained transaction ID.
pub fn execute(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<String> {
    lock.check_context(host)?;
    journal::ensure_writable(host)?;
    let inputs = &approval.plan.inputs;
    if journal::encode(host)? != journal::encode(&inputs.host)? {
        return Err(Error::Invalid(
            "approval belongs to another host context".into(),
        ));
    }
    let profiles: Vec<_> = inputs.active_profiles.iter().cloned().collect();
    let fresh = planner::plan(planner::observe(
        host,
        &inputs.repository,
        &profiles,
        inputs.request.clone(),
    )?);
    if fresh.has_blockers() || journal::digest(canonical(&fresh)?.as_bytes()) != approval.digest {
        return Err(Error::Invalid(
            "stale or substituted adoption plan; observe and approve again".into(),
        ));
    }
    let local = crate::local::LocalConfig::load(host)?;
    let repository_id = local
        .repositories
        .iter()
        .find(|(_, record)| record.root == inputs.repository)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| Error::Invalid("adoption requires a registered repository".into()))?;
    let file = &fresh.inputs.files[0];
    let source = file.source.as_ref().unwrap();
    let destination = file.destination.as_ref().unwrap();
    if fresh
        .inputs
        .manifest
        .fingerprint
        .as_ref()
        .is_none_or(|f| f.mode & 0o7000 != 0)
    {
        return Err(Error::Invalid(
            "manifest must not have special permission bits".into(),
        ));
    }
    let manifest_bytes = checked_bytes(&fresh.inputs.manifest)?;
    let destination_bytes = checked_bytes(destination)?;
    #[derive(Serialize)]
    struct Addition<'a> {
        files: Vec<&'a ManagedFile>,
    }
    let addition = toml::to_string(&Addition {
        files: vec![&file.mapping],
    })
    .map_err(|_| Error::Invalid("cannot encode adoption mapping".into()))?;
    let new_manifest = format!(
        "{}\n{}",
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?,
        addition
    );
    Manifest::parse(&new_manifest)?.validate_for_host(host, &profiles, Some(&inputs.repository))?;
    let header = Header {
        repository_id,
        repository: inputs.repository.clone(),
        transaction_id: transaction_id(),
        approved_plan_sha256: approval.digest,
        mapping: file.mapping.clone(),
        profiles,
        manifest: fresh.inputs.manifest.clone(),
        destination: destination.clone(),
        source: source.clone(),
        local_config: fresh.inputs.local_config.clone(),
        anchors: anchors(&[
            &fresh.inputs.manifest,
            destination,
            source,
            &fresh.inputs.local_config,
        ]),
        manifest_backup_sha256: journal::digest(&manifest_bytes),
        destination_backup_sha256: journal::digest(&destination_bytes),
    };
    let mut journal = Journal::begin(host, header)?;
    notify(&journal, Boundary::Checkpoint, &mut observer)?;
    journal
        .directory
        .create("manifest.backup", &manifest_bytes)?;
    journal
        .directory
        .create("destination.backup", &destination_bytes)?;
    journal.verify_backups()?;
    let device = Fingerprint::from_stat(
        &fs::fstat(&journal.directory.file)
            .map_err(|e| io_error("inspect transaction filesystem", &journal.directory.path, e))?,
    )
    .device;
    // No cross-filesystem fallback: every publication/inverse must be an atomic rename.
    for entry in [&fresh.inputs.manifest, destination, source] {
        if entry
            .parents
            .last()
            .is_none_or(|p| p.fingerprint.device != device)
            || entry
                .fingerprint
                .as_ref()
                .is_some_and(|f| f.device != device)
        {
            return Err(Error::Invalid(
                "adoption requires repository, destination and transaction state on one filesystem"
                    .into(),
            ));
        }
        if entry
            .parents
            .last()
            .is_none_or(|p| p.fingerprint.uid != fresh.inputs.user_id)
        {
            return Err(Error::Invalid(
                "live file parents must be owned by this user".into(),
            ));
        }
    }
    let mut directories = vec![];
    let mut parent = file.source_path.parent().unwrap();
    while observation::inspect(parent, false)?.kind == Kind::Missing {
        directories.push(parent.to_owned());
        parent = parent
            .parent()
            .ok_or_else(|| Error::Invalid("missing repository root".into()))?;
    }
    directories.reverse();
    let mut roles: Vec<_> = directories
        .into_iter()
        .map(|path| (Role::Directory, path))
        .collect();
    roles.extend([
        (Role::Source, file.source_path.clone()),
        (Role::Manifest, inputs.manifest.path.clone()),
        (Role::Destination, file.target_path.clone()),
    ]);
    for (id, (role, path)) in roles.into_iter().enumerate() {
        let name = stage_name(id);
        match role {
            Role::Directory => {
                fs::mkdirat(
                    &journal.directory.file,
                    name.as_str(),
                    Mode::RUSR | Mode::WUSR | Mode::XUSR,
                )
                .map_err(|e| io_error("prepare source parent", &journal.directory.path, e))?;
                let fd = File::from(
                    fs::openat(
                        &journal.directory.file,
                        name.as_str(),
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|e| io_error("open prepared directory", &journal.directory.path, e))?,
                );
                fd.sync_all()
                    .map_err(|e| io_error("sync prepared directory", &journal.directory.path, e))?;
            }
            Role::Source => stage_file(
                &journal,
                id,
                &destination_bytes,
                destination.fingerprint.as_ref().unwrap().mode & 0o777,
                destination.fingerprint.as_ref().unwrap().gid,
            )?,
            Role::Manifest => stage_file(
                &journal,
                id,
                new_manifest.as_bytes(),
                inputs.manifest.fingerprint.as_ref().unwrap().mode & 0o777,
                inputs.manifest.fingerprint.as_ref().unwrap().gid,
            )?,
            Role::Destination => {
                fs::symlinkat(&file.source_path, &journal.directory.file, name.as_str())
                    .map_err(|e| io_error("prepare destination link", &journal.directory.path, e))?
            }
        }
        journal.directory.sync()?;
        let before = match role {
            Role::Source => journal.record.header.source.clone(),
            Role::Manifest => journal.record.header.manifest.clone(),
            Role::Destination => journal.record.header.destination.clone(),
            Role::Directory => inspect(&path, role)?,
        };
        let staged = inspect(&stage_path(&journal, id), role)?;
        journal.record.operations.push(Operation {
            id,
            role,
            before,
            staged,
            after: None,
            progress: Progress::Pending,
        });
    }
    journal.record.phase = Phase::Running;
    checkpoint(&mut journal, &mut observer)?;
    validate_layout(host, &journal)?;
    for id in 0..journal.record.operations.len() {
        check_configuration(host, &journal)?;
        validate_layout(host, &journal)?;
        journal.verify_backups()?;
        journal.record.operations[id].progress = Progress::Intent;
        checkpoint(&mut journal, &mut observer)?;
        validate_layout(host, &journal)?;
        journal.verify_backups()?;
        mutate(&journal, id, false)?;
        notify(&journal, Boundary::Mutation, &mut observer)?;
        let after = inspect(
            &journal.record.operations[id].before.path,
            journal.record.operations[id].role,
        )?;
        if !same(&journal.record.operations[id].staged, &after, false) {
            return Err(Error::Invalid("adoption postcondition mismatch".into()));
        }
        journal.record.operations[id].after = Some(after);
        journal.record.operations[id].progress = Progress::Done;
        checkpoint(&mut journal, &mut observer)?;
    }
    journal.record.phase = Phase::Verifying;
    checkpoint(&mut journal, &mut observer)?;
    validate_layout(host, &journal)?;
    journal.verify_backups()?;
    journal.record.phase = Phase::Completed;
    checkpoint(&mut journal, &mut observer)?;
    Ok(journal.record.header.transaction_id)
}

// Resolve an intent interrupted immediately before/after its atomic rename. Inode,
// content, mode and mtime must match; ctime alone changes as a consequence of rename.
fn applied(journal: &Journal, op: &Operation) -> Result<bool> {
    let live = inspect(&op.before.path, op.role)?;
    check_parents(journal, &live)?;
    let stage = inspect(&stage_path(journal, op.id), op.role)?;
    let before = same(&op.before, &live, op.progress == Progress::Pending)
        && same(&op.staged, &stage, op.progress == Progress::Pending);
    let after = same(&op.staged, &live, false)
        && if matches!(op.role, Role::Directory | Role::Source) {
            stage.kind == Kind::Missing
        } else {
            same(&op.before, &stage, false)
        };
    match op.progress {
        Progress::Pending if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if after => Ok(true),
        Progress::Done
            if after
                && op
                    .after
                    .as_ref()
                    .is_some_and(|expected| same(expected, &live, true)) =>
        {
            Ok(true)
        }
        Progress::Undone if before => Ok(false),
        _ => Err(Error::Invalid(
            "transaction guard mismatch (file, source, stage or parent edited); recovery refused"
                .into(),
        )),
    }
}

fn validate_layout(host: &HostContext, journal: &Journal) -> Result<()> {
    check_configuration(host, journal)?;
    let states: Vec<_> = journal
        .record
        .operations
        .iter()
        .map(|op| applied(journal, op))
        .collect::<Result<_>>()?;
    for op in &journal.record.operations {
        if op.role != Role::Directory {
            continue;
        }
        let path = if states[op.id] {
            op.before.path.clone()
        } else {
            stage_path(journal, op.id)
        };
        let mut expected: Vec<_> = journal
            .record
            .operations
            .iter()
            .filter(|child| {
                states[op.id]
                    && states[child.id]
                    && child.before.path.parent() == Some(op.before.path.as_path())
            })
            .map(|child| child.before.path.file_name().unwrap().to_owned())
            .collect();
        expected.sort();
        if observation::names(&path, 4096)? != expected {
            return Err(Error::Invalid(
                "created/prepared source directory contains unapproved entries; recovery refused"
                    .into(),
            ));
        }
    }
    Ok(())
}

fn mutate(journal: &Journal, id: usize, undo: bool) -> Result<()> {
    let op = &journal.record.operations[id];
    let live = observation::parent_handle(&op.before.path)?;
    if live.missing {
        return Err(Error::Invalid("operation parent is missing".into()));
    }
    let current = inspect(&op.before.path, op.role)?;
    check_parents(journal, &current)?;
    // Re-opened descriptors must refer to the same parents just validated by name.
    if live.parents.len() != current.parents.len() {
        return Err(Error::Invalid(
            "parent disappeared during transaction".into(),
        ));
    }
    for (a, b) in live.parents.iter().zip(&current.parents) {
        if a.path != b.path || !parent_identity(&a.fingerprint, &b.fingerprint) {
            return Err(Error::Invalid(
                "parent substituted during transaction".into(),
            ));
        }
    }
    let is_applied = applied(journal, op)?;
    if is_applied != undo {
        return Err(Error::Invalid(
            "operation is not at the expected precondition".into(),
        ));
    }
    let name = stage_name(id);
    let created = matches!(op.role, Role::Directory | Role::Source);
    if undo && op.role == Role::Directory && !observation::names(&op.before.path, 1)?.is_empty() {
        return Err(Error::Invalid(
            "created source directory is no longer empty".into(),
        ));
    }
    let flags = if created {
        RenameFlags::NOREPLACE
    } else {
        RenameFlags::EXCHANGE
    };
    let result = if undo && created {
        fs::renameat_with(
            &live.directory,
            &live.name,
            &journal.directory.file,
            name.as_str(),
            flags,
        )
    } else {
        fs::renameat_with(
            &journal.directory.file,
            name.as_str(),
            &live.directory,
            &live.name,
            flags,
        )
    };
    result.map_err(|e| {
        io_error(
            "publish/invert journalled rename (no unsafe fallback)",
            &op.before.path,
            e,
        )
    })?;
    live.directory
        .sync_all()
        .map_err(|e| io_error("sync live parent", &op.before.path, e))?;
    journal.directory.sync()
}

/// Read-only preflight and conservative inverse paths for the head-bound recovery service.
pub(crate) fn rollback_preview(
    host: &HostContext,
    repository_id: &str,
    transaction_id: &str,
) -> Result<(String, Vec<PathBuf>)> {
    let j = Journal::open(host, repository_id, transaction_id)?;
    validate_record(host, &j)?;
    if !matches!(j.record.phase, Phase::Preparing | Phase::RolledBack) {
        j.verify_backups()?;
        validate_layout(host, &j)?;
    }
    let effects = if matches!(j.record.phase, Phase::Preparing | Phase::RolledBack) {
        vec![]
    } else {
        j.record
            .operations
            .iter()
            .rev()
            .filter(|o| o.progress != Progress::Undone)
            .map(|o| o.before.path.clone())
            .collect()
    };
    Ok((
        journal::digest(journal::encode(&j.record)?.as_bytes()),
        effects,
    ))
}

/// Explicit guarded rollback, never startup replay. Can also undo a completed adoption.
/// Preflight ALL guards (including linked-source content) before changing anything.
pub fn rollback(
    host: &HostContext,
    lock: &WriterLock,
    repository_id: &str,
    transaction_id: &str,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    lock.check_context(host)?;
    let mut journal = Journal::open(host, repository_id, transaction_id)?;
    validate_record(host, &journal)?;
    if journal.record.phase == Phase::RolledBack {
        return Ok(());
    }
    if journal.record.phase == Phase::Preparing {
        // Only private staging/backups may exist; no live effects can precede Running.
        journal.record.phase = Phase::RolledBack;
        return checkpoint(&mut journal, &mut observer);
    }
    journal.verify_backups()?;
    validate_layout(host, &journal)?;
    if journal.record.phase != Phase::RollingBack {
        journal.record.phase = Phase::RollingBack;
        checkpoint(&mut journal, &mut observer)?;
    }
    for id in (0..journal.record.operations.len()).rev() {
        if journal.record.operations[id].progress == Progress::Undone {
            continue;
        }
        validate_layout(host, &journal)?;
        let was_applied = applied(&journal, &journal.record.operations[id])?;
        journal.record.operations[id].progress = Progress::UndoIntent;
        checkpoint(&mut journal, &mut observer)?;
        validate_layout(host, &journal)?;
        journal.verify_backups()?;
        if was_applied {
            mutate(&journal, id, true)?;
            notify(&journal, Boundary::Mutation, &mut observer)?;
        }
        journal.record.operations[id].progress = Progress::Undone;
        checkpoint(&mut journal, &mut observer)?;
    }
    validate_layout(host, &journal)?;
    journal.record.phase = Phase::RolledBack;
    checkpoint(&mut journal, &mut observer)
}

// Check scope independently of checksums. Journal strings are never arbitrary write paths.
fn validate_record(host: &HostContext, journal: &Journal) -> Result<()> {
    let h = &journal.record.header;
    journal::registered_root(host, &h.repository_id, &h.repository)?;
    observation::validate_output_path(&h.repository)?;
    if h.source.path != h.repository.join(&h.mapping.source)
        || h.destination.path != host.target_path(&h.mapping.target, Some(&h.repository))?
        || h.manifest.path != h.repository.join("dort.toml")
        || h.local_config.path != host.local_config_path()
    {
        return Err(Error::Invalid(
            "journal paths do not match adoption scope".into(),
        ));
    }
    let mut minimal = Manifest::parse("schema=1\n")?;
    minimal.files.push(h.mapping.clone());
    minimal.profiles.available = h
        .profiles
        .iter()
        .chain(&h.mapping.profiles)
        .filter(|p| p.as_str() != "base")
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    minimal.validate_for_host(host, &h.profiles, Some(&h.repository))?;
    if h.mapping.mode != crate::model::FileMode::Link
        || !h.mapping.requires.is_empty()
        || !h.mapping.overrides.is_empty()
    {
        return Err(Error::Invalid("unsupported journal mapping".into()));
    }
    if journal.record.phase != Phase::Preparing
        && !(journal.record.phase == Phase::RolledBack && journal.record.operations.is_empty())
    {
        let n = journal.record.operations.len();
        if n < 3 {
            return Err(Error::Invalid("incomplete adoption operation set".into()));
        }
        let mut previous_dir: Option<&Path> = None;
        for (id, op) in journal.record.operations.iter().enumerate() {
            let (role, path) = if id == n - 3 {
                (Role::Source, &h.source.path)
            } else if id == n - 2 {
                (Role::Manifest, &h.manifest.path)
            } else if id == n - 1 {
                (Role::Destination, &h.destination.path)
            } else {
                if !op.before.path.starts_with(&h.repository)
                    || op.before.path == h.repository
                    || !h.source.path.parent().unwrap().starts_with(&op.before.path)
                    || previous_dir.is_some_and(|p| op.before.path.parent() != Some(p))
                {
                    return Err(Error::Invalid("invalid source parent operation".into()));
                }
                previous_dir = Some(op.before.path.as_path());
                (Role::Directory, &op.before.path)
            };
            if op.id != id
                || op.role != role
                || &op.before.path != path
                || op.staged.path != stage_path(journal, id)
            {
                return Err(Error::Invalid(
                    "invalid journal operation ID/path/order".into(),
                ));
            }
        }
    }
    Ok(())
}
