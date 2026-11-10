//! Separate private Tools attempt storage. Never a file lifecycle journal or receipt.
use super::{Outcome, Scope, Sources, invalid};
use crate::{
    Result, adoption,
    context::HostContext,
    error::io_error,
    journal,
    observation::Fingerprint,
    state::{PrivateDir, WriterLock},
};
use rustix::fs::{self, Mode, OFlags};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fs::File, io::Read, path::PathBuf};

const MAX_ATTEMPTS: usize = 256;
const MAX_FILE: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    MissingIntent,
    Malformed,
    Changed,
}
#[derive(Serialize)]
pub struct Attempt {
    pub id: String,
    pub scope: Option<Scope>,
    pub outcome: Option<Outcome>,
    pub problem: Option<Problem>,
    pub unresolved: bool,
    pub explicitly_closed: bool,
    #[serde(skip)]
    pub(super) head: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    id: String,
    scope: Scope,
    sources: Sources,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    schema: u32,
    id: String,
    intent_sha256: String,
    outcome: Outcome,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProbeScope {
    pub executable: PathBuf,
    pub home: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm_binding: Option<super::inspection::NpmBinding>,
}
impl ProbeScope {
    pub(super) fn preview(&self) -> Result<super::inspection::Preview> {
        if let Some(b) = &self.npm_binding {
            if b.node != self.executable {
                return Err(invalid("closure Node binding differs"));
            }
            super::inspection::Preview::npm_probe(
                b.clone(),
                self.home.clone(),
                self.search_dirs.clone(),
            )
        } else {
            super::inspection::Preview::new(
                super::inspection::Query::MiseInstalled,
                self.executable.clone(),
                self.home.clone(),
                self.search_dirs.clone(),
            )
        }
        .map_err(|_| invalid("invalid explicit Tools closure scope"))
    }
    fn schema(&self) -> u32 {
        if self.npm_binding.is_some() { 2 } else { 1 }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Closure {
    schema: u32,
    id: String,
    head: String,
    scope: ProbeScope,
    observed_records: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    payload: String,
}
pub(super) struct Handle {
    pub id: String,
    head: String,
}
fn root(host: &HostContext) -> PathBuf {
    host.state_dir.join("tools-attempts")
}
fn directory(host: &HostContext, id: &str) -> Result<PrivateDir> {
    if !journal::valid_id(id) {
        return Err(invalid("invalid Tools attempt identifier"));
    }
    PrivateDir::open(&root(host).join(id), false)?.ok_or_else(|| invalid("Tools attempt not found"))
}
fn names(dir: &PrivateDir, limit: usize) -> Result<Vec<String>> {
    let mut names = vec![];
    for item in
        fs::Dir::read_from(&dir.file).map_err(|e| io_error("list Tools state", &dir.path, e))?
    {
        let item = item.map_err(|e| io_error("list Tools state entry", &dir.path, e))?;
        let raw = item.file_name().to_bytes();
        if raw == b"." || raw == b".." {
            continue;
        }
        if names.len() >= limit {
            return Err(invalid("Tools state entry limit exceeded"));
        }
        names.push(String::from_utf8(raw.to_vec()).map_err(|_| {
            invalid("Tools state names must be UTF-8; explicit manual repair required")
        })?);
    }
    names.sort();
    Ok(names)
}
fn read_bytes(dir: &PrivateDir, name: &str) -> Result<Vec<u8>> {
    let mut file = File::from(
        fs::openat(
            &dir.file,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open private Tools metadata", dir.path.join(name), e))?,
    );
    dir.check_file(&file, name)?;
    let stat = |file: &File| {
        fs::fstat(file)
            .map(|s| Fingerprint::from_stat(&s))
            .map_err(|e| io_error("inspect Tools metadata", &dir.path, e))
    };
    let before = stat(&file)?;
    if before.size < 0 || before.size as usize > MAX_FILE {
        return Err(invalid("Tools metadata exceeds bounded read limit"));
    }
    let mut bytes = vec![];
    (&mut file)
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read Tools metadata", &dir.path, e))?;
    let named = fs::statat(&dir.file, name, fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|e| io_error("recheck Tools metadata", &dir.path, e))?;
    if bytes.len() > MAX_FILE || before != stat(&file)? || before != Fingerprint::from_stat(&named)
    {
        return Err(invalid("Tools metadata changed during bounded read"));
    }
    Ok(bytes)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let payload = journal::encode(value)?;
    let bytes = journal::encode(&Envelope {
        sha256: journal::digest(payload.as_bytes()),
        payload,
    })?
    .into_bytes();
    if bytes.len() > MAX_FILE {
        return Err(invalid("Tools metadata exceeds durable record limit"));
    }
    Ok(bytes)
}
fn read<T: DeserializeOwned>(dir: &PrivateDir, name: &str) -> Result<T> {
    let envelope: Envelope = serde_json::from_slice(&read_bytes(dir, name)?)
        .map_err(|_| invalid("malformed Tools metadata; contents withheld"))?;
    if journal::digest(envelope.payload.as_bytes()) != envelope.sha256 {
        return Err(invalid("Tools metadata checksum mismatch"));
    }
    serde_json::from_str(&envelope.payload)
        .map_err(|_| invalid("malformed Tools record; contents withheld"))
}
pub(super) fn valid_scope(s: &Scope) -> bool {
    let token = |s: &str| {
        !s.is_empty()
            && s.len() <= 256
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    let absolute = super::inspection::absolute;
    journal::valid_id(&s.repository_id)
        && [s.tool.as_str(), &s.binary].into_iter().all(token)
        && if s.npm.is_some() {
            valid_npm_package(&s.package)
        } else {
            token(&s.package)
        }
        && matches!(
            super::VersionPolicy::parse(
                if s.npm.is_some() {
                    crate::model::Provider::Npm
                } else {
                    crate::model::Provider::Mise
                },
                Some(&s.version)
            ),
            Ok(super::VersionPolicy::Exact { .. })
        )
        && s.version.len() <= 256
        && s.profiles.contains("base")
        && s.profiles.iter().all(|p| token(p))
        && s.bound_tools.contains(&s.tool)
        && s.bound_tools.len() <= 256
        && s.bound_tools.iter().all(|id| token(id))
        && [&s.repository, &s.executable, &s.home]
            .into_iter()
            .all(|p| absolute(p))
        && !s.search_dirs.is_empty()
        && s.search_dirs.len() <= 256
        && s.search_dirs.iter().all(|p| absolute(p))
        && !s.tool_search_dirs.is_empty()
        && s.tool_search_dirs.len() <= super::binaries::MAX_DIRECTORIES
        && s.tool_search_dirs.iter().all(|p| absolute(p))
        && (s.npm.is_none() || super::npm::valid(s))
}
fn valid_npm_package(s: &str) -> bool {
    let name = |p: &str| {
        !p.is_empty()
            && p.len() <= 214
            && p.as_bytes()[0].is_ascii_alphanumeric()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    };
    if let Some(s) = s.strip_prefix('@') {
        s.split_once('/').is_some_and(|(a, b)| name(a) && name(b))
    } else {
        name(s)
    }
}
fn intent(dir: &PrivateDir, id: &str) -> Result<Intent> {
    let i: Intent = read(dir, "intent.json")?;
    if i.schema != if i.scope.npm.is_some() { 2 } else { 1 }
        || i.scope.npm.is_some() != i.sources.npm.is_some()
        || i.id != id
        || !valid_scope(&i.scope)
        || i.sources.manifest.path != i.scope.repository.join("dort.toml")
        || i.sources.executable.path != i.scope.executable
        || !journal::valid_id(&i.sources.host_sha256)
        || i.sources
            .manifest
            .sha256
            .as_deref()
            .is_none_or(|h| !journal::valid_id(h))
    {
        return Err(invalid("invalid Tools attempt scope"));
    }
    Ok(i)
}
// Binds immutable record bytes, all unexpected entries and directory identity.
// Unsafe/unreadable artifacts are never followed/read; their stable metadata is
// still part of the explicit closure review. This is not authenticity vs same user.
fn head(dir: &PrivateDir, exclude: Option<&str>) -> Result<String> {
    let stat = fs::fstat(&dir.file)
        .map_err(|e| io_error("inspect Tools attempt directory", &dir.path, e))?;
    let identity = (
        stat.st_dev,
        stat.st_ino,
        stat.st_uid,
        stat.st_gid,
        stat.st_mode,
    );
    let mut entries = vec![];
    for name in names(dir, 32)?
        .into_iter()
        .filter(|n| Some(n.as_str()) != exclude)
    {
        let stat = fs::statat(&dir.file, name.as_str(), fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| io_error("inspect Tools attempt entry", &dir.path, e))?;
        let fp = Fingerprint::from_stat(&stat);
        let digest = read_bytes(dir, &name)
            .ok()
            .map(|bytes| journal::digest(&bytes));
        let named = fs::statat(&dir.file, name.as_str(), fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| io_error("recheck Tools attempt entry", &dir.path, e))?;
        if fp != Fingerprint::from_stat(&named) {
            return Err(invalid("Tools attempt changed during diagnosis"));
        }
        entries.push((name, fp, digest));
    }
    Ok(journal::digest(
        journal::encode(&(identity, entries))?.as_bytes(),
    ))
}
fn closure_index(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("close-")?.strip_suffix(".json")?;
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
fn scan(dir: &PrivateDir, id: &str) -> Result<Attempt> {
    let before = head(dir, None)?;
    let files = names(dir, 32)?;
    let i = intent(dir, id).ok();
    let mut a = Attempt {
        id: id.into(),
        scope: i.as_ref().map(|i| i.scope.clone()),
        outcome: None,
        problem: None,
        unresolved: true,
        explicitly_closed: false,
        head: before.clone(),
    };
    if i.is_none() {
        a.problem = Some(if files.iter().any(|n| n == "intent.json") {
            Problem::Malformed
        } else {
            Problem::MissingIntent
        });
    }
    if files.iter().any(|n| {
        !["intent.json", "outcome.json"].contains(&n.as_str()) && closure_index(n).is_none()
    }) {
        a.problem = Some(Problem::Malformed);
    }
    if files.iter().any(|n| n == "outcome.json") {
        let completed = read::<Completed>(dir, "outcome.json").ok();
        if let (Some(i), Some(c)) = (&i, completed)
            && c.schema == i.schema
            && c.id == id
            && c.intent_sha256 == journal::digest(&read_bytes(dir, "intent.json")?)
        {
            a.outcome = Some(c.outcome);
        } else {
            a.problem = Some(Problem::Malformed);
        }
    }
    if a.problem.is_none() && a.outcome.is_some() {
        a.unresolved = false;
    }
    if let Some(name) = files.iter().rev().find(|n| closure_index(n).is_some()) {
        if let Ok(c) = read::<Closure>(dir, name)
            && c.schema == c.scope.schema()
            && c.id == id
            && c.head == head(dir, Some(name))?
            && c.scope.preview().is_ok()
            && c.observed_records <= super::planning::MAX_VERSION_CLAIMS
        {
            a.explicitly_closed = true;
            a.unresolved = false;
        } else {
            a.problem = Some(Problem::Malformed);
            a.unresolved = true;
        }
    }
    if before != head(dir, None)? {
        a.problem = Some(Problem::Changed);
        a.unresolved = true;
        a.explicitly_closed = false;
    }
    Ok(a)
}
/// Read-only; unavailable storage is an error, never a healthy empty inventory.
pub fn diagnose(host: &HostContext) -> Result<Vec<Attempt>> {
    let Some(dir) = PrivateDir::open(&root(host), false)? else {
        return Ok(vec![]);
    };
    let mut out = vec![];
    for id in names(&dir, MAX_ATTEMPTS)? {
        if !journal::valid_id(&id) {
            return Err(invalid(
                "unexpected Tools state entry; explicit manual repair required",
            ));
        }
        out.push(scan(&directory(host, &id)?, &id)?);
    }
    Ok(out)
}
pub(super) fn ensure_clear(host: &HostContext) -> Result<()> {
    let attempts = diagnose(host)?;
    if attempts.iter().any(|a| a.unresolved) {
        return Err(invalid(
            "unresolved Tools attempt; inspect and close it before another installation",
        ));
    }
    if attempts.len() >= MAX_ATTEMPTS {
        return Err(invalid(
            "Tools attempt storage limit reached; no automatic history deletion",
        ));
    }
    Ok(())
}
pub(super) fn begin(
    host: &HostContext,
    lock: &WriterLock,
    scope: Scope,
    sources: Sources,
) -> Result<Handle> {
    lock.check_context(host)?;
    ensure_clear(host)?;
    if !valid_scope(&scope) {
        return Err(invalid("invalid Tools installation scope"));
    }
    let id = adoption::transaction_id();
    let bytes = encode(&Intent {
        schema: if scope.npm.is_some() { 2 } else { 1 },
        id: id.clone(),
        scope,
        sources,
    })?;
    let parent = PrivateDir::open(&root(host), true)?.expect("created Tools state");
    fs::mkdirat(&parent.file, id.as_str(), Mode::from_raw_mode(0o700))
        .map_err(|e| io_error("allocate Tools attempt", &parent.path, e))?;
    parent.sync()?;
    let dir = directory(host, &id)?;
    dir.create("intent.json", &bytes)?;
    Ok(Handle {
        id,
        head: head(&dir, None)?,
    })
}
pub(super) fn verify_pending(host: &HostContext, handle: &Handle) -> Result<()> {
    let dir = directory(host, &handle.id)?;
    let a = scan(&dir, &handle.id)?;
    if a.head != handle.head
        || a.problem.is_some()
        || !a.unresolved
        || a.outcome.is_some()
        || a.explicitly_closed
    {
        return Err(invalid("Tools attempt changed; installer/outcome refused"));
    }
    Ok(())
}
pub(super) fn finish(
    host: &HostContext,
    lock: &WriterLock,
    handle: &Handle,
    outcome: Outcome,
) -> Result<()> {
    lock.check_context(host)?;
    verify_pending(host, handle)?;
    let dir = directory(host, &handle.id)?;
    let completed = Completed {
        schema: intent(&dir, &handle.id)?.schema,
        id: handle.id.clone(),
        intent_sha256: journal::digest(&read_bytes(&dir, "intent.json")?),
        outcome,
    };
    dir.create("outcome.json", &encode(&completed)?)
}
pub(super) fn get(host: &HostContext, id: &str) -> Result<Attempt> {
    scan(&directory(host, id)?, id)
}
pub(super) fn close(
    host: &HostContext,
    lock: &WriterLock,
    id: &str,
    expected_head: &str,
    scope: ProbeScope,
    records: usize,
) -> Result<Attempt> {
    lock.check_context(host)?;
    let dir = directory(host, id)?;
    let a = scan(&dir, id)?;
    if a.head != expected_head || !a.unresolved {
        return Err(invalid(
            "Tools closure scope changed; request a fresh review",
        ));
    }
    let c = Closure {
        schema: scope.schema(),
        id: id.into(),
        head: expected_head.into(),
        scope,
        observed_records: records,
    };
    // A later explicit closure binds every earlier artifact, including damaged
    // closure records. Nothing is overwritten or called a successful install.
    let next = names(&dir, 32)?
        .iter()
        .filter_map(|n| closure_index(n))
        .max()
        .map_or(0, |i| i + 1);
    if next >= 32 {
        return Err(invalid("Tools closure record limit reached"));
    }
    dir.create(&format!("close-{next:04}.json"), &encode(&c)?)?;
    scan(&dir, id)
}
