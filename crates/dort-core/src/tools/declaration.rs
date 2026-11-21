//! Manifest tool declarations.
use crate::{
    Error, Manifest, Result,
    adoption::{self, Boundary, Event, anchors, checked_bytes, parent_identity, same},
    context::HostContext,
    error::io_error,
    journal::{self, Phase, Progress},
    local::LocalConfig,
    model::Tool,
    observation::{self, Entry, Kind},
    state::WriterLock,
    tool_journal::{Header, Journal, Operation},
};
use rustix::fs::{self, Mode, OFlags, RenameFlags};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
const MAX_TOOLS: usize = 4096;
/// Schema-6 manifest operations. Absence retains schema-5 add-only semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    Edit,
    Remove,
}
/// Reports contain typed declarations and identities only, never raw manifest bytes.
#[derive(Serialize)]
pub struct Preview {
    pub schema: u32,
    pub preview_only: bool,
    pub repository: PathBuf,
    pub repository_id: String,
    pub tool: Tool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<Change>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Tool>,
    pub profiles: Vec<String>,
    pub no_op: bool,
    pub effects: Vec<PathBuf>,
    #[serde(skip)]
    host: HostContext,
    #[serde(skip)]
    manifest: Entry,
    #[serde(skip)]
    local_config: Entry,
    #[serde(skip)]
    binding: String,
}
pub struct Approval(Preview);
impl Approval {
    pub fn from_preview(preview: Preview) -> Result<Self> {
        if bind(&preview)? != preview.binding {
            return Err(Error::Invalid(
                "tool declaration preview was modified".into(),
            ));
        }
        Ok(Self(preview))
    }
}
fn bind(p: &Preview) -> Result<String> {
    Ok(journal::digest(
        journal::encode(&(p, &p.host, &p.manifest, &p.local_config))?.as_bytes(),
    ))
}
fn parse(bytes: &[u8]) -> Result<Manifest> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?;
    Manifest::parse(text).map_err(|_| Error::Invalid("invalid manifest (content withheld)".into()))
}
fn entry(path: &Path) -> Result<Entry> {
    let mut e = observation::inspect(path, false)?;
    let fp = e.fingerprint.as_ref().filter(|f| e.kind == Kind::Regular && f.links == 1 && f.uid == rustix::process::geteuid().as_raw() && f.mode & 0o7000 == 0).ok_or_else(|| Error::Invalid("manifest must be an owned single-link regular file without special permission bits".into()))?;
    let handle = observation::parent_handle(path)?;
    let bytes = observation::read_checked(&handle, path, fp)?;
    e.sha256 = Some(journal::digest(&bytes));
    Ok(e)
}
fn clear_positions(table: &mut toml_edit::Table) {
    table.set_position(None);
    for (_, item) in table.iter_mut() {
        match item {
            toml_edit::Item::Table(t) => clear_positions(t),
            toml_edit::Item::ArrayOfTables(a) => {
                for t in a.iter_mut() {
                    clear_positions(t);
                }
            }
            _ => (),
        }
    }
}
/// Pure focused edit. Semantic equality includes every unrelated declaration;
/// unrelated comments/formatting stay in the original document. Identical edits are no-op.
fn edit(
    bytes: &[u8],
    tool: &Tool,
    host: &HostContext,
    profiles: &[String],
    repository: &Path,
    change: Option<Change>,
) -> Result<Option<Vec<u8>>> {
    if journal::encode(tool)?.len() > 32 * 1024 {
        return Err(Error::Invalid("tool declaration exceeds 32 KiB".into()));
    }
    let mut expected = parse(bytes)?;
    let index = expected.tools.iter().position(|t| t.id == tool.id);
    match (change, index) {
        (Some(Change::Remove), Some(i)) => {
            expected.tools.remove(i);
        }
        (Some(Change::Edit), Some(i)) => {
            if journal::encode(&expected.tools[i])? == journal::encode(tool)? {
                return Ok(None);
            }
            expected.tools[i] = tool.clone();
        }
        (Some(_), None) => return Err(Error::Invalid("tool declaration not found".into())),
        (None, Some(i)) => {
            if journal::encode(&expected.tools[i])? == journal::encode(tool)? {
                return Ok(None);
            }
            return Err(Error::Invalid(
                "tool ID already has a different declaration; use edit".into(),
            ));
        }
        (None, None) => {
            if expected.tools.len() >= MAX_TOOLS {
                return Err(Error::Invalid("tool declaration limit".into()));
            }
            expected.tools.push(tool.clone());
        }
    }
    // The original manifest has already passed structural parsing. These are
    // typed declaration diagnostics, never TOML parser excerpts or file bodies.
    expected.validate_for_host(host, profiles, Some(repository))?;
    #[derive(Serialize)]
    struct Addition<'a> {
        tools: [&'a Tool; 1],
    }
    let addition = toml::to_string(&Addition { tools: [tool] })
        .map_err(|_| Error::Invalid("cannot encode tool declaration".into()))?;
    let added = addition
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| Error::Invalid("cannot encode tool tables".into()))?;
    let mut tables = added["tools"]
        .as_array_of_tables()
        .ok_or_else(|| Error::Invalid("missing tool table".into()))?
        .clone();
    for t in tables.iter_mut() {
        clear_positions(t);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| Error::Invalid("cannot edit manifest (content withheld)".into()))?;
    if let Some(i) = index.filter(|_| change.is_some()) {
        if let Some(a) = doc["tools"].as_array_of_tables_mut() {
            if change == Some(Change::Remove) {
                a.remove(i);
            } else {
                let mut replacement = tables.get(0).unwrap().clone();
                replacement.set_position(a.get(i).unwrap().position());
                *a.get_mut(i).unwrap() = replacement;
            }
        } else if let Some(a) = doc["tools"].as_array_mut() {
            if change == Some(Change::Remove) {
                a.remove(i);
            } else {
                a.replace(i, tables.get(0).unwrap().clone().into_inline_table());
            }
        } else {
            return Err(Error::Invalid(
                "unsupported manifest tools representation".into(),
            ));
        }
    } else if doc.get("tools").is_none() {
        doc["tools"] = toml_edit::Item::ArrayOfTables(tables);
    } else if let Some(a) = doc["tools"].as_array_of_tables_mut() {
        for t in &tables {
            a.push(t.clone());
        }
    } else if let Some(a) = doc["tools"].as_array_mut() {
        for t in &tables {
            a.push(t.clone().into_inline_table());
        }
    } else {
        return Err(Error::Invalid(
            "unsupported manifest tools representation".into(),
        ));
    }
    let result = doc.to_string().into_bytes();
    if result.len() as u64 > journal::LIMIT
        || journal::encode(&parse(&result)?)? != journal::encode(&expected)?
    {
        return Err(Error::Invalid(
            "tool edit changed unrelated declarations or exceeded manifest limit".into(),
        ));
    }
    Ok(Some(result))
}
/// Read-only, requires existing registration. Does not create state or lock files.
pub fn preview(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    tool: Tool,
) -> Result<Preview> {
    prepare(host, repository, profiles, tool, None)
}
/// Edit an existing declaration without changing its ID.
pub fn preview_edit(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    tool: Tool,
) -> Result<Preview> {
    prepare(host, repository, profiles, tool, Some(Change::Edit))
}
/// Remove desired metadata only. References must remain valid.
pub fn preview_remove(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    id: &str,
) -> Result<Preview> {
    crate::context::validate_absolute(repository)?;
    observation::validate_output_path(repository)?;
    if !LocalConfig::load(host)?
        .repositories
        .values()
        .any(|r| r.root == repository)
    {
        return Err(Error::Invalid(
            "tool declarations require an already registered repository".into(),
        ));
    }
    let manifest = entry(&repository.join("dort.toml"))?;
    let tool = parse(&checked_bytes(&manifest)?)?
        .tools
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| Error::Invalid("tool declaration not found".into()))?;
    prepare(host, repository, profiles, tool, Some(Change::Remove))
}
fn prepare(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    tool: Tool,
    change: Option<Change>,
) -> Result<Preview> {
    crate::context::validate_absolute(repository)?;
    observation::validate_output_path(repository)?;
    let local = LocalConfig::load(host)?;
    let repository_id = local
        .repositories
        .iter()
        .find(|(_, r)| r.root == repository)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| {
            Error::Invalid("tool declarations require an already registered repository".into())
        })?;
    let manifest = entry(&repository.join("dort.toml"))?;
    let bytes = checked_bytes(&manifest)?;
    let before = change.and_then(|_| {
        parse(&bytes)
            .ok()?
            .tools
            .into_iter()
            .find(|t| t.id == tool.id)
    });
    if change == Some(Change::Remove)
        && before.as_ref().map(journal::encode).transpose()? != Some(journal::encode(&tool)?)
    {
        return Err(Error::Invalid(
            "declaration changed during preparation; review again".into(),
        ));
    }
    let no_op = edit(&bytes, &tool, host, profiles, repository, change)?.is_none();
    let local_config = observation::inspect(&host.local_config_path(), false)?;
    let mut p = Preview {
        schema: if change.is_some() { 2 } else { 1 },
        preview_only: true,
        repository: repository.to_owned(),
        repository_id,
        tool,
        change,
        before,
        profiles: profiles.to_vec(),
        no_op,
        effects: if no_op {
            vec![]
        } else {
            vec![manifest.path.clone()]
        },
        host: host.clone(),
        manifest,
        local_config,
        binding: String::new(),
    };
    p.binding = bind(&p)?;
    Ok(p)
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
fn stage_path(j: &Journal) -> PathBuf {
    j.directory.path.join("op-0000")
}
fn check_parents(j: &Journal, e: &Entry) -> Result<()> {
    if e.parents.iter().any(|p| {
        j.record
            .header
            .anchors
            .iter()
            .find(|a| a.path == p.path)
            .is_none_or(|a| !parent_identity(&a.fingerprint, &p.fingerprint))
    }) {
        return Err(Error::Invalid(
            "declaration parent identity/permissions changed".into(),
        ));
    }
    Ok(())
}
fn applied(j: &Journal) -> Result<bool> {
    let h = &j.record.header;
    let op = j
        .record
        .operation
        .as_ref()
        .ok_or_else(|| Error::Invalid("missing declaration operation".into()))?;
    let live = entry(&h.manifest.path)?;
    check_parents(j, &live)?;
    let stage = entry(&stage_path(j))?;
    let strict = op.progress == Progress::Pending;
    let before = same(&h.manifest, &live, strict) && same(&op.staged, &stage, strict);
    let after = same(&op.staged, &live, false) && same(&h.manifest, &stage, false);
    match op.progress {
        Progress::Pending if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if before => Ok(false),
        Progress::Intent | Progress::UndoIntent if after => Ok(true),
        Progress::Done if after && op.after.as_ref().is_some_and(|e| same(e, &live, true)) => {
            Ok(true)
        }
        Progress::Undone if before => Ok(false),
        _ => Err(Error::Invalid(
            "manifest/stage changed; declaration recovery refused".into(),
        )),
    }
}
fn validate_scope(host: &HostContext, j: &Journal) -> Result<()> {
    let h = &j.record.header;
    journal::registered_root(host, &h.repository_id, &h.repository)?;
    if h.local_config.path != host.local_config_path()
        || h.manifest.path != h.repository.join("dort.toml")
    {
        return Err(Error::Invalid("declaration scope mismatch".into()));
    }
    if j.record.operation.is_none() {
        return Ok(());
    }
    j.verify_backup()?;
    let bytes = journal::read_bytes(&j.directory, "manifest.backup")?;
    if edit(&bytes, &h.tool, host, &h.profiles, &h.repository, h.change)?
        .as_ref()
        .map(|b| journal::digest(b))
        .as_ref()
        != Some(&h.new_manifest_sha256)
    {
        return Err(Error::Invalid("declaration edit binding mismatch".into()));
    }
    Ok(())
}
fn validate_layout(host: &HostContext, j: &Journal) -> Result<()> {
    journal::registered_root(
        host,
        &j.record.header.repository_id,
        &j.record.header.repository,
    )?;
    j.verify_backup()?;
    let config = observation::inspect(&host.local_config_path(), false)?;
    if !same(&j.record.header.local_config, &config, true) {
        return Err(Error::Invalid(
            "local configuration changed since declaration approval".into(),
        ));
    }
    check_parents(j, &config)?;
    applied(j)?;
    Ok(())
}
fn mutate(j: &Journal, undo: bool) -> Result<()> {
    let path = &j.record.header.manifest.path;
    let handle = observation::parent_handle(path)?;
    let current = entry(path)?;
    check_parents(j, &current)?;
    if handle.missing
        || handle.parents.len() != current.parents.len()
        || handle
            .parents
            .iter()
            .zip(&current.parents)
            .any(|(a, b)| a.path != b.path || !parent_identity(&a.fingerprint, &b.fingerprint))
        || applied(j)? != undo
    {
        return Err(Error::Invalid(
            "declaration mutation precondition changed".into(),
        ));
    }
    fs::renameat_with(
        &j.directory.file,
        "op-0000",
        &handle.directory,
        &handle.name,
        RenameFlags::EXCHANGE,
    )
    .map_err(|e| {
        io_error(
            "exchange declaration manifest (no unsafe fallback)",
            path,
            e,
        )
    })?;
    handle
        .directory
        .sync_all()
        .map_err(|e| io_error("sync declaration parent", path, e))?;
    j.directory.sync()
}
pub fn execute(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Option<String>> {
    lock.check_context(host)?;
    journal::ensure_writable(host)?;
    let old = approval.0;
    if journal::encode(host)? != journal::encode(&old.host)? {
        return Err(Error::Invalid(
            "declaration approval belongs to another host".into(),
        ));
    }
    let fresh = prepare(
        host,
        &old.repository,
        &old.profiles,
        old.tool.clone(),
        old.change,
    )?;
    if old.binding != fresh.binding {
        return Err(Error::Invalid(
            "stale declaration preview; inspect and approve again".into(),
        ));
    }
    if fresh.no_op {
        return Ok(None);
    }
    let bytes = checked_bytes(&fresh.manifest)?;
    let edited = edit(
        &bytes,
        &fresh.tool,
        host,
        &fresh.profiles,
        &fresh.repository,
        fresh.change,
    )?
    .ok_or_else(|| Error::Invalid("declaration unexpectedly became no-op".into()))?;
    let h = Header {
        repository_id: fresh.repository_id,
        repository: fresh.repository,
        transaction_id: adoption::transaction_id(),
        approved_plan_sha256: old.binding,
        tool: fresh.tool,
        change: fresh.change,
        profiles: fresh.profiles,
        anchors: anchors(&[&fresh.manifest, &fresh.local_config]),
        manifest: fresh.manifest,
        local_config: fresh.local_config,
        new_manifest_sha256: journal::digest(&edited),
    };
    let mut j = Journal::begin(host, h)?;
    notify(&j, Boundary::Checkpoint, &mut observer)?;
    j.directory.create("manifest.backup", &bytes)?;
    j.verify_backup()?;
    let fp = j.record.header.manifest.fingerprint.as_ref().unwrap();
    let device = fs::fstat(&j.directory.file)
        .map_err(|e| io_error("inspect declaration filesystem", &j.directory.path, e))?
        .st_dev;
    if fp.device != device as u64
        || j.record.header.manifest.parents.last().is_none_or(|p| {
            p.fingerprint.device != fp.device
                || p.fingerprint.uid != rustix::process::geteuid().as_raw()
        })
    {
        return Err(Error::Invalid(
            "declaration requires owned parent and same filesystem; no cross-device fallback"
                .into(),
        ));
    }
    j.directory.create("op-0000", &edited)?;
    let file = File::from(
        fs::openat(
            &j.directory.file,
            "op-0000",
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open prepared declaration", &j.directory.path, e))?,
    );
    fs::fchown(&file, None, Some(rustix::process::Gid::from_raw(fp.gid)))
        .map_err(|e| io_error("preserve manifest group", &j.directory.path, e))?;
    fs::fchmod(&file, Mode::from_bits_truncate((fp.mode & 0o777) as _))
        .map_err(|e| io_error("preserve manifest permissions", &j.directory.path, e))?;
    file.sync_all()
        .map_err(|e| io_error("sync prepared declaration", &j.directory.path, e))?;
    j.directory.sync()?;
    j.record.operation = Some(Operation {
        staged: entry(&stage_path(&j))?,
        after: None,
        progress: Progress::Pending,
    });
    j.record.phase = Phase::Running;
    checkpoint(&mut j, &mut observer)?;
    validate_scope(host, &j)?;
    validate_layout(host, &j)?;
    j.record.operation.as_mut().unwrap().progress = Progress::Intent;
    checkpoint(&mut j, &mut observer)?;
    validate_layout(host, &j)?;
    mutate(&j, false)?;
    notify(&j, Boundary::Mutation, &mut observer)?;
    let after = entry(&j.record.header.manifest.path)?;
    if !same(&j.record.operation.as_ref().unwrap().staged, &after, false) {
        return Err(Error::Invalid("declaration postcondition mismatch".into()));
    }
    let op = j.record.operation.as_mut().unwrap();
    op.after = Some(after);
    op.progress = Progress::Done;
    checkpoint(&mut j, &mut observer)?;
    j.record.phase = Phase::Verifying;
    checkpoint(&mut j, &mut observer)?;
    validate_layout(host, &j)?;
    j.record.phase = Phase::Completed;
    checkpoint(&mut j, &mut observer)?;
    Ok(Some(j.record.header.transaction_id))
}
pub(crate) fn rollback_preview(
    host: &HostContext,
    repository: &str,
    tx: &str,
) -> Result<(String, Vec<PathBuf>)> {
    let j = Journal::open(host, repository, tx)?;
    validate_scope(host, &j)?;
    let effects = if matches!(j.record.phase, Phase::Preparing | Phase::RolledBack) {
        vec![]
    } else {
        validate_layout(host, &j)?;
        vec![j.record.header.manifest.path.clone()]
    };
    Ok((
        journal::digest(journal::encode(&j.record)?.as_bytes()),
        effects,
    ))
}
pub(crate) fn rollback(
    host: &HostContext,
    lock: &WriterLock,
    repository: &str,
    tx: &str,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    lock.check_context(host)?;
    let mut j = Journal::open(host, repository, tx)?;
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
    if j.record.operation.as_ref().unwrap().progress != Progress::Undone {
        let was_applied = applied(&j)?;
        j.record.operation.as_mut().unwrap().progress = Progress::UndoIntent;
        checkpoint(&mut j, &mut observer)?;
        validate_layout(host, &j)?;
        if was_applied {
            mutate(&j, true)?;
            notify(&j, Boundary::Mutation, &mut observer)?;
        }
        j.record.operation.as_mut().unwrap().progress = Progress::Undone;
        checkpoint(&mut j, &mut observer)?;
    }
    validate_layout(host, &j)?;
    j.record.phase = Phase::RolledBack;
    checkpoint(&mut j, &mut observer)
}
