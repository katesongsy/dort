//! Tool declaration change records.
use crate::{
    Error, Result,
    context::HostContext,
    error::io_error,
    journal::{self, Diagnosis, Phase, Progress},
    model::Tool,
    observation::{self, Entry, Kind, Parent},
    state::PrivateDir,
};
use rustix::fs::{self, Mode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub repository_id: String,
    pub repository: PathBuf,
    pub transaction_id: String,
    pub approved_plan_sha256: String,
    pub tool: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<crate::tools::declaration::Change>,
    pub profiles: Vec<String>,
    pub manifest: Entry,
    pub local_config: Entry,
    pub anchors: Vec<Parent>,
    pub new_manifest_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
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
    pub operation: Option<Operation>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    payload: String,
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
        .map_err(|e| io_error("allocate declaration transaction", &root, e))?;
        parent.sync()?;
        let directory = PrivateDir::open(&root.join(&header.transaction_id), false)?.unwrap();
        let mut j = Self {
            directory,
            record: Record {
                schema: if header.change.is_some() { 6 } else { 5 },
                sequence: 0,
                previous_sha256: None,
                header,
                phase: Phase::Preparing,
                operation: None,
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
        let bytes = journal::encode(&Envelope {
            sha256: sha256.clone(),
            payload,
        })?;
        if bytes.len() as u64 > journal::LIMIT || self.next >= 4096 {
            return Err(Error::Invalid("declaration checkpoint limit".into()));
        }
        self.directory
            .create(&format!("{:06}.json", self.next), bytes.as_bytes())?;
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
                "malformed declaration transaction; read-only diagnosis/manual repair required"
                    .into(),
            ));
        }
        Ok(Self {
            directory,
            record: record.ok_or_else(|| Error::Invalid("missing transaction records".into()))?,
            hash,
            next: d.valid_checkpoints,
        })
    }
    pub fn verify_backup(&self) -> Result<()> {
        verify_backup(&self.directory, &self.record.header)
    }
}
fn verify_backup(dir: &PrivateDir, h: &Header) -> Result<()> {
    if Some(journal::digest(&journal::read_bytes(
        dir,
        "manifest.backup",
    )?)) != h.manifest.sha256
    {
        return Err(Error::Invalid(
            "declaration backup integrity mismatch".into(),
        ));
    }
    Ok(())
}
fn structure(r: &Record, dir: &Path) -> Result<()> {
    let h = &r.header;
    if !matches!((r.schema, h.change), (5, None) | (6, Some(_)))
        || !journal::valid_id(&h.repository_id)
        || !journal::valid_id(&h.transaction_id)
        || !journal::valid_id(&h.approved_plan_sha256)
        || !journal::valid_id(&h.new_manifest_sha256)
        || h.manifest.kind != Kind::Regular
        || h.manifest
            .sha256
            .as_deref()
            .is_none_or(|s| !journal::valid_id(s))
        || h.manifest.path != h.repository.join("dort.toml")
    {
        return Err(Error::Invalid("invalid declaration header".into()));
    }
    crate::context::validate_absolute(&h.repository)?;
    if let Some(op) = &r.operation {
        if r.phase == Phase::Preparing
            || op.staged.kind != Kind::Regular
            || op.staged.path != dir.join("op-0000")
            || op.staged.sha256.as_ref() != Some(&h.new_manifest_sha256)
            || op.after.as_ref().is_some_and(|e| {
                e.path != h.manifest.path || e.sha256.as_ref() != Some(&h.new_manifest_sha256)
            })
            || (op.progress == Progress::Done && op.after.is_none())
            || (matches!(op.progress, Progress::Pending | Progress::Intent) && op.after.is_some())
        {
            return Err(Error::Invalid("invalid declaration operation".into()));
        }
        let valid = match r.phase {
            Phase::Running => matches!(
                op.progress,
                Progress::Pending | Progress::Intent | Progress::Done
            ),
            Phase::Verifying | Phase::Completed => op.progress == Progress::Done,
            Phase::RollingBack => true,
            Phase::RolledBack => op.progress == Progress::Undone,
            Phase::Preparing => false,
        };
        if !valid {
            return Err(Error::Invalid("invalid declaration progress".into()));
        }
    } else if !matches!(r.phase, Phase::Preparing | Phase::RolledBack) {
        return Err(Error::Invalid("missing declaration operation".into()));
    }
    Ok(())
}
fn transition(a: &Record, b: &Record) -> Result<()> {
    let valid = match a.phase {
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
    if !valid || a.schema != b.schema || journal::encode(&a.header)? != journal::encode(&b.header)?
    {
        return Err(Error::Invalid(
            "invalid declaration phase/header transition".into(),
        ));
    }
    if a.phase == Phase::Preparing {
        if b.phase == Phase::Running
            && b.operation
                .as_ref()
                .is_none_or(|o| o.progress != Progress::Pending)
        {
            return Err(Error::Invalid("declaration must start pending".into()));
        }
        if b.phase == Phase::RolledBack && b.operation.is_some() {
            return Err(Error::Invalid("unexpected preparation inverse".into()));
        }
        return Ok(());
    }
    let (Some(a), Some(b)) = (&a.operation, &b.operation) else {
        return Err(Error::Invalid("changed declaration operation".into()));
    };
    if a.staged != b.staged
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
        return Err(Error::Invalid(
            "invalid declaration operation transition".into(),
        ));
    }
    Ok(())
}
// Hint only; scan independently validates the full chain.
pub(crate) fn is_declaration(dir: &PrivateDir) -> bool {
    journal::read_bytes(dir, "000000.json")
        .ok()
        .and_then(|b| serde_json::from_slice::<Envelope>(&b).ok())
        .and_then(|e| serde_json::from_str::<serde_json::Value>(&e.payload).ok())
        .is_some_and(|v| matches!(v.get("schema").and_then(|v| v.as_u64()), Some(5 | 6)))
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
            let r: Record = serde_json::from_str(&e.payload).map_err(|_| {
                Error::Invalid("unsupported or malformed declaration checkpoint".into())
            })?;
            // Schema 5 retains its original field set, including refusal of null additions.
            if r.schema == 5
                && serde_json::from_str::<serde_json::Value>(&e.payload)
                    .ok()
                    .and_then(|v| v.get("header")?.get("change").cloned())
                    .is_some()
            {
                return Err(Error::Invalid(
                    "schema-5 declaration has schema-6 fields".into(),
                ));
            }
            if r.sequence != i
                || r.previous_sha256 != hash
                || r.header.repository_id != repo
                || r.header.transaction_id != tx
            {
                return Err(Error::Invalid(
                    "invalid declaration checkpoint binding".into(),
                ));
            }
            structure(&r, &dir.path)?;
            if let Some(a) = &latest {
                transition(a, &r)?;
            } else if r.phase != Phase::Preparing || r.operation.is_some() {
                return Err(Error::Invalid("first checkpoint must be preparing".into()));
            }
            d.valid_checkpoints += 1;
            d.phase = Some(r.phase);
            d.journal_schema = Some(r.schema);
            hash = Some(e.sha256);
            latest = Some(r);
        }
        for name in &names {
            let name = name
                .to_str()
                .ok_or_else(|| Error::Invalid("unrepresentable transaction artifact".into()))?;
            if !name.ends_with(".json") && !matches!(name, "op-0000" | "manifest.backup") {
                return Err(Error::Invalid(
                    "unknown declaration transaction artifact".into(),
                ));
            }
        }
        if latest.as_ref().is_some_and(|r| r.operation.is_some()) {
            verify_backup(dir, &latest.as_ref().unwrap().header)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        d.problem = Some(e.to_string());
    }
    (d, latest, hash)
}
