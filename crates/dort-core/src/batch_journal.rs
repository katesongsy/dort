//! Schema 4: explicitly selected homogeneous batches; older formats are unchanged.
pub(crate) use crate::batch::Action;
use crate::{
    Error, Result,
    context::HostContext,
    error::io_error,
    journal::{self, Diagnosis, Phase, Progress},
    model::ManagedFile,
    observation::{self, Entry, Kind, Parent},
    state::PrivateDir,
};
use rustix::fs::{self, Mode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    Directory,
    Source,
    Destination,
    Manifest,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mutation {
    Publish,
    Exchange,
    Withdraw,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Watch {
    pub mapping: ManagedFile,
    pub source: Entry,
    pub destination: Entry,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Backup {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub repository_id: String,
    pub repository: PathBuf,
    pub transaction_id: String,
    pub approved_plan_sha256: String,
    pub action: Action,
    pub acknowledged_ids: Vec<String>,
    pub new_manifest_sha256: Option<String>,
    pub profiles: Vec<String>,
    pub watches: Vec<Watch>,
    pub manifest: Entry,
    pub local_config: Entry,
    pub anchors: Vec<Parent>,
    pub backups: Vec<Backup>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub id: usize,
    pub role: Role,
    pub mutation: Mutation,
    pub before: Entry,
    pub staged: Entry,
    pub after: Option<Entry>,
    pub progress: Progress,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub schema: u32,
    pub sequence: usize,
    pub previous_sha256: Option<String>,
    pub header: Header,
    pub phase: Phase,
    pub operations: Vec<Operation>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    payload: String,
}

pub(crate) fn changed(h: &Header, w: &Watch) -> bool {
    h.action != Action::Apply
        || w.destination.kind != Kind::Symlink
        || observation::link_path(&w.destination).as_ref() != Some(&w.source.path)
}
pub(crate) fn backup_specs(h: &Header) -> Result<Vec<Backup>> {
    let mut entries = vec![];
    if h.action != Action::Apply {
        entries.push(&h.manifest);
    }
    entries.extend(
        h.watches
            .iter()
            .filter(|w| changed(h, w))
            .map(|w| &w.destination)
            .filter(|e| e.kind != Kind::Missing),
    );
    entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let hash = if e.kind == Kind::Symlink {
                journal::digest(
                    e.link_target
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .ok_or_else(|| Error::Invalid("missing backup link text".into()))?
                        .as_bytes(),
                )
            } else {
                e.sha256
                    .clone()
                    .ok_or_else(|| Error::Invalid("missing backup hash".into()))?
            };
            Ok(Backup {
                name: format!("backup-{i:04}"),
                path: e.path.clone(),
                sha256: hash,
            })
        })
        .collect()
}

// Pure expected live scope/order; journal strings cannot introduce arbitrary operations.
pub(crate) fn expected(h: &Header) -> Result<Vec<(Role, Mutation, PathBuf)>> {
    let mut result = vec![];
    if matches!(h.action, Action::Apply | Action::Adopt) {
        let mut dirs = std::collections::BTreeSet::new();
        for w in h.watches.iter().filter(|w| changed(h, w)) {
            let entry = if h.action == Action::Adopt {
                &w.source
            } else {
                &w.destination
            };
            let last = entry
                .parents
                .last()
                .ok_or_else(|| Error::Invalid("missing batch anchor".into()))?;
            let mut parent = entry
                .path
                .parent()
                .ok_or_else(|| Error::Invalid("invalid batch path".into()))?;
            while parent != last.path {
                if !parent.starts_with(&last.path) {
                    return Err(Error::Invalid("invalid batch parent scope".into()));
                }
                dirs.insert(parent.to_owned());
                parent = parent
                    .parent()
                    .ok_or_else(|| Error::Invalid("invalid batch parents".into()))?;
            }
        }
        let mut dirs: Vec<_> = dirs.into_iter().collect();
        dirs.sort_by(|a, b| {
            a.components()
                .count()
                .cmp(&b.components().count())
                .then(a.cmp(b))
        });
        result.extend(
            dirs.into_iter()
                .map(|p| (Role::Directory, Mutation::Publish, p)),
        );
    }
    if h.action == Action::Adopt {
        result.extend(
            h.watches
                .iter()
                .map(|w| (Role::Source, Mutation::Publish, w.source.path.clone())),
        );
        result.push((Role::Manifest, Mutation::Exchange, h.manifest.path.clone()));
    }
    for w in h.watches.iter().filter(|w| changed(h, w)) {
        result.push((
            Role::Destination,
            match h.action {
                Action::Remove => Mutation::Withdraw,
                _ if w.destination.kind == Kind::Missing => Mutation::Publish,
                _ => Mutation::Exchange,
            },
            w.destination.path.clone(),
        ));
    }
    if matches!(h.action, Action::Keep | Action::Remove) {
        result.push((Role::Manifest, Mutation::Exchange, h.manifest.path.clone()));
    }
    if result.is_empty() || result.len() > 256 {
        return Err(Error::Invalid(
            "empty or oversized batch transaction".into(),
        ));
    }
    Ok(result)
}

pub(crate) struct Journal {
    pub directory: PrivateDir,
    pub record: Record,
    hash: Option<String>,
    next: usize,
}
impl Journal {
    pub fn begin(host: &HostContext, header: Header) -> Result<Self> {
        let root = journal::transactions(host, &header.repository_id)?;
        let parent = PrivateDir::open(&root, true)?.unwrap();
        fs::mkdirat(
            &parent.file,
            header.transaction_id.as_str(),
            Mode::from_raw_mode(0o700),
        )
        .map_err(|e| io_error("allocate batch transaction", &root, e))?;
        parent.sync()?;
        let directory = PrivateDir::open(&root.join(&header.transaction_id), false)?.unwrap();
        let mut j = Self {
            directory,
            record: Record {
                schema: 4,
                sequence: 0,
                previous_sha256: None,
                header,
                phase: Phase::Preparing,
                operations: vec![],
            },
            hash: None,
            next: 0,
        };
        j.save()?;
        Ok(j)
    }
    pub fn save(&mut self) -> Result<()> {
        self.record.sequence = self.next;
        self.record.previous_sha256 = self.hash.clone();
        structure(&self.record, &self.directory.path)?;
        let payload = journal::encode(&self.record)?;
        let sha256 = journal::digest(payload.as_bytes());
        let envelope = journal::encode(&Envelope {
            sha256: sha256.clone(),
            payload,
        })?;
        if envelope.len() as u64 > journal::LIMIT || self.next >= 4096 {
            return Err(Error::Invalid("batch checkpoint limit".into()));
        }
        self.directory
            .create(&format!("{:06}.json", self.next), envelope.as_bytes())?;
        self.hash = Some(sha256);
        self.next += 1;
        Ok(())
    }
    pub fn open(host: &HostContext, repository: &str, tx: &str) -> Result<Self> {
        if !journal::valid_id(tx) {
            return Err(Error::Invalid("invalid transaction ID".into()));
        }
        let directory =
            PrivateDir::open(&journal::transactions(host, repository)?.join(tx), false)?
                .ok_or_else(|| Error::Invalid("unknown transaction".into()))?;
        let (d, record, hash) = scan(&directory, repository, tx);
        if d.problem.is_some() {
            return Err(Error::Invalid(
                "malformed transaction; read-only diagnosis/manual repair required".into(),
            ));
        }
        Ok(Self {
            directory,
            record: record.ok_or_else(|| Error::Invalid("missing transaction records".into()))?,
            hash,
            next: d.valid_checkpoints,
        })
    }
    pub fn verify_backups(&self) -> Result<()> {
        verify_backups(&self.directory, &self.record.header)
    }
}
fn verify_backups(dir: &PrivateDir, h: &Header) -> Result<()> {
    for b in &h.backups {
        if journal::digest(&journal::read_bytes(dir, &b.name)?) != b.sha256 {
            return Err(Error::Invalid("backup integrity mismatch".into()));
        }
    }
    Ok(())
}

fn structure(r: &Record, dir: &Path) -> Result<()> {
    let h = &r.header;
    if !journal::valid_id(&h.approved_plan_sha256)
        || h.watches.is_empty()
        || h.watches.len() > crate::batch::MAX_FILES
        || h.acknowledged_ids
            != if h.action == Action::Remove {
                h.watches
                    .iter()
                    .map(|w| w.mapping.id.clone())
                    .collect::<Vec<_>>()
            } else {
                vec![]
            }
        || (h.action == Action::Apply) != h.new_manifest_sha256.is_none()
        || h.manifest.kind != Kind::Regular
        || h.backups != backup_specs(h)?
    {
        return Err(Error::Invalid("invalid batch header/backup binding".into()));
    }
    if h.watches
        .windows(2)
        .any(|w| w[0].mapping.id >= w[1].mapping.id)
    {
        return Err(Error::Invalid("unordered batch watches".into()));
    }
    let expected = expected(h)?;
    if r.operations.is_empty() {
        if matches!(r.phase, Phase::Preparing | Phase::RolledBack) {
            return Ok(());
        }
        return Err(Error::Invalid("missing batch operations".into()));
    }
    if r.phase == Phase::Preparing || r.operations.len() != expected.len() {
        return Err(Error::Invalid("invalid batch operation set".into()));
    }
    let mut boundary = false;
    let mut inverse = false;
    let mut intents = 0;
    for (i, (op, (role, mutation, path))) in r.operations.iter().zip(expected).enumerate() {
        if op.id != i
            || op.role != role
            || op.mutation != mutation
            || op.before.path != path
            || op.staged.path != dir.join(format!("op-{i:04}"))
            || op.after.as_ref().is_some_and(|e| e.path != path)
            || (op.progress == Progress::Done && op.after.is_none())
            || (matches!(op.progress, Progress::Pending | Progress::Intent) && op.after.is_some())
        {
            return Err(Error::Invalid(
                "invalid batch operation identity/postcondition".into(),
            ));
        }
        match role {
            Role::Directory
                if op.before.kind == Kind::Missing && op.staged.kind == Kind::Directory => {}
            Role::Manifest
                if op.before == h.manifest
                    && op.staged.kind == Kind::Regular
                    && op.staged.sha256 == h.new_manifest_sha256 => {}
            Role::Source => {
                let w = h
                    .watches
                    .iter()
                    .find(|w| w.source.path == path)
                    .ok_or_else(|| Error::Invalid("unbound adoption source".into()))?;
                if h.action != Action::Adopt
                    || op.before != w.source
                    || op.before.kind != Kind::Missing
                    || op.staged.kind != Kind::Regular
                    || op.staged.sha256 != w.destination.sha256
                {
                    return Err(Error::Invalid("invalid adoption source binding".into()));
                }
            }
            Role::Destination => {
                let w = h
                    .watches
                    .iter()
                    .find(|w| w.destination.path == path)
                    .ok_or_else(|| Error::Invalid("unbound destination".into()))?;
                let ok = match h.action {
                    Action::Apply | Action::Adopt => {
                        op.staged.kind == Kind::Symlink
                            && op.staged.link_target.as_ref() == Some(&w.source.path)
                    }
                    Action::Keep => {
                        op.staged.kind == Kind::Regular && op.staged.sha256 == w.source.sha256
                    }
                    Action::Remove => {
                        op.staged.kind == Kind::Missing && op.staged.fingerprint.is_none()
                    }
                };
                if op.before != w.destination || !ok {
                    return Err(Error::Invalid("invalid destination binding".into()));
                }
            }
            _ => {
                return Err(Error::Invalid(
                    "invalid directory/manifest preparation".into(),
                ));
            }
        }
        match r.phase {
            Phase::Running => match op.progress {
                Progress::Done if !boundary => {}
                Progress::Intent if !boundary => boundary = true,
                Progress::Pending => boundary = true,
                _ => return Err(Error::Invalid("invalid forward ordering".into())),
            },
            Phase::Verifying | Phase::Completed if op.progress != Progress::Done => {
                return Err(Error::Invalid("unverified batch completion".into()));
            }
            Phase::RollingBack => {
                if op.progress == Progress::UndoIntent {
                    intents += 1;
                }
                if intents > 1 {
                    return Err(Error::Invalid("multiple inverse intents".into()));
                }
                if matches!(op.progress, Progress::UndoIntent | Progress::Undone) {
                    inverse = true;
                } else if inverse {
                    return Err(Error::Invalid("invalid inverse ordering".into()));
                }
            }
            Phase::RolledBack if op.progress != Progress::Undone => {
                return Err(Error::Invalid("unfinished batch inverse".into()));
            }
            _ => {}
        }
    }
    Ok(())
}
fn transition(a: &Record, b: &Record) -> Result<()> {
    let phase_ok = match a.phase {
        Phase::Preparing => matches!(b.phase, Phase::Running | Phase::RolledBack),
        Phase::Running => matches!(
            b.phase,
            Phase::Running | Phase::Verifying | Phase::RollingBack
        ),
        Phase::Verifying => matches!(b.phase, Phase::Completed | Phase::RollingBack),
        Phase::Completed => b.phase == Phase::RollingBack,
        Phase::RollingBack => matches!(b.phase, Phase::RollingBack | Phase::RolledBack),
        Phase::RolledBack => false,
    };
    if !phase_ok || journal::encode(&a.header)? != journal::encode(&b.header)? {
        return Err(Error::Invalid(
            "invalid batch phase/header transition".into(),
        ));
    }
    if a.phase == Phase::Preparing {
        return Ok(());
    }
    if a.operations.len() != b.operations.len() {
        return Err(Error::Invalid("changed batch operations".into()));
    }
    for (a, b) in a.operations.iter().zip(&b.operations) {
        if a.id != b.id
            || a.role != b.role
            || a.mutation != b.mutation
            || a.before != b.before
            || a.staged != b.staged
            || (a.after.is_some() && a.after != b.after)
            || !(a.progress == b.progress
                || matches!(
                    (a.progress, b.progress),
                    (Progress::Pending, Progress::Intent)
                        | (Progress::Intent, Progress::Done)
                        | (
                            Progress::Pending | Progress::Intent | Progress::Done,
                            Progress::UndoIntent
                        )
                        | (Progress::UndoIntent, Progress::Undone)
                ))
        {
            return Err(Error::Invalid("invalid batch operation transition".into()));
        }
    }
    Ok(())
}

// Dispatch is only a hint; scan validates the entire checksum chain independently.
pub(crate) fn is_schema4(dir: &PrivateDir) -> bool {
    journal::read_bytes(dir, "000000.json")
        .ok()
        .and_then(|b| serde_json::from_slice::<Envelope>(&b).ok())
        .and_then(|e| serde_json::from_str::<serde_json::Value>(&e.payload).ok())
        .is_some_and(|v| v.get("schema").and_then(|v| v.as_u64()) == Some(4))
}
pub(crate) fn scan(
    dir: &PrivateDir,
    repo: &str,
    tx: &str,
) -> (Diagnosis, Option<Record>, Option<String>) {
    let mut d = Diagnosis {
        repository_id: repo.into(),
        transaction_id: tx.into(),
        journal_schema: None,
        phase: None,
        valid_checkpoints: 0,
        problem: None,
    };
    let mut latest: Option<Record> = None;
    let mut hash = None;
    let result: Result<()> = (|| {
        let names = observation::names(&dir.path, 4096)?;
        let checkpoints: Vec<_> = names
            .iter()
            .filter_map(|n| n.to_str())
            .filter(|n| n.ends_with(".json"))
            .collect();
        if checkpoints.is_empty() {
            return Err(Error::Invalid("transaction has no durable intent".into()));
        }
        for (i, name) in checkpoints.iter().enumerate() {
            if *name != format!("{i:06}.json") {
                return Err(Error::Invalid("checkpoint sequence gap".into()));
            }
            let e: Envelope = serde_json::from_slice(&journal::read_bytes(dir, name)?)
                .map_err(|_| Error::Invalid("truncated or malformed checkpoint".into()))?;
            if journal::digest(e.payload.as_bytes()) != e.sha256 {
                return Err(Error::Invalid("checkpoint checksum mismatch".into()));
            }
            let r: Record = serde_json::from_str(&e.payload)
                .map_err(|_| Error::Invalid("unsupported or malformed batch checkpoint".into()))?;
            if r.schema != 4
                || r.sequence != i
                || r.previous_sha256 != hash
                || r.header.repository_id != repo
                || r.header.transaction_id != tx
            {
                return Err(Error::Invalid(
                    "unsupported schema or invalid batch binding".into(),
                ));
            }
            structure(&r, &dir.path)?;
            if let Some(a) = &latest {
                transition(a, &r)?;
            } else if r.phase != Phase::Preparing || !r.operations.is_empty() {
                return Err(Error::Invalid("first checkpoint must be preparing".into()));
            }
            d.valid_checkpoints += 1;
            d.phase = Some(r.phase);
            d.journal_schema = Some(r.schema);
            hash = Some(e.sha256);
            latest = Some(r);
        }
        let r = latest.as_ref().unwrap();
        for name in &names {
            let name = name
                .to_str()
                .ok_or_else(|| Error::Invalid("unrepresentable transaction artifact".into()))?;
            let stage = name.strip_prefix("op-").is_some_and(|s| {
                s.len() == 4
                    && s.bytes().all(|b| b.is_ascii_digit())
                    && (r.operations.is_empty()
                        || s.parse::<usize>().is_ok_and(|i| i < r.operations.len()))
            });
            if !name.ends_with(".json")
                && !stage
                && !r.header.backups.iter().any(|b| b.name == name)
            {
                return Err(Error::Invalid("unknown batch transaction artifact".into()));
            }
        }
        if !r.operations.is_empty() {
            verify_backups(dir, &r.header)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        d.problem = Some(e.to_string());
    }
    (d, latest, hash)
}
