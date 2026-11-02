//! Schema-3 staged local initialization. No network, automatic replay or repository deletion.
use crate::{
    Error, Manifest, Result,
    adoption::{self, Boundary, Event, parent_identity, same},
    context::HostContext,
    error::io_error,
    git::Git,
    journal::{self, Diagnosis, Phase},
    local,
    observation::{self, Entry, Kind},
    state::{PrivateDir, WriterLock},
};
use rustix::fs::{self, Mode, OFlags, RenameFlags};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    root: Entry,
    manifest: Option<Entry>,
    git_marker: Option<Entry>,
    config: Entry,
    state: PathBuf,
    config_sha256: Option<String>,
    registration_sha256: String,
    repository_id: String,
}
#[derive(Serialize)]
pub struct Preview {
    pub schema: u32,
    pub preview_only: bool,
    pub create: bool,
    pub repository: PathBuf,
    pub repository_id: String,
    pub effects: Vec<&'static str>,
    #[serde(skip)]
    inputs: Inputs,
    #[serde(skip)]
    binding: String,
}
pub struct Approval(Preview);
impl Approval {
    pub fn from_preview(preview: Preview) -> Result<Self> {
        if binding(&preview.inputs)? != preview.binding
            || preview.repository != preview.inputs.root.path
            || preview.create != (preview.inputs.root.kind == Kind::Missing)
            || preview.repository_id != preview.inputs.repository_id
            || preview.effects != effects(preview.create)
            || preview.schema != 1
            || !preview.preview_only
        {
            return Err(invalid("initialization preview was modified"));
        }
        Ok(Self(preview))
    }
}
fn effects(create: bool) -> Vec<&'static str> {
    if create {
        vec![
            "create private staged Git repository and minimal manifest",
            "publish repository without replacement",
            "register and select repository",
        ]
    } else {
        vec!["register and select existing repository"]
    }
}
fn binding(inputs: &Inputs) -> Result<String> {
    let mut i = inputs.clone();
    for e in [&mut i.root, &mut i.config]
        .into_iter()
        .chain(i.manifest.iter_mut())
        .chain(i.git_marker.iter_mut())
    {
        for p in &mut e.parents {
            p.fingerprint.size = 0;
            p.fingerprint.links = 0;
            p.fingerprint.modified = (0, 0);
            p.fingerprint.changed = (0, 0);
        }
        if e.kind == Kind::Directory
            && let Some(f) = &mut e.fingerprint
        {
            f.size = 0;
            f.links = 0;
            f.modified = (0, 0);
            f.changed = (0, 0);
        }
    }
    Ok(journal::digest(journal::encode(&i)?.as_bytes()))
}
fn config_entry(host: &HostContext) -> Result<(Entry, Option<Vec<u8>>)> {
    let mut entry = observation::inspect(&host.local_config_path(), false)?;
    let bytes = if entry.kind == Kind::Missing {
        None
    } else {
        let dir = PrivateDir::open(&host.config_dir, false)?
            .ok_or_else(|| invalid("missing configuration directory"))?;
        Some(journal::read_bytes(&dir, "local.toml")?)
    };
    entry.sha256 = bytes.as_deref().map(journal::digest);
    Ok((entry, bytes))
}
fn root_scope(host: &HostContext, root: &Path) -> Result<()> {
    observation::validate_output_path(root)?;
    if root == host.home
        || root.parent().is_none()
        || root.starts_with(&host.config_dir)
        || host.config_dir.starts_with(root)
        || root.starts_with(&host.state_dir)
        || host.state_dir.starts_with(root)
    {
        return Err(invalid(
            "repository cannot be HOME/root or overlap Dort metadata",
        ));
    }
    Ok(())
}
/// Destination must be absolute/physical, absent with an existing owned safe parent,
/// or an already-valid non-bare repository. No directories, lock or state are created.
pub fn preview(host: &HostContext, root: &Path, git: &Path) -> Result<Preview> {
    Git::new(git.to_owned())?;
    root_scope(host, root)?;
    let root_entry = observation::inspect(root, false)?;
    let parent = observation::parent_handle(root)?;
    if parent.missing {
        return Err(invalid("initialization requires an existing parent"));
    }
    let stat =
        fs::fstat(&parent.directory).map_err(|e| io_error("inspect repository parent", root, e))?;
    if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o022 != 0 {
        return Err(invalid(
            "repository parent must be owned and not group/world writable",
        ));
    }
    if root_entry
        .fingerprint
        .as_ref()
        .is_some_and(|f| f.uid != rustix::process::geteuid().as_raw() || f.mode & 0o022 != 0)
    {
        return Err(invalid(
            "repository root must be owned and not group/world writable",
        ));
    }
    let create = root_entry.kind == Kind::Missing;
    if !create && root_entry.kind != Kind::Directory {
        return Err(invalid(
            "repository destination must be absent or a valid repository directory",
        ));
    }
    let (manifest, git_marker) = if create {
        Manifest::parse("schema = 1\n")?.validate_for_host(
            host,
            &local::LocalConfig::load(host)?.active_profiles,
            Some(root),
        )?;
        (None, None)
    } else {
        Git::new(git.to_owned())?.status(root)?;
        let (manifest, entry) = observation::read_manifest(&root.join("dort.toml"))?;
        manifest.validate_for_host(
            host,
            &local::LocalConfig::load(host)?.active_profiles,
            Some(root),
        )?;
        let marker = observation::inspect(&root.join(".git"), true)?;
        if !matches!(marker.kind, Kind::Directory | Kind::Regular) {
            return Err(invalid(
                "attachment requires a regular Git marker or metadata directory",
            ));
        }
        (Some(entry), Some(marker))
    };
    let (config, bytes) = config_entry(host)?;
    let (id, text) = local::registration_text(host, root)?;
    let inputs = Inputs {
        root: root_entry,
        manifest,
        git_marker,
        config,
        state: host.state_dir.clone(),
        config_sha256: bytes.as_deref().map(journal::digest),
        registration_sha256: journal::digest(text.as_bytes()),
        repository_id: id.clone(),
    };
    let binding = binding(&inputs)?;
    Ok(Preview {
        schema: 1,
        preview_only: true,
        create,
        repository: root.into(),
        repository_id: id,
        effects: effects(create),
        inputs,
        binding,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Step {
    Preparing,
    PublishIntent,
    Published,
    RegisterIntent,
    Completed,
    Abandoned,
}
impl Step {
    fn phase(self) -> Phase {
        match self {
            Self::Preparing => Phase::Preparing,
            Self::Completed => Phase::Completed,
            Self::Abandoned => Phase::RolledBack,
            _ => Phase::Running,
        }
    }
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Abandoned)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    tree: Vec<Entry>,
    config: Entry,
    destination: Entry,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    schema: u32,
    sequence: usize,
    previous_sha256: Option<String>,
    transaction_id: String,
    inputs: Inputs,
    step: Step,
    prepared: Option<Prepared>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    payload: String,
}
struct Journal {
    directory: PrivateDir,
    record: Record,
    hash: Option<String>,
    next: usize,
}
impl Journal {
    fn check_directory(&self) -> Result<()> {
        let named = observation::inspect(&self.directory.path, false)?;
        let retained =
            observation::Fingerprint::from_stat(&fs::fstat(&self.directory.file).map_err(|e| {
                io_error(
                    "inspect retained bootstrap directory",
                    &self.directory.path,
                    e,
                )
            })?);
        if named.kind != Kind::Directory
            || retained.uid != rustix::process::geteuid().as_raw()
            || retained.mode & 0o077 != 0
            || named
                .fingerprint
                .as_ref()
                .is_none_or(|f| !parent_identity(f, &retained))
        {
            return Err(invalid("bootstrap transaction directory substituted"));
        }
        if let Some(p) = &self.record.prepared
            && !parents_match(&p.config, &observation::inspect(&p.config.path, false)?)
        {
            return Err(invalid("bootstrap private parent identities changed"));
        }
        Ok(())
    }
    fn save(&mut self, observer: &mut impl FnMut(&Event) -> Result<()>) -> Result<()> {
        self.check_directory()?;
        self.record.sequence = self.next;
        self.record.previous_sha256 = self.hash.clone();
        let payload = journal::encode(&self.record)?;
        let sha256 = journal::digest(payload.as_bytes());
        let bytes = journal::encode(&Envelope {
            sha256: sha256.clone(),
            payload,
        })?;
        if bytes.len() as u64 > journal::LIMIT {
            return Err(invalid("bootstrap checkpoint too large"));
        }
        self.directory
            .create(&format!("{:06}.json", self.next), bytes.as_bytes())?;
        self.hash = Some(sha256);
        self.next += 1;
        self.notify(Boundary::Checkpoint, observer)
    }
    fn notify(
        &self,
        boundary: Boundary,
        observer: &mut impl FnMut(&Event) -> Result<()>,
    ) -> Result<()> {
        observer(&Event {
            transaction_id: self.record.transaction_id.clone(),
            boundary,
            sequence: self.record.sequence,
        })
    }
    fn open(host: &HostContext, rid: &str, tid: &str) -> Result<Self> {
        if !journal::valid_id(tid) {
            return Err(invalid("invalid bootstrap transaction ID"));
        }
        let directory = PrivateDir::open(&journal::transactions(host, rid)?.join(tid), false)?
            .ok_or_else(|| invalid("unknown bootstrap transaction"))?;
        let (d, record, hash) = scan(&directory, rid, tid);
        if d.problem.is_some() {
            return Err(invalid(
                "malformed bootstrap journal; manual diagnosis required",
            ));
        }
        let record = record.ok_or_else(|| invalid("missing bootstrap intent"))?;
        validate_scope(host, &record)?;
        Ok(Self {
            directory,
            record,
            hash,
            next: d.valid_checkpoints,
        })
    }
}
fn validate_scope(host: &HostContext, r: &Record) -> Result<()> {
    root_scope(host, &r.inputs.root.path)?;
    if r.inputs.config.path != host.local_config_path() || r.inputs.state != host.state_dir {
        return Err(invalid("bootstrap host scope mismatch"));
    }
    Ok(())
}
pub(crate) fn is_schema3(d: &PrivateDir) -> bool {
    journal::read_bytes(d, "000000.json")
        .ok()
        .and_then(|b| serde_json::from_slice::<Envelope>(&b).ok())
        .and_then(|e| serde_json::from_str::<serde_json::Value>(&e.payload).ok())
        .is_some_and(|v| v.get("schema").and_then(|n| n.as_u64()) == Some(3))
}
pub(crate) fn scan(
    d: &PrivateDir,
    rid: &str,
    tid: &str,
) -> (Diagnosis, Option<Record>, Option<String>) {
    let mut report = Diagnosis {
        repository_id: rid.into(),
        transaction_id: tid.into(),
        phase: None,
        journal_schema: None,
        valid_checkpoints: 0,
        problem: None,
    };
    let mut latest: Option<Record> = None;
    let mut hash = None;
    let result: Result<()> = (|| {
        let names = observation::names(&d.path, 4096)?;
        let mut cps = vec![];
        for n in &names {
            let n = n
                .to_str()
                .ok_or_else(|| invalid("invalid bootstrap artifact name"))?;
            if n.ends_with(".json") {
                cps.push(n);
            } else if !matches!(n, "repository" | "config.new" | "config.backup") {
                return Err(invalid("unknown bootstrap artifact"));
            }
        }
        if cps.is_empty() {
            return Err(invalid("missing bootstrap intent"));
        }
        for (i, name) in cps.into_iter().enumerate() {
            if name != format!("{i:06}.json") {
                return Err(invalid("bootstrap checkpoint sequence gap"));
            }
            let env: Envelope = serde_json::from_slice(&journal::read_bytes(d, name)?)
                .map_err(|_| invalid("malformed bootstrap checkpoint"))?;
            if journal::digest(env.payload.as_bytes()) != env.sha256 {
                return Err(invalid("bootstrap checksum mismatch"));
            }
            let r: Record = serde_json::from_str(&env.payload)
                .map_err(|_| invalid("unsupported bootstrap checkpoint"))?;
            if r.schema != 3
                || r.sequence != i
                || r.previous_sha256 != hash
                || r.transaction_id != tid
                || r.inputs.repository_id != rid
                || !journal::valid_id(rid)
            {
                return Err(invalid("bootstrap record binding mismatch"));
            }
            observation::validate_output_path(&r.inputs.root.path)?;
            observation::validate_output_path(&r.inputs.config.path)?;
            observation::validate_output_path(&r.inputs.state)?;
            if !journal::valid_id(&r.inputs.registration_sha256)
                || !matches!(r.inputs.config.kind, Kind::Missing | Kind::Regular)
                || r.inputs.config_sha256.is_some() != (r.inputs.config.kind == Kind::Regular)
                || r.inputs.config.sha256 != r.inputs.config_sha256
            {
                return Err(invalid("invalid bootstrap configuration binding"));
            }
            if let Some(m) = &r.inputs.manifest
                && (m.path != r.inputs.root.path.join("dort.toml")
                    || m.kind != Kind::Regular
                    || m.sha256.is_none())
            {
                return Err(invalid("bootstrap manifest scope mismatch"));
            }
            if let Some(g) = &r.inputs.git_marker
                && (g.path != r.inputs.root.path.join(".git")
                    || !matches!(g.kind, Kind::Directory | Kind::Regular))
            {
                return Err(invalid("bootstrap Git marker scope mismatch"));
            }
            if !matches!(r.inputs.root.kind, Kind::Missing | Kind::Directory)
                || r.inputs.manifest.is_some() != (r.inputs.root.kind == Kind::Directory)
                || r.inputs.git_marker.is_some() != r.inputs.manifest.is_some()
            {
                return Err(invalid("bootstrap input scope mismatch"));
            }
            if let Some(p) = &r.prepared {
                if p.destination.path != r.inputs.config.path
                    || p.config.path != d.path.join("config.new")
                    || p.config.sha256.as_ref() != Some(&r.inputs.registration_sha256)
                    || p.config.kind != Kind::Regular
                    || !same(&r.inputs.config, &p.destination, true)
                    || p.tree.len() > 256
                {
                    return Err(invalid("bootstrap prepared scope mismatch"));
                }
                let mut paths = std::collections::BTreeSet::new();
                for e in &p.tree {
                    observation::validate_output_path(&e.path)?;
                    if e.fingerprint.as_ref().is_none_or(|f| {
                        f.uid != rustix::process::geteuid().as_raw()
                            || f.mode & 0o7022 != 0
                            || (e.kind == Kind::Regular && (f.links != 1 || e.sha256.is_none()))
                    }) {
                        return Err(invalid("unsafe bootstrap tree metadata"));
                    }
                    if !e.path.starts_with(d.path.join("repository"))
                        || !matches!(e.kind, Kind::Directory | Kind::Regular)
                        || !paths.insert(&e.path)
                    {
                        return Err(invalid("bootstrap tree scope mismatch"));
                    }
                }
                if !p.tree.is_empty()
                    && (!paths.contains(&d.path.join("repository"))
                        || p.tree.iter().any(|e| {
                            e.path != d.path.join("repository")
                                && e.path
                                    .parent()
                                    .is_none_or(|parent| !paths.contains(&parent.to_owned()))
                        }))
                {
                    return Err(invalid("bootstrap tree has missing directory anchors"));
                }
                if (r.inputs.root.kind == Kind::Missing) != !p.tree.is_empty() {
                    return Err(invalid("bootstrap tree binding mismatch"));
                }
            }
            if let Some(prev) = &latest {
                if journal::encode(&prev.inputs)? != journal::encode(&r.inputs)?
                    || prev.step.terminal()
                    || (prev.step == Step::Preparing
                        && r.step == Step::Abandoned
                        && r.prepared.is_some())
                    || (prev.prepared.is_some()
                        && journal::encode(&prev.prepared)? != journal::encode(&r.prepared)?)
                {
                    return Err(invalid("bootstrap immutable identity changed"));
                }
                let valid = r.step == Step::Abandoned
                    || matches!(
                        (prev.step, r.step),
                        (Step::Preparing, Step::PublishIntent)
                            | (Step::PublishIntent, Step::Published)
                            | (Step::Published, Step::RegisterIntent)
                            | (Step::RegisterIntent, Step::Completed)
                    );
                if !valid {
                    return Err(invalid("invalid bootstrap transition"));
                }
            } else if r.step != Step::Preparing || r.prepared.is_some() {
                return Err(invalid("bootstrap must begin preparing"));
            }
            if !matches!(r.step, Step::Preparing | Step::Abandoned) && r.prepared.is_none() {
                return Err(invalid("missing prepared bootstrap guards"));
            }
            report.phase = Some(r.step.phase());
            report.journal_schema = Some(3);
            report.valid_checkpoints += 1;
            hash = Some(env.sha256);
            latest = Some(r);
        }
        if let Some(r) = &latest
            && r.prepared.is_some()
            && let Some(expected) = &r.inputs.config_sha256
            && journal::digest(&journal::read_bytes(d, "config.backup")?) != *expected
        {
            return Err(invalid("bootstrap configuration backup mismatch"));
        }
        Ok(())
    })();
    if result.is_err() {
        report.problem =
            Some("invalid bootstrap chain/artifacts; explicit manual diagnosis required".into());
    }
    (report, latest, hash)
}

/// Sync and capture a small newly initialized tree. No symlinks/special/hardlinked files.
fn tree(root: &Path, sync: bool) -> Result<Vec<Entry>> {
    fn walk(path: &Path, out: &mut Vec<Entry>, sync: bool) -> Result<()> {
        if out.len() >= 256 {
            return Err(invalid("bootstrap tree exceeds entry limit"));
        }
        let mut e = observation::inspect(path, true)?;
        let fp = e
            .fingerprint
            .as_ref()
            .ok_or_else(|| invalid("missing bootstrap tree entry"))?;
        if fp.uid != rustix::process::geteuid().as_raw()
            || fp.mode & 0o7022 != 0
            || !matches!(e.kind, Kind::Regular | Kind::Directory)
            || (e.kind == Kind::Regular && (fp.links != 1 || e.sha256.is_none()))
        {
            return Err(invalid("unsafe bootstrap tree entry"));
        }
        if e.kind == Kind::Directory {
            for n in observation::names(path, 256)? {
                walk(&path.join(n), out, sync)?;
            }
        }
        if sync {
            let h = observation::parent_handle(path)?;
            let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
            let f = File::from(
                fs::openat(&h.directory, &h.name, flags, Mode::empty())
                    .map_err(|e| io_error("open bootstrap tree", path, e))?,
            );
            f.sync_all()
                .map_err(|e| io_error("sync bootstrap tree", path, e))?;
            let current = observation::inspect(path, true)?;
            if !same(&e, &current, true) {
                return Err(invalid("bootstrap tree raced during sync"));
            }
            e = current;
        }
        out.push(e);
        Ok(())
    }
    let mut entries = vec![];
    walk(root, &mut entries, sync)?;
    Ok(entries)
}
fn parents_match(before: &Entry, current: &Entry) -> bool {
    before.parents.len() == current.parents.len()
        && before
            .parents
            .iter()
            .zip(&current.parents)
            .all(|(a, b)| a.path == b.path && parent_identity(&a.fingerprint, &b.fingerprint))
}
fn tree_matches(j: &Journal, root: &Path) -> Result<()> {
    let expected = &j
        .record
        .prepared
        .as_ref()
        .ok_or_else(|| invalid("missing bootstrap preparation"))?
        .tree;
    let actual = tree(root, false)?;
    if expected.len() != actual.len() {
        return Err(invalid("bootstrap repository has new/missing entries"));
    }
    for (a, b) in expected.iter().zip(&actual) {
        let relative = a
            .path
            .strip_prefix(j.directory.path.join("repository"))
            .map_err(|_| invalid("invalid bootstrap tree path"))?;
        if b.path != root.join(relative) || !same(a, b, false) {
            return Err(invalid("bootstrap repository edited or substituted"));
        }
    }
    Ok(())
}
// Return true only when the exact prepared repository is already published.
fn repository_state(j: &Journal) -> Result<bool> {
    j.check_directory()?;
    let i = &j.record.inputs;
    let current = observation::inspect(&i.root.path, false)?;
    if !parents_match(&i.root, &current) {
        return Err(invalid("bootstrap repository parents changed"));
    }
    if i.root.kind == Kind::Directory {
        if !same(&i.root, &current, false) {
            return Err(invalid("attached repository identity changed"));
        }
        for e in [i.manifest.as_ref().unwrap(), i.git_marker.as_ref().unwrap()] {
            let now = observation::inspect(&e.path, e.kind == Kind::Regular)?;
            if !same(e, &now, true) || !parents_match(e, &now) {
                return Err(invalid("attached repository manifest/Git marker changed"));
            }
        }
        return Ok(true);
    }
    let stage = j.directory.path.join("repository");
    if current.kind == Kind::Missing {
        if j.record.prepared.is_some() {
            tree_matches(j, &stage)?;
        }
        return Ok(false);
    }
    if j.record.prepared.is_none() || observation::inspect(&stage, false)?.kind != Kind::Missing {
        return Err(invalid("unrecognized bootstrap publication"));
    }
    tree_matches(j, &i.root.path)?;
    Ok(true)
}
// Exact before/after states only; recognize interrupted config exchange by identity/hash.
fn config_state(host: &HostContext, j: &Journal) -> Result<bool> {
    j.check_directory()?;
    let i = &j.record.inputs;
    let (now, bytes) = config_entry(host)?;
    let sha = bytes.as_deref().map(journal::digest);
    if let Some(p) = &j.record.prepared
        && !parents_match(&p.destination, &now)
    {
        return Err(invalid("registration parent identity changed"));
    }
    if same(&i.config, &now, true) && sha == i.config_sha256 {
        // Missing ancestors may have been created privately for registration.
        if !i.config.parents.iter().all(|a| {
            now.parents
                .iter()
                .any(|b| a.path == b.path && parent_identity(&a.fingerprint, &b.fingerprint))
        }) {
            return Err(invalid("bootstrap configuration parents changed"));
        }
        if let Some(p) = &j.record.prepared {
            let mut stage = observation::inspect(&p.config.path, false)?;
            stage.sha256 = Some(journal::digest(&journal::read_bytes(
                &j.directory,
                "config.new",
            )?));
            if !same(&p.config, &stage, true) {
                return Err(invalid("prepared registration changed"));
            }
        }
        return Ok(false);
    }
    if let Some(p) = &j.record.prepared
        && same(&p.config, &now, false)
        && sha.as_ref() == Some(&i.registration_sha256)
    {
        if i.config.kind != Kind::Missing {
            let mut old = observation::inspect(&p.config.path, false)?;
            old.sha256 = Some(journal::digest(&journal::read_bytes(
                &j.directory,
                "config.new",
            )?));
            if !same(&i.config, &old, false)
                || journal::digest(&journal::read_bytes(&j.directory, "config.new")?)
                    != i.config_sha256.clone().unwrap_or_default()
            {
                return Err(invalid("retained original configuration changed"));
            }
        } else if observation::inspect(&p.config.path, false)?.kind != Kind::Missing {
            return Err(invalid("unexpected retained registration"));
        }
        return Ok(true);
    }
    Err(invalid("configuration changed; bootstrap recovery refused"))
}
fn drive(
    host: &HostContext,
    j: &mut Journal,
    observer: &mut impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    if j.record.step == Step::PublishIntent {
        if config_state(host, j)? {
            return Err(invalid("registration published before durable intent"));
        }
        if !repository_state(j)? {
            let h = observation::parent_handle(&j.record.inputs.root.path)?;
            fs::renameat_with(
                &j.directory.file,
                "repository",
                &h.directory,
                &h.name,
                RenameFlags::NOREPLACE,
            )
            .map_err(|e| {
                io_error(
                    "publish repository without replacement",
                    &j.record.inputs.root.path,
                    e,
                )
            })?;
            h.directory
                .sync_all()
                .map_err(|e| io_error("sync repository parent", &j.record.inputs.root.path, e))?;
            j.directory.sync()?;
            j.notify(Boundary::Mutation, observer)?;
        }
        if !repository_state(j)? {
            return Err(invalid("repository publication not verified"));
        }
        j.record.step = Step::Published;
        j.save(observer)?;
    }
    if j.record.step == Step::Published {
        if !repository_state(j)? || config_state(host, j)? {
            return Err(invalid("unexpected published bootstrap layout"));
        }
        j.record.step = Step::RegisterIntent;
        j.save(observer)?;
    }
    if j.record.step == Step::RegisterIntent {
        if !repository_state(j)? {
            return Err(invalid("published repository is no longer present"));
        }
        let config = PrivateDir::open(&host.config_dir, false)?
            .ok_or_else(|| invalid("prepared configuration directory is missing"))?;
        if !config_state(host, j)? {
            let flags = if j.record.inputs.config.kind == Kind::Missing {
                RenameFlags::NOREPLACE
            } else {
                RenameFlags::EXCHANGE
            };
            fs::renameat_with(
                &j.directory.file,
                "config.new",
                &config.file,
                "local.toml",
                flags,
            )
            .map_err(|e| {
                io_error(
                    "publish repository registration",
                    host.local_config_path(),
                    e,
                )
            })?;
            config.sync()?;
            j.directory.sync()?;
            j.notify(Boundary::Mutation, observer)?;
        }
        if !config_state(host, j)? || !repository_state(j)? {
            return Err(invalid("bootstrap completion not verified"));
        }
        j.record.step = Step::Completed;
        j.save(observer)?;
    }
    Ok(())
}

pub fn execute(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    git: &Path,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<String> {
    lock.check_context(host)?;
    journal::ensure_writable(host)?;
    let approved = approval.0;
    let fresh = preview(host, &approved.repository, git)?;
    if fresh.binding != approved.binding {
        return Err(invalid("initialization approval is stale"));
    }
    let tid = adoption::transaction_id();
    let txroot = journal::transactions(host, &fresh.repository_id)?;
    let parent = PrivateDir::open(&txroot, true)?.expect("created transaction root");
    fs::mkdirat(
        &parent.file,
        tid.as_str(),
        Mode::RUSR | Mode::WUSR | Mode::XUSR,
    )
    .map_err(|e| io_error("allocate bootstrap transaction", &txroot, e))?;
    parent.sync()?;
    let directory = PrivateDir::open(&txroot.join(&tid), false)?.expect("allocated bootstrap");
    let mut j = Journal {
        directory,
        record: Record {
            schema: 3,
            sequence: 0,
            previous_sha256: None,
            transaction_id: tid.clone(),
            inputs: fresh.inputs,
            step: Step::Preparing,
            prepared: None,
        },
        hash: None,
        next: 0,
    };
    j.save(&mut observer)?;
    let entries = if fresh.create {
        fs::mkdirat(
            &j.directory.file,
            "repository",
            Mode::RUSR | Mode::WUSR | Mode::XUSR,
        )
        .map_err(|e| io_error("create private repository stage", &j.directory.path, e))?;
        j.directory.sync()?;
        let stage = j.directory.path.join("repository");
        Git::initialize_private(git.to_owned(), &stage)?;
        let dir = PrivateDir::open(&stage, false)?.expect("private repository");
        dir.create("dort.toml", b"schema = 1\n")?;
        Manifest::parse("schema = 1\n")?.validate_for_host(host, &[], Some(&fresh.repository))?;
        tree(&stage, true)?
    } else {
        vec![]
    };
    let (_, original) = config_entry(host)?;
    if original.as_deref().map(journal::digest) != j.record.inputs.config_sha256 {
        return Err(invalid("configuration changed during preparation"));
    }
    if let Some(bytes) = original {
        j.directory.create("config.backup", &bytes)?;
    }
    let (_, text) = local::registration_text(host, &fresh.repository)?;
    if journal::digest(text.as_bytes()) != j.record.inputs.registration_sha256 {
        return Err(invalid("registration changed during preparation"));
    }
    j.directory.create("config.new", text.as_bytes())?;
    let mut config = observation::inspect(&j.directory.path.join("config.new"), true)?;
    config.sha256 = Some(journal::digest(text.as_bytes()));
    PrivateDir::open(&host.config_dir, true)?.expect("created private configuration directory");
    let destination = config_entry(host)?.0;
    j.record.prepared = Some(Prepared {
        tree: entries,
        config,
        destination,
    });
    repository_state(&j)?;
    config_state(host, &j)?;
    // Refuse cross-device before any publication (including config on a different FS).
    let state_device = observation::Fingerprint::from_stat(
        &fs::fstat(&j.directory.file)
            .map_err(|e| io_error("inspect bootstrap device", &j.directory.path, e))?,
    )
    .device;
    for path in [&fresh.repository, &host.local_config_path()] {
        let entry = observation::inspect(path, false)?;
        if entry
            .parents
            .last()
            .is_none_or(|p| p.fingerprint.device != state_device)
        {
            return Err(invalid(
                "bootstrap requires repository/configuration/state on one filesystem",
            ));
        }
    }
    j.record.step = Step::PublishIntent;
    j.save(&mut observer)?;
    drive(host, &mut j, &mut observer)?;
    Ok(tid)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Resume,
    Abandon,
}
#[derive(Serialize)]
pub struct RecoveryPreview {
    pub schema: u32,
    pub preview_only: bool,
    pub repository_id: String,
    pub transaction_id: String,
    pub repository: PathBuf,
    pub action: RecoveryAction,
    pub repository_published: bool,
    pub registration_published: bool,
    pub effects: Vec<&'static str>,
    pub journal_head_sha256: String,
    #[serde(skip)]
    binding: String,
}
pub struct RecoveryApproval(RecoveryPreview);
impl RecoveryApproval {
    pub fn from_preview(p: RecoveryPreview) -> Result<Self> {
        if journal::digest(journal::encode(&p)?.as_bytes()) != p.binding {
            return Err(invalid("bootstrap recovery preview modified"));
        }
        Ok(Self(p))
    }
}
pub fn recovery_preview(
    host: &HostContext,
    rid: &str,
    tid: &str,
    action: RecoveryAction,
) -> Result<RecoveryPreview> {
    let j = Journal::open(host, rid, tid)?;
    if j.record.step.terminal() {
        return Err(invalid("bootstrap is already terminal"));
    }
    if action == RecoveryAction::Resume && j.record.step == Step::Preparing {
        return Err(invalid(
            "incomplete private preparation; explicitly abandon and retry with a new transaction",
        ));
    }
    let repository_published = repository_state(&j)?;
    let registration_published = config_state(host, &j)?;
    if matches!(j.record.step, Step::Published | Step::RegisterIntent) && !repository_published {
        return Err(invalid("published repository is no longer present"));
    }
    if matches!(
        j.record.step,
        Step::Preparing | Step::PublishIntent | Step::Published
    ) && registration_published
    {
        return Err(invalid("registration published before durable intent"));
    }
    let effects = if action == RecoveryAction::Abandon {
        vec!["mark abandoned; retain repository, registration and private artifacts unchanged"]
    } else {
        vec![
            "publish exact prepared repository if absent",
            "publish exact prepared registration if absent",
            "mark completed",
        ]
    };
    let mut p = RecoveryPreview {
        schema: 1,
        preview_only: true,
        repository_id: rid.into(),
        transaction_id: tid.into(),
        repository: j.record.inputs.root.path.clone(),
        action,
        repository_published,
        registration_published,
        effects,
        journal_head_sha256: j.hash.unwrap_or_default(),
        binding: String::new(),
    };
    p.binding = journal::digest(journal::encode(&p)?.as_bytes());
    Ok(p)
}
pub fn recover(
    host: &HostContext,
    lock: &WriterLock,
    approval: RecoveryApproval,
    mut observer: impl FnMut(&Event) -> Result<()>,
) -> Result<()> {
    lock.check_context(host)?;
    let p = approval.0;
    if recovery_preview(host, &p.repository_id, &p.transaction_id, p.action)?.binding != p.binding {
        return Err(invalid("bootstrap recovery approval stale"));
    }
    let mut j = Journal::open(host, &p.repository_id, &p.transaction_id)?;
    if p.action == RecoveryAction::Abandon {
        j.record.step = Step::Abandoned;
        j.save(&mut observer)
    } else {
        drive(host, &mut j, &mut observer)
    }
}
