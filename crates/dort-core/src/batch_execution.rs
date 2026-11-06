//! Schema-4 guarded execution and whole-transaction rollback.
pub use crate::adoption::{Boundary, Event};
use crate::{
    Error, Manifest, Result,
    adoption::{self, anchors, canonical, checked_bytes, parent_identity, same},
    batch::{self, Action, Approval},
    batch_journal::{self as wire, Header, Journal, Mutation, Operation, Role, Watch},
    context::HostContext,
    error::io_error,
    journal::{self, Phase, Progress},
    observation::{self, Entry, Fingerprint, Kind},
    planner::{self, Outcome, Request, UnmanageMode},
    state::WriterLock,
};
use rustix::fs::{self, Mode, OFlags, RenameFlags};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
fn stage_name(id: usize) -> String {
    format!("op-{id:04}")
}
fn stage_path(j: &Journal, id: usize) -> PathBuf {
    j.directory.path.join(stage_name(id))
}
fn inspect(path: &Path, role: Role) -> Result<Entry> {
    if role == Role::Manifest && observation::inspect(path, false)?.kind == Kind::Regular {
        observation::read_manifest(path).map(|(_, e)| e)
    } else {
        observation::inspect(path, true)
    }
}
fn notify(
    j: &Journal,
    boundary: Boundary,
    observer: &mut impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    observer(&Event {
        transaction_id: j.record.header.transaction_id.clone(),
        boundary,
        sequence: j.record.sequence,
    })
}
fn checkpoint(j: &mut Journal, observer: &mut impl FnMut(&Event) -> Result<()>) -> Result<()> {
    j.save()?;
    notify(j, Boundary::Checkpoint, observer)
}
fn stage_file(j: &Journal, id: usize, bytes: &[u8], fp: &Fingerprint) -> Result<()> {
    let name = stage_name(id);
    j.directory.create(&name, bytes)?;
    let file = File::from(
        fs::openat(
            &j.directory.file,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open batch preparation", &j.directory.path, e))?,
    );
    fs::fchown(&file, None, Some(rustix::process::Gid::from_raw(fp.gid)))
        .map_err(|e| io_error("preserve prepared group", &j.directory.path, e))?;
    fs::fchmod(&file, Mode::from_bits_truncate((fp.mode & 0o777) as _))
        .map_err(|e| io_error("preserve prepared permissions", &j.directory.path, e))?;
    file.sync_all()
        .map_err(|e| io_error("sync batch preparation", &j.directory.path, e))?;
    j.directory.sync()
}

/// Parsed tables carry document-global positions. Discard positions imported
/// from the separate addition document so normal insertion order is used.
/// The full semantic equality check below remains authoritative.
fn clear_imported_positions(table: &mut toml_edit::Table) {
    table.set_position(None);
    for (_, item) in table.iter_mut() {
        match item {
            toml_edit::Item::Table(child) => clear_imported_positions(child),
            toml_edit::Item::ArrayOfTables(children) => {
                for child in children.iter_mut() {
                    clear_imported_positions(child);
                }
            }
            _ => (),
        }
    }
}
fn edit_manifest(bytes: &[u8], h: &Header, host: &HostContext) -> Result<Option<Vec<u8>>> {
    if h.action == Action::Apply {
        return Ok(None);
    }
    if h.action != Action::Adopt {
        let mut edited = bytes.to_vec();
        for w in &h.watches {
            edited = crate::lifecycle::remove_mapping(
                &edited,
                &w.mapping.id,
                host,
                &h.repository,
                &h.profiles,
            )?;
        }
        return Ok(Some(edited));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?;
    let mut expected = Manifest::parse(text)?;
    let mappings: Vec<_> = h.watches.iter().map(|w| w.mapping.clone()).collect();
    batch::validate_adoptions(&mut expected, &mappings, host, &h.profiles, &h.repository)?;
    #[derive(serde::Serialize)]
    struct Addition<'a> {
        files: &'a [crate::model::ManagedFile],
    }
    let added = toml::to_string(&Addition { files: &mappings })
        .map_err(|_| Error::Invalid("cannot encode batch mappings".into()))?
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| Error::Invalid("cannot encode batch mapping tables".into()))?;
    let expected = journal::encode(&expected)?;
    // Keep every previously successful edit byte-for-byte: recovery binds the
    // edited-manifest digest in existing schema-4 headers. Only retry an edit
    // rejected by the original semantic guard, never relax that guard.
    for reset_positions in [false, true] {
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| Error::Invalid("cannot edit manifest".into()))?;
        let mut tables = added["files"]
            .as_array_of_tables()
            .ok_or_else(|| Error::Invalid("cannot encode batch mapping tables".into()))?
            .clone();
        if reset_positions {
            for table in tables.iter_mut() {
                clear_imported_positions(table);
            }
        }
        if doc.get("files").is_none() {
            doc["files"] = toml_edit::Item::ArrayOfTables(tables);
        } else if let Some(array) = doc["files"].as_array_of_tables_mut() {
            for table in &tables {
                array.push(table.clone());
            }
        } else if let Some(array) = doc["files"].as_array_mut() {
            for table in &tables {
                array.push(table.clone().into_inline_table());
            }
        } else {
            return Err(Error::Invalid(
                "unsupported manifest files representation".into(),
            ));
        }
        let edited = doc.to_string();
        if let Ok(parsed) = Manifest::parse(&edited)
            && journal::encode(&parsed)? == expected
        {
            parsed.validate_for_host(host, &h.profiles, Some(&h.repository))?;
            return Ok(Some(edited.into_bytes()));
        }
    }
    Err(Error::Invalid(
        "batch edit changed unrelated manifest declarations".into(),
    ))
}

/// Returns None on an entirely correct/empty apply; no transaction/backups are created.
/// One approved request is one transaction. Atomic publications are NOT global atomicity.
pub fn execute(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Option<String>> {
    lock.check_context(host)?;
    journal::ensure_writable(host)?;
    let inputs = approval
        .plan
        .inputs
        .items
        .first()
        .ok_or_else(|| Error::Invalid("empty batch".into()))?;
    if journal::encode(host)? != journal::encode(&inputs.host)? {
        return Err(Error::Invalid("approval belongs to another host".into()));
    }
    let profiles: Vec<_> = inputs.active_profiles.iter().cloned().collect();
    let fresh = batch::plan(batch::observe(
        host,
        &inputs.repository,
        &profiles,
        approval.plan.request(),
    )?);
    if fresh.has_blockers() || journal::digest(canonical(&fresh)?.as_bytes()) != approval.digest {
        return Err(Error::Invalid(
            "stale or substituted batch plan; observe and approve again".into(),
        ));
    }
    let local = crate::local::LocalConfig::load(host)?;
    let repo_id = local
        .repositories
        .iter()
        .find(|(_, r)| r.root == inputs.repository)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| Error::Invalid("batch execution requires a registered repository".into()))?;
    if fresh
        .items
        .iter()
        .flat_map(|p| &p.files)
        .all(|f| matches!(f.outcome, Outcome::NoOp))
    {
        return Ok(None);
    }
    let action = fresh.inputs.action;
    let watches: Vec<_> = fresh
        .inputs
        .items
        .iter()
        .flat_map(|s| &s.files)
        .map(|f| Watch {
            mapping: f.mapping.clone(),
            source: f.source.clone().unwrap(),
            destination: f.destination.clone().unwrap(),
        })
        .collect();
    let first = &fresh.inputs.items[0];
    let mut entries = vec![&first.manifest, &first.local_config];
    for w in &watches {
        entries.extend([&w.source, &w.destination]);
    }
    let mut h = Header {
        repository_id: repo_id,
        repository: inputs.repository.clone(),
        transaction_id: adoption::transaction_id(),
        approved_plan_sha256: approval.digest,
        action,
        acknowledged_ids: if action == Action::Remove {
            watches.iter().map(|w| w.mapping.id.clone()).collect()
        } else {
            vec![]
        },
        new_manifest_sha256: None,
        profiles,
        anchors: anchors(&entries),
        watches,
        manifest: first.manifest.clone(),
        local_config: first.local_config.clone(),
        backups: vec![],
    };
    if h.manifest
        .fingerprint
        .as_ref()
        .is_none_or(|f| f.mode & 0o7000 != 0)
    {
        return Err(Error::Invalid(
            "manifest special permission bits refused".into(),
        ));
    }
    h.backups = wire::backup_specs(&h)?;
    let expected = wire::expected(&h)?;
    // Read bytes and construct/revalidate the focused edit before allocating a transaction.
    let manifest_bytes = checked_bytes(&h.manifest)?;
    let new_manifest = edit_manifest(&manifest_bytes, &h, host)?;
    h.new_manifest_sha256 = new_manifest.as_ref().map(|b| journal::digest(b));
    let mut backup_bytes = vec![];
    for b in &h.backups {
        let e = if b.path == h.manifest.path {
            &h.manifest
        } else {
            &h.watches
                .iter()
                .find(|w| w.destination.path == b.path)
                .unwrap()
                .destination
        };
        let bytes = if e.kind == Kind::Symlink {
            e.link_target
                .as_ref()
                .unwrap()
                .to_str()
                .unwrap()
                .as_bytes()
                .to_vec()
        } else {
            checked_bytes(e)?
        };
        if journal::digest(&bytes) != b.sha256 {
            return Err(Error::Invalid("backup input changed".into()));
        }
        backup_bytes.push(bytes);
    }
    let prepared_bytes = h
        .watches
        .iter()
        .map(|w| {
            if action == Action::Keep {
                checked_bytes(&w.source)
            } else if action == Action::Adopt {
                checked_bytes(&w.destination)
            } else {
                Ok(vec![])
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut j = Journal::begin(host, h)?;
    notify(&j, Boundary::Checkpoint, &mut observer)?;
    for (backup, bytes) in j.record.header.backups.iter().zip(&backup_bytes) {
        j.directory.create(&backup.name, bytes)?;
    }
    j.verify_backups()?;
    let device = Fingerprint::from_stat(
        &fs::fstat(&j.directory.file)
            .map_err(|e| io_error("inspect transaction filesystem", &j.directory.path, e))?,
    )
    .device;
    // Even watched sources stay on this device: consistent with the reviewed constraint.
    for entry in std::iter::once(&j.record.header.manifest).chain(
        j.record
            .header
            .watches
            .iter()
            .flat_map(|w| [&w.source, &w.destination]),
    ) {
        if entry
            .parents
            .last()
            .is_none_or(|p| p.fingerprint.device != device || p.fingerprint.uid != first.user_id)
            || entry
                .fingerprint
                .as_ref()
                .is_some_and(|f| f.device != device)
        {
            return Err(Error::Invalid(
                "batch requires owned parents and one filesystem; no cross-device fallback".into(),
            ));
        }
    }
    for (id, (role, mutation, path)) in expected.into_iter().enumerate() {
        let before = match role {
            Role::Directory => inspect(&path, role)?,
            Role::Manifest => j.record.header.manifest.clone(),
            Role::Source => j
                .record
                .header
                .watches
                .iter()
                .find(|w| w.source.path == path)
                .unwrap()
                .source
                .clone(),
            Role::Destination => j
                .record
                .header
                .watches
                .iter()
                .find(|w| w.destination.path == path)
                .unwrap()
                .destination
                .clone(),
        };
        match role {
            Role::Directory => {
                fs::mkdirat(
                    &j.directory.file,
                    stage_name(id).as_str(),
                    Mode::from_raw_mode(0o700),
                )
                .map_err(|e| io_error("prepare destination parent", &path, e))?;
                let dir = File::from(
                    fs::openat(
                        &j.directory.file,
                        stage_name(id).as_str(),
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|e| io_error("open prepared parent", &path, e))?,
                );
                dir.sync_all()
                    .map_err(|e| io_error("sync prepared parent", &path, e))?;
            }
            Role::Manifest => stage_file(
                &j,
                id,
                new_manifest.as_ref().unwrap(),
                before.fingerprint.as_ref().unwrap(),
            )?,
            Role::Source => {
                let index = j
                    .record
                    .header
                    .watches
                    .iter()
                    .position(|w| w.source.path == path)
                    .unwrap();
                stage_file(
                    &j,
                    id,
                    &prepared_bytes[index],
                    j.record.header.watches[index]
                        .destination
                        .fingerprint
                        .as_ref()
                        .unwrap(),
                )?;
            }
            Role::Destination => match action {
                Action::Apply | Action::Adopt => {
                    let w = j
                        .record
                        .header
                        .watches
                        .iter()
                        .find(|w| w.destination.path == path)
                        .unwrap();
                    fs::symlinkat(&w.source.path, &j.directory.file, stage_name(id).as_str())
                        .map_err(|e| io_error("prepare managed link", &path, e))?;
                }
                Action::Keep => {
                    let index = j
                        .record
                        .header
                        .watches
                        .iter()
                        .position(|w| w.destination.path == path)
                        .unwrap();
                    stage_file(
                        &j,
                        id,
                        &prepared_bytes[index],
                        j.record.header.watches[index]
                            .source
                            .fingerprint
                            .as_ref()
                            .unwrap(),
                    )?;
                }
                Action::Remove => {}
            },
        }
        j.directory.sync()?;
        let staged = inspect(&stage_path(&j, id), role)?;
        j.record.operations.push(Operation {
            id,
            role,
            mutation,
            before,
            staged,
            after: None,
            progress: Progress::Pending,
        });
    }
    j.record.phase = Phase::Running;
    checkpoint(&mut j, &mut observer)?;
    validate_scope(host, &j)?;
    for id in 0..j.record.operations.len() {
        validate_layout(host, &j)?;
        j.record.operations[id].progress = Progress::Intent;
        checkpoint(&mut j, &mut observer)?;
        validate_layout(host, &j)?;
        mutate(&j, id, false)?;
        notify(&j, Boundary::Mutation, &mut observer)?;
        let op = &j.record.operations[id];
        let after = inspect(&op.before.path, op.role)?;
        let ok = if op.mutation == Mutation::Withdraw {
            after.kind == Kind::Missing
        } else {
            same(&op.staged, &after, false)
        };
        if !ok {
            return Err(Error::Invalid("batch postcondition mismatch".into()));
        }
        j.record.operations[id].after = Some(after);
        j.record.operations[id].progress = Progress::Done;
        checkpoint(&mut j, &mut observer)?;
    }
    j.record.phase = Phase::Verifying;
    checkpoint(&mut j, &mut observer)?;
    validate_layout(host, &j)?;
    j.record.phase = Phase::Completed;
    checkpoint(&mut j, &mut observer)?;
    Ok(Some(j.record.header.transaction_id))
}

fn check_parents(j: &Journal, entry: &Entry) -> Result<()> {
    for p in &entry.parents {
        let expected = j
            .record
            .header
            .anchors
            .iter()
            .find(|e| e.path == p.path)
            .map(|e| &e.fingerprint)
            .or_else(|| {
                j.record
                    .operations
                    .iter()
                    .find(|o| o.role == Role::Directory && o.before.path == p.path)
                    .and_then(|o| o.staged.fingerprint.as_ref())
            });
        if expected.is_none_or(|e| !parent_identity(e, &p.fingerprint)) {
            return Err(Error::Invalid(
                "parent identity/permissions changed; batch refused".into(),
            ));
        }
    }
    Ok(())
}
fn applied(j: &Journal, op: &Operation) -> Result<bool> {
    let live = inspect(&op.before.path, op.role)?;
    check_parents(j, &live)?;
    let stage = inspect(&stage_path(j, op.id), op.role)?;
    let strict = op.progress == Progress::Pending;
    let before = same(&op.before, &live, strict) && same(&op.staged, &stage, strict);
    let after = match op.mutation {
        Mutation::Publish => same(&op.staged, &live, false) && stage.kind == Kind::Missing,
        Mutation::Exchange => same(&op.staged, &live, false) && same(&op.before, &stage, false),
        Mutation::Withdraw => live.kind == Kind::Missing && same(&op.before, &stage, false),
    };
    match op.progress {
        Progress::Pending if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if after => Ok(true),
        Progress::Done if after && op.after.as_ref().is_some_and(|e| same(e, &live, true)) => {
            Ok(true)
        }
        Progress::Undone if before => Ok(false),
        _ => Err(Error::Invalid(
            "batch guard mismatch (file, stage or parent edited); recovery refused".into(),
        )),
    }
}
fn validate_layout(host: &HostContext, j: &Journal) -> Result<()> {
    let h = &j.record.header;
    journal::registered_root(host, &h.repository_id, &h.repository)?;
    j.verify_backups()?;
    let config = observation::inspect(&host.local_config_path(), false)?;
    if !same(&h.local_config, &config, true) {
        return Err(Error::Invalid(
            "local configuration changed since approval".into(),
        ));
    }
    check_parents(j, &config)?;
    for w in &h.watches {
        if h.action != Action::Adopt {
            let source = observation::inspect(&w.source.path, true)?;
            if !same(&w.source, &source, true) {
                return Err(Error::Invalid(
                    "repository source edited; batch/recovery refused".into(),
                ));
            }
            check_parents(j, &source)?;
        }
        if !wire::changed(h, w) {
            let current = observation::inspect(&w.destination.path, true)?;
            if !same(&w.destination, &current, true) {
                return Err(Error::Invalid("unchanged mapping drifted".into()));
            }
            check_parents(j, &current)?;
        }
    }
    if h.action == Action::Apply {
        let manifest = observation::read_manifest(&h.manifest.path)?.1;
        if !same(&h.manifest, &manifest, true) {
            return Err(Error::Invalid("manifest edited since approval".into()));
        }
        check_parents(j, &manifest)?;
    }
    let states: Vec<_> = j
        .record
        .operations
        .iter()
        .map(|o| applied(j, o))
        .collect::<Result<_>>()?;
    for op in j
        .record
        .operations
        .iter()
        .filter(|o| o.role == Role::Directory)
    {
        let path = if states[op.id] {
            op.before.path.clone()
        } else {
            stage_path(j, op.id)
        };
        let mut expected: Vec<_> = j
            .record
            .operations
            .iter()
            .filter(|child| {
                states[op.id]
                    && states[child.id]
                    && child.before.path.parent() == Some(op.before.path.as_path())
            })
            .map(|o| o.before.path.file_name().unwrap().to_owned())
            .collect();
        expected.sort();
        if observation::names(&path, 4096)? != expected {
            return Err(Error::Invalid(
                "created/prepared destination directory contains unapproved entries".into(),
            ));
        }
    }
    Ok(())
}
fn mutate(j: &Journal, id: usize, undo: bool) -> Result<()> {
    let op = &j.record.operations[id];
    let live = observation::parent_handle(&op.before.path)?;
    if live.missing {
        return Err(Error::Invalid("operation parent is missing".into()));
    }
    let current = inspect(&op.before.path, op.role)?;
    check_parents(j, &current)?;
    if live.parents.len() != current.parents.len()
        || live
            .parents
            .iter()
            .zip(&current.parents)
            .any(|(a, b)| a.path != b.path || !parent_identity(&a.fingerprint, &b.fingerprint))
    {
        return Err(Error::Invalid(
            "parent substituted during batch mutation".into(),
        ));
    }
    if applied(j, op)? != undo {
        return Err(Error::Invalid("batch precondition changed".into()));
    }
    if undo && op.role == Role::Directory && !observation::names(&op.before.path, 1)?.is_empty() {
        return Err(Error::Invalid(
            "created destination parent is not empty".into(),
        ));
    }
    let into_private = match op.mutation {
        Mutation::Publish => undo,
        Mutation::Exchange => false,
        Mutation::Withdraw => !undo,
    };
    let flags = if op.mutation == Mutation::Exchange {
        RenameFlags::EXCHANGE
    } else {
        RenameFlags::NOREPLACE
    };
    let result = if into_private {
        fs::renameat_with(
            &live.directory,
            &live.name,
            &j.directory.file,
            stage_name(id).as_str(),
            flags,
        )
    } else {
        fs::renameat_with(
            &j.directory.file,
            stage_name(id).as_str(),
            &live.directory,
            &live.name,
            flags,
        )
    };
    result.map_err(|e| {
        io_error(
            "publish/invert batch rename (no unsafe fallback)",
            &op.before.path,
            e,
        )
    })?;
    live.directory
        .sync_all()
        .map_err(|e| io_error("sync batch parent", &op.before.path, e))?;
    j.directory.sync()
}

fn selected_mapping(a: &crate::model::ManagedFile, b: &crate::model::ManagedFile) -> bool {
    journal::encode(a).ok() == journal::encode(b).ok()
}

// Independently bind journal paths to a validated original manifest and host protections.
fn validate_scope(host: &HostContext, j: &Journal) -> Result<()> {
    let h = &j.record.header;
    journal::registered_root(host, &h.repository_id, &h.repository)?;
    observation::validate_output_path(&h.repository)?;
    if h.manifest.path != h.repository.join("dort.toml")
        || h.local_config.path != host.local_config_path()
    {
        return Err(Error::Invalid("batch journal scope mismatch".into()));
    }
    // Preparing can only create private artifacts, including incomplete backups.
    if j.record.operations.is_empty() {
        return Ok(());
    }
    j.verify_backups()?;
    let bytes = if h.action == Action::Apply {
        checked_bytes(&h.manifest)?
    } else {
        let b = h
            .backups
            .iter()
            .find(|b| b.path == h.manifest.path)
            .ok_or_else(|| Error::Invalid("missing manifest backup".into()))?;
        journal::read_bytes(&j.directory, &b.name)?
    };
    let manifest = Manifest::parse(
        std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?,
    )?;
    let active = manifest.validate_for_host(host, &h.profiles, Some(&h.repository))?;
    let mappings: Vec<_> = h.watches.iter().map(|w| w.mapping.clone()).collect();
    if h.action == Action::Adopt {
        let mut combined = manifest.clone();
        batch::validate_adoptions(&mut combined, &mappings, host, &h.profiles, &h.repository)?;
    } else {
        for mapping in &mappings {
            if !manifest.files.iter().any(|m| {
                m.id == mapping.id
                    && selected_mapping(m, mapping)
                    && crate::profiles::selected(&m.profiles, &active)
            }) {
                return Err(Error::Invalid(
                    "batch watches do not match active original mappings".into(),
                ));
            }
        }
    }
    let mut snapshots = vec![];
    for w in &h.watches {
        let source = h.repository.join(
            w.mapping
                .overrides
                .get(&host.platform)
                .unwrap_or(&w.mapping.source),
        );
        if w.source.path != source
            || w.destination.path != host.target_path(&w.mapping.target, Some(&h.repository))?
        {
            return Err(Error::Invalid(
                "batch journal mapping scope mismatch".into(),
            ));
        }
        snapshots.push(planner::Snapshot {
            user_id: rustix::process::geteuid().as_raw(),
            host: host.clone(),
            repository: h.repository.clone(),
            manifest: h.manifest.clone(),
            local_config: h.local_config.clone(),
            active_profiles: active.clone(),
            request: match h.action {
                Action::Adopt => Request::Adopt {
                    mapping: w.mapping.clone(),
                },
                Action::Apply => Request::Apply,
                Action::Keep | Action::Remove => Request::Unmanage {
                    id: w.mapping.id.clone(),
                    mode: if h.action == Action::Keep {
                        UnmanageMode::Keep
                    } else {
                        UnmanageMode::Remove
                    },
                },
            },
            files: vec![planner::FileSnapshot {
                mapping: w.mapping.clone(),
                source_path: w.source.path.clone(),
                target_path: w.destination.path.clone(),
                source: Some(w.source.clone()),
                destination: Some(w.destination.clone()),
                blockers: vec![],
            }],
        });
    }
    let plan = batch::plan(batch::Snapshot {
        action: h.action,
        items: snapshots,
        blockers: vec![],
    });
    if plan.has_blockers()
        || journal::digest(canonical(&plan)?.as_bytes()) != h.approved_plan_sha256
    {
        return Err(Error::Invalid(
            "batch journal does not reconstruct approved combined plan".into(),
        ));
    }
    let edited = edit_manifest(&bytes, h, host)?;
    if edited.as_ref().map(|b| journal::digest(b)) != h.new_manifest_sha256 {
        return Err(Error::Invalid(
            "batch manifest edit binding mismatch".into(),
        ));
    }
    let expected = wire::expected(h)?;
    if j.record.operations.len() != expected.len() {
        return Err(Error::Invalid("batch operation scope mismatch".into()));
    }
    for (op, (role, mutation, path)) in j.record.operations.iter().zip(expected) {
        if op.role != role
            || op.mutation != mutation
            || op.before.path != path
            || op.staged.path != stage_path(j, op.id)
        {
            return Err(Error::Invalid("batch operation path/order mismatch".into()));
        }
    }
    Ok(())
}

/// Read-only preflight and conservative inverse paths for the head-bound recovery service.
pub(crate) fn rollback_preview(
    host: &HostContext,
    repository_id: &str,
    transaction_id: &str,
) -> Result<(String, Vec<PathBuf>)> {
    let j = Journal::open(host, repository_id, transaction_id)?;
    validate_scope(host, &j)?;
    if !matches!(j.record.phase, Phase::Preparing | Phase::RolledBack) {
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

/// Explicit/restartable guarded rollback of a schema-4 selected batch.
/// Older schemas retain their independent executors and recovery validation.
pub fn rollback(
    host: &HostContext,
    lock: &WriterLock,
    repository_id: &str,
    transaction_id: &str,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    lock.check_context(host)?;
    let mut j = Journal::open(host, repository_id, transaction_id)?;
    validate_scope(host, &j)?;
    if j.record.phase == Phase::RolledBack {
        return Ok(());
    }
    if j.record.phase == Phase::Preparing {
        j.record.phase = Phase::RolledBack;
        return checkpoint(&mut j, &mut observer);
    }
    validate_layout(host, &j)?;
    if j.record.phase != Phase::RollingBack {
        j.record.phase = Phase::RollingBack;
        checkpoint(&mut j, &mut observer)?;
    }
    for id in (0..j.record.operations.len()).rev() {
        if j.record.operations[id].progress == Progress::Undone {
            continue;
        }
        validate_layout(host, &j)?;
        let was_applied = applied(&j, &j.record.operations[id])?;
        j.record.operations[id].progress = Progress::UndoIntent;
        checkpoint(&mut j, &mut observer)?;
        validate_layout(host, &j)?;
        if was_applied {
            mutate(&j, id, true)?;
            notify(&j, Boundary::Mutation, &mut observer)?;
        }
        j.record.operations[id].progress = Progress::Undone;
        checkpoint(&mut j, &mut observer)?;
    }
    validate_layout(host, &j)?;
    j.record.phase = Phase::RolledBack;
    checkpoint(&mut j, &mut observer)
}
