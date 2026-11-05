//! Immutable schema-1 checkpoint chains. Checksums detect corruption, not a malicious owner.
use crate::{
    Error, Result,
    context::HostContext,
    error::io_error,
    local::{LocalConfig, repository_state_path},
    model::ManagedFile,
    observation::{self, Entry, Parent},
    state::PrivateDir,
};
use rustix::fs::{self, Mode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(crate) const LIMIT: u64 = 16 * 1024 * 1024;

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|_| Error::Invalid("cannot encode transaction metadata".into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Running,
    Verifying,
    Completed,
    RollingBack,
    RolledBack,
}

impl Phase {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::RolledBack)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    Directory,
    Source,
    Manifest,
    Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Progress {
    Pending,
    Intent,
    Done,
    UndoIntent,
    Undone,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub id: usize,
    pub role: Role,
    pub before: Entry,
    pub staged: Entry,
    pub after: Option<Entry>,
    pub progress: Progress,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub repository_id: String,
    pub repository: std::path::PathBuf,
    pub transaction_id: String,
    pub approved_plan_sha256: String,
    pub mapping: ManagedFile,
    pub profiles: Vec<String>,
    pub manifest: Entry,
    pub destination: Entry,
    pub source: Entry,
    pub local_config: Entry,
    pub anchors: Vec<Parent>,
    pub manifest_backup_sha256: String,
    pub destination_backup_sha256: String,
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

/// Content-free diagnostics; malformed records never appear verbatim in errors/reports.
#[derive(Debug, Serialize)]
pub struct Diagnosis {
    pub repository_id: String,
    pub transaction_id: String,
    pub phase: Option<Phase>,
    /// Last fully validated checkpoint schema, not an untrusted dispatch hint.
    pub journal_schema: Option<u32>,
    pub valid_checkpoints: usize,
    pub problem: Option<String>,
}

impl Diagnosis {
    pub fn blocks_writes(&self) -> bool {
        self.problem.is_some() || self.phase.is_none_or(|p| !p.terminal())
    }
}

pub(crate) struct Journal {
    pub directory: PrivateDir,
    pub record: Record,
    last_sha256: Option<String>,
    next: usize,
}

pub(crate) fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn transactions(host: &HostContext, repository_id: &str) -> Result<std::path::PathBuf> {
    Ok(repository_state_path(host, repository_id)?.join("transactions"))
}

pub(crate) fn read_bytes(directory: &PrivateDir, name: &str) -> Result<Vec<u8>> {
    let mut file = File::from(
        fs::openat(
            &directory.file,
            name,
            fs::OFlags::RDONLY | fs::OFlags::NOFOLLOW | fs::OFlags::NONBLOCK | fs::OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open transaction artifact", directory.path.join(name), e))?,
    );
    directory.check_file(&file, name)?;
    let before = observation::Fingerprint::from_stat(
        &fs::fstat(&file)
            .map_err(|e| io_error("inspect transaction artifact", &directory.path, e))?,
    );
    if before.size < 0 || before.size as u64 > LIMIT {
        return Err(Error::Invalid(
            "transaction artifact exceeds size limit".into(),
        ));
    }
    let mut bytes = vec![];
    (&mut file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read transaction artifact", &directory.path, e))?;
    let after = observation::Fingerprint::from_stat(
        &fs::fstat(&file)
            .map_err(|e| io_error("recheck transaction artifact", &directory.path, e))?,
    );
    let named = observation::Fingerprint::from_stat(
        &fs::statat(&directory.file, name, fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| io_error("recheck named transaction artifact", &directory.path, e))?,
    );
    if before != after || after != named || bytes.len() as u64 > LIMIT {
        return Err(Error::Invalid(
            "transaction artifact changed during read".into(),
        ));
    }
    Ok(bytes)
}

impl Journal {
    pub(crate) fn begin(host: &HostContext, header: Header) -> Result<Self> {
        let root = transactions(host, &header.repository_id)?;
        let parent = PrivateDir::open(&root, true)?.expect("created transactions directory");
        fs::mkdirat(
            &parent.file,
            header.transaction_id.as_str(),
            Mode::RUSR | Mode::WUSR | Mode::XUSR,
        )
        .map_err(|e| io_error("allocate unique transaction", &root, e))?;
        parent.sync()?;
        let directory =
            PrivateDir::open(&root.join(&header.transaction_id), false)?.expect("new transaction");
        let record = Record {
            schema: 1,
            sequence: 0,
            previous_sha256: None,
            header,
            phase: Phase::Preparing,
            operations: vec![],
        };
        let mut journal = Self {
            directory,
            record,
            last_sha256: None,
            next: 0,
        };
        journal.save()?;
        Ok(journal)
    }

    /// A torn final file is deliberately retained. Earlier durable intent is never replaced.
    pub(crate) fn save(&mut self) -> Result<()> {
        self.record.sequence = self.next;
        self.record.previous_sha256 = self.last_sha256.clone();
        let payload = encode(&self.record)?;
        let checksum = digest(payload.as_bytes());
        let envelope = encode(&Envelope {
            sha256: checksum.clone(),
            payload,
        })?;
        if envelope.len() as u64 > LIMIT {
            return Err(Error::Invalid("checkpoint exceeds size limit".into()));
        }
        self.directory
            .create(&format!("{:06}.json", self.next), envelope.as_bytes())?;
        self.last_sha256 = Some(checksum);
        self.next += 1;
        Ok(())
    }

    pub(crate) fn open(
        host: &HostContext,
        repository_id: &str,
        transaction_id: &str,
    ) -> Result<Self> {
        if !valid_id(transaction_id) {
            return Err(Error::Invalid("invalid transaction ID".into()));
        }
        let path = transactions(host, repository_id)?.join(transaction_id);
        let directory = PrivateDir::open(&path, false)?
            .ok_or_else(|| Error::Invalid("unknown transaction".into()))?;
        let (diagnosis, latest, hash) = scan(&directory, repository_id, transaction_id);
        if diagnosis.problem.is_some() || latest.is_none() {
            return Err(Error::Invalid(
                "malformed transaction; read-only diagnosis/manual repair required".into(),
            ));
        }
        Ok(Self {
            directory,
            record: latest.unwrap(),
            last_sha256: hash,
            next: diagnosis.valid_checkpoints,
        })
    }

    pub(crate) fn verify_backups(&self) -> Result<()> {
        for (name, expected) in [
            (
                "manifest.backup",
                &self.record.header.manifest_backup_sha256,
            ),
            (
                "destination.backup",
                &self.record.header.destination_backup_sha256,
            ),
        ] {
            if digest(&read_bytes(&self.directory, name)?) != *expected {
                return Err(Error::Invalid(
                    "backup integrity mismatch; recovery refused".into(),
                ));
            }
        }
        Ok(())
    }
}

fn transition(previous: &Record, next: &Record) -> Result<()> {
    if encode(&previous.header)? != encode(&next.header)? || previous.phase.terminal() {
        // Completed transactions may be explicitly rolled back, but never resumed forward.
        if !(previous.phase == Phase::Completed
            && next.phase == Phase::RollingBack
            && encode(&previous.header)? == encode(&next.header)?)
        {
            return Err(Error::Invalid(
                "invalid journal header/terminal transition".into(),
            ));
        }
    }
    let allowed = match previous.phase {
        Phase::Preparing => matches!(next.phase, Phase::Running | Phase::RolledBack),
        Phase::Running => matches!(
            next.phase,
            Phase::Running | Phase::Verifying | Phase::RollingBack
        ),
        Phase::Verifying => matches!(next.phase, Phase::Completed | Phase::RollingBack),
        Phase::Completed => next.phase == Phase::RollingBack,
        Phase::RollingBack => matches!(next.phase, Phase::RollingBack | Phase::RolledBack),
        Phase::RolledBack => false,
    };
    if !allowed {
        return Err(Error::Invalid("invalid journal phase transition".into()));
    }
    if previous.phase != Phase::Preparing {
        if previous.operations.len() != next.operations.len() {
            return Err(Error::Invalid("journal operation set changed".into()));
        }
        for (a, b) in previous.operations.iter().zip(&next.operations) {
            if a.id != b.id
                || a.role != b.role
                || a.before != b.before
                || a.staged != b.staged
                || (a.after.is_some() && a.after != b.after)
            {
                return Err(Error::Invalid("journal operation identity changed".into()));
            }
            let ok = a.progress == b.progress
                || matches!(
                    (a.progress, b.progress),
                    (Progress::Pending, Progress::Intent)
                        | (Progress::Intent, Progress::Done)
                        | (
                            Progress::Pending | Progress::Intent | Progress::Done,
                            Progress::UndoIntent
                        )
                        | (Progress::UndoIntent, Progress::Undone)
                );
            if !ok {
                return Err(Error::Invalid("invalid operation transition".into()));
            }
        }
    }
    Ok(())
}

fn validate_structure(record: &Record, directory: &Path) -> Result<()> {
    let h = &record.header;
    if !valid_id(&h.approved_plan_sha256)
        || !valid_id(&h.manifest_backup_sha256)
        || !valid_id(&h.destination_backup_sha256)
    {
        return Err(Error::Invalid("invalid transaction digest binding".into()));
    }
    if record.phase == Phase::Preparing && !record.operations.is_empty() {
        return Err(Error::Invalid(
            "preparing checkpoint cannot contain live operations".into(),
        ));
    }
    if record.operations.is_empty() {
        if !matches!(record.phase, Phase::Preparing | Phase::RolledBack) {
            return Err(Error::Invalid("missing adoption operation set".into()));
        }
        return Ok(());
    }
    let n = record.operations.len();
    if n < 3 {
        return Err(Error::Invalid("incomplete adoption operation set".into()));
    }
    if h.source.kind != observation::Kind::Missing
        || h.manifest.kind != observation::Kind::Regular
        || h.destination.kind != observation::Kind::Regular
        || h.manifest.sha256.as_ref() != Some(&h.manifest_backup_sha256)
        || h.destination.sha256.as_ref() != Some(&h.destination_backup_sha256)
    {
        return Err(Error::Invalid(
            "invalid adoption input/backup binding".into(),
        ));
    }
    let mut forward_boundary = false;
    let mut undone_suffix = false;
    let mut inverse_intents = 0;
    for (id, op) in record.operations.iter().enumerate() {
        let expected = if id == n - 3 {
            (Role::Source, &h.source)
        } else if id == n - 2 {
            (Role::Manifest, &h.manifest)
        } else if id == n - 1 {
            (Role::Destination, &h.destination)
        } else {
            (Role::Directory, &op.before)
        };
        if op.id != id
            || op.role != expected.0
            || &op.before != expected.1
            || op.staged.path != directory.join(format!("op-{id:04}"))
            || op
                .after
                .as_ref()
                .is_some_and(|after| after.path != op.before.path)
            || (op.progress == Progress::Done && op.after.is_none())
            || (matches!(op.progress, Progress::Pending | Progress::Intent) && op.after.is_some())
        {
            return Err(Error::Invalid(
                "invalid operation ID, binding or postcondition".into(),
            ));
        }
        let staged_kind = match op.role {
            Role::Directory => observation::Kind::Directory,
            Role::Source | Role::Manifest => observation::Kind::Regular,
            Role::Destination => observation::Kind::Symlink,
        };
        if op.staged.kind != staged_kind
            || (op.role == Role::Directory && op.before.kind != observation::Kind::Missing)
            || (op.role == Role::Source && op.staged.sha256 != h.destination.sha256)
            || (op.role == Role::Destination
                && op.staged.link_target.as_ref() != Some(&h.source.path))
        {
            return Err(Error::Invalid(
                "invalid prepared operation postcondition".into(),
            ));
        }
        match record.phase {
            Phase::Running => match op.progress {
                Progress::Done if !forward_boundary => {}
                Progress::Intent if !forward_boundary => forward_boundary = true,
                Progress::Pending => forward_boundary = true,
                _ => return Err(Error::Invalid("invalid forward operation ordering".into())),
            },
            Phase::Verifying | Phase::Completed if op.progress != Progress::Done => {
                return Err(Error::Invalid(
                    "completion without verified operations".into(),
                ));
            }
            Phase::RollingBack => {
                if op.progress == Progress::UndoIntent {
                    inverse_intents += 1;
                }
                if inverse_intents > 1 {
                    return Err(Error::Invalid("multiple inverse intents".into()));
                }
                if matches!(op.progress, Progress::UndoIntent | Progress::Undone) {
                    undone_suffix = true;
                } else if undone_suffix {
                    return Err(Error::Invalid("invalid inverse operation ordering".into()));
                }
            }
            Phase::RolledBack if op.progress != Progress::Undone => {
                return Err(Error::Invalid(
                    "rollback completion without inverse operations".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn scan(
    directory: &PrivateDir,
    repository_id: &str,
    transaction_id: &str,
) -> (Diagnosis, Option<Record>, Option<String>) {
    let mut diagnosis = Diagnosis {
        repository_id: repository_id.into(),
        transaction_id: transaction_id.into(),
        journal_schema: None,
        phase: None,
        valid_checkpoints: 0,
        problem: None,
    };
    let mut latest: Option<Record> = None;
    let mut hash = None;
    let result: Result<()> = (|| {
        let names = observation::names(&directory.path, 4096)?;
        for name in &names {
            let name = name
                .to_str()
                .ok_or_else(|| Error::Invalid("unrepresentable transaction artifact".into()))?;
            let stage = name
                .strip_prefix("op-")
                .is_some_and(|id| id.len() == 4 && id.bytes().all(|b| b.is_ascii_digit()));
            if !name.ends_with(".json")
                && !stage
                && !matches!(name, "manifest.backup" | "destination.backup")
            {
                return Err(Error::Invalid("unknown transaction artifact".into()));
            }
        }
        let checkpoints: Vec<_> = names
            .iter()
            .filter_map(|n| n.to_str())
            .filter(|n| n.ends_with(".json"))
            .collect();
        if checkpoints.is_empty() {
            return Err(Error::Invalid("transaction has no durable intent".into()));
        }
        for (i, name) in checkpoints.into_iter().enumerate() {
            if name != format!("{i:06}.json") {
                return Err(Error::Invalid(
                    "checkpoint sequence gap or unknown record".into(),
                ));
            }
            let bytes = read_bytes(directory, name)?;
            let envelope: Envelope = serde_json::from_slice(&bytes)
                .map_err(|_| Error::Invalid("truncated or malformed checkpoint".into()))?;
            if digest(envelope.payload.as_bytes()) != envelope.sha256 {
                return Err(Error::Invalid("checkpoint checksum mismatch".into()));
            }
            let record: Record = serde_json::from_str(&envelope.payload)
                .map_err(|_| Error::Invalid("unsupported or malformed checkpoint".into()))?;
            if record.schema != 1
                || record.sequence != i
                || record.previous_sha256 != hash
                || record.header.repository_id != repository_id
                || record.header.transaction_id != transaction_id
            {
                return Err(Error::Invalid(
                    "unsupported schema or invalid checkpoint binding".into(),
                ));
            }
            validate_structure(&record, &directory.path)?;
            if let Some(previous) = &latest {
                transition(previous, &record)?;
            } else if record.phase != Phase::Preparing || !record.operations.is_empty() {
                return Err(Error::Invalid("first checkpoint must be preparing".into()));
            }
            diagnosis.valid_checkpoints += 1;
            diagnosis.phase = Some(record.phase);
            diagnosis.journal_schema = Some(record.schema);
            hash = Some(envelope.sha256);
            latest = Some(record);
        }
        if let Some(record) = &latest
            && !record.operations.is_empty()
        {
            for (name, expected) in [
                ("manifest.backup", &record.header.manifest_backup_sha256),
                (
                    "destination.backup",
                    &record.header.destination_backup_sha256,
                ),
            ] {
                if digest(&read_bytes(directory, name)?) != *expected {
                    return Err(Error::Invalid("backup integrity mismatch".into()));
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        diagnosis.problem = Some(error.to_string());
    }
    (diagnosis, latest, hash)
}

/// Does not create state, lock, repair, read live file contents or replay anything.
pub fn diagnose(host: &HostContext) -> Result<Vec<Diagnosis>> {
    let root = host.state_dir.join("repositories");
    if PrivateDir::open(&host.state_dir, false)?.is_none() {
        return Ok(vec![]);
    }
    if observation::inspect(&root, false)?.kind == observation::Kind::Missing {
        return Ok(vec![]);
    }
    let mut reports = vec![];
    for repository in observation::names(&root, 4096)? {
        let id = repository
            .to_str()
            .filter(|id| valid_id(id))
            .ok_or_else(|| Error::Invalid("invalid repository state directory".into()))?;
        let path = transactions(host, id)?;
        if observation::inspect(&path, false)?.kind == observation::Kind::Missing {
            continue;
        }
        PrivateDir::open(&path, false)?
            .ok_or_else(|| Error::Invalid("missing transactions directory".into()))?;
        for name in observation::names(&path, 4096)? {
            let Some(txid) = name.to_str().filter(|id| valid_id(id)) else {
                reports.push(Diagnosis {
                    repository_id: id.into(),
                    transaction_id: "<invalid name>".into(),
                    journal_schema: None,
                    phase: None,
                    valid_checkpoints: 0,
                    problem: Some("unknown transaction entry".into()),
                });
                continue;
            };
            match PrivateDir::open(&path.join(txid), false) {
                Ok(Some(directory)) => reports.push(if crate::bootstrap::is_schema3(&directory) {
                    crate::bootstrap::scan(&directory, id, txid).0
                } else if crate::batch_journal::is_schema4(&directory) {
                    crate::batch_journal::scan(&directory, id, txid).0
                } else if crate::lifecycle_journal::is_schema2(&directory) {
                    crate::lifecycle_journal::scan(&directory, id, txid).0
                } else {
                    scan(&directory, id, txid).0
                }),
                _ => reports.push(Diagnosis {
                    repository_id: id.into(),
                    transaction_id: txid.into(),
                    journal_schema: None,
                    phase: None,
                    valid_checkpoints: 0,
                    problem: Some("unsafe transaction directory".into()),
                }),
            }
        }
    }
    Ok(reports)
}

/// Registry and executor writes fail closed on every incomplete/malformed transaction.
pub(crate) fn ensure_writable(host: &HostContext) -> Result<()> {
    if diagnose(host)?.iter().any(Diagnosis::blocks_writes) {
        return Err(Error::Invalid(
            "incomplete or malformed transaction blocks writes; explicit recovery required".into(),
        ));
    }
    Ok(())
}

pub(crate) fn registered_root(host: &HostContext, id: &str, root: &Path) -> Result<()> {
    let local = LocalConfig::load(host)?;
    if local.repositories.get(id).is_none_or(|r| r.root != root) {
        return Err(Error::Invalid(
            "transaction repository is not registered at this root".into(),
        ));
    }
    Ok(())
}
