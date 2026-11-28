//! Machine-local settings for declared Jinja inputs.
use super::{find_file, inputs, renderer_error};
use crate::{
    Error, Result,
    context::HostContext,
    journal,
    local::{LocalConfig, repository_state_path},
    model::ManagedFile,
    observation::{self, Entry, Fingerprint},
    state::{PrivateDir, WriterLock},
    template::{self, CONTEXT_LIMIT, VALUE_LIMIT},
};
use rustix::{fs, io::Errno};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

const FILE_LIMIT: u64 = 4 * CONTEXT_LIMIT as u64;
pub(super) type Values = BTreeMap<String, String>;

pub struct Row {
    pub name: String,
    pub value: Option<String>,
}

pub struct Review<'a> {
    pub repository: &'a Path,
    pub id: &'a str,
    pub name: &'a str,
    pub previous: Option<&'a str>,
    pub proposed: Option<&'a str>,
}

pub struct Preview {
    host: HostContext,
    slot: Slot,
    manifest: Entry,
    before: Stored,
    name: String,
    proposed: Values,
}

pub struct Approval(Preview);

pub fn inspect(host: &HostContext, repository: &Path, id: &str) -> Result<Vec<Row>> {
    let (slot, _, file) = read_scope(host, repository, id)?;
    let Some(names) = file.local_inputs.filter(|names| !names.is_empty()) else {
        return Ok(vec![]);
    };
    let mut stored = slot.load()?.values;
    Ok(names
        .into_iter()
        .map(|name| Row {
            value: stored.remove(&name),
            name,
        })
        .collect())
}

impl Preview {
    /// Set a literal value, including an empty string, or clear it with `None`.
    pub fn prepare(
        host: &HostContext,
        repository: &Path,
        id: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<Self> {
        let (slot, manifest, file) = read_scope(host, repository, id)?;
        if !file.local_inputs.iter().flatten().any(|key| key == name) {
            return Err(Error::Invalid(
                "Declare the input in local_inputs in dort.toml before setting it here.".into(),
            ));
        }
        if let Some(value) = value {
            validate_value(value)?;
        }
        let before = slot.load()?;
        let mut proposed = before.values.clone();
        match value {
            Some(value) => {
                proposed.insert(name.into(), value.into());
            }
            None => {
                proposed.remove(name);
            }
        }
        validate_values(id, &proposed)?;
        validate_resolved(host, &file, &proposed)?;
        Ok(Self {
            host: host.clone(),
            slot,
            manifest,
            before,
            name: name.into(),
            proposed,
        })
    }

    pub fn review(&self) -> Review<'_> {
        Review {
            repository: self.manifest.path.parent().expect("manifest parent"),
            id: &self.slot.identity.mapping,
            name: &self.name,
            previous: self.before.values.get(&self.name).map(String::as_str),
            proposed: self.proposed.get(&self.name).map(String::as_str),
        }
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }

    fn check(&self) -> Result<()> {
        let review = self.review();
        let (slot, manifest, file) =
            read_scope(&self.host, review.repository, review.id).map_err(|_| stale())?;
        if manifest != self.manifest
            || slot.identity != self.slot.identity
            || slot.load().map_err(|_| stale())? != self.before
        {
            return Err(stale());
        }
        validate_resolved(&self.host, &file, &self.proposed).map_err(|_| stale())
    }
}

impl Approval {
    /// Returns whether the settings changed.
    pub fn execute(self) -> Result<bool> {
        let p = self.0;
        let _lock = WriterLock::acquire(&p.host)?;
        journal::ensure_writable(&p.host)?;
        p.check()?;
        if p.before.values == p.proposed {
            return Ok(false);
        }
        let text = serde_json::to_string(&StoredFile {
            identity: p.slot.identity.clone(),
            values: p.proposed.clone(),
        })
        .map_err(|_| invalid_storage())?;
        if text.len() as u64 > FILE_LIMIT {
            return Err(invalid_storage());
        }
        let dir = PrivateDir::open(&p.slot.folder, true)?.expect("created local input folder");
        dir.replace_checked(&p.slot.name, &text, || {
            p.check()?;
            let current = PrivateDir::open(&p.slot.folder, false)?.ok_or_else(stale)?;
            let identity = |dir: &PrivateDir| {
                let stat = fs::fstat(&dir.file).map_err(|_| stale())?;
                Ok::<_, Error>((stat.st_dev, stat.st_ino))
            };
            if identity(&dir)? != identity(&current)? {
                return Err(stale());
            }
            Ok(())
        })?;
        Ok(true)
    }
}

fn read_scope(
    host: &HostContext,
    repository: &Path,
    id: &str,
) -> Result<(Slot, Entry, ManagedFile)> {
    observation::validate_output_path(repository)?;
    let local = LocalConfig::load_bound(host)?.0;
    let repository_id = local.repository_id(repository).ok_or_else(|| {
        Error::Invalid("Register this repository before setting machine-local inputs.".into())
    })?;
    let (manifest, entry) = observation::read_manifest(&repository.join("dort.toml"))
        .map_err(|error| match error {
            Error::Parse(_) | Error::Validation(_) => Error::Invalid(
                "Invalid dort.toml. Check the generated file declaration before editing local inputs."
                    .into(),
            ),
            other => other,
        })?;
    let file = find_file(&manifest, id)?.clone();
    Ok((
        Slot::new(host, repository_id, id)?,
        entry.stable_parents(),
        file,
    ))
}

pub(super) fn load(
    host: &HostContext,
    repository_id: Option<&str>,
    file: &ManagedFile,
) -> Result<Values> {
    match repository_id {
        Some(id)
            if file
                .local_inputs
                .as_ref()
                .is_some_and(|names| !names.is_empty()) =>
        {
            Ok(Slot::new(host, id, &file.id)?.load()?.values)
        }
        _ => Ok(Values::new()),
    }
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    repository: String,
    mapping: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFile {
    identity: Identity,
    values: Values,
}

#[derive(Default, PartialEq, Eq)]
struct Stored {
    fingerprint: Option<Fingerprint>,
    values: Values,
}

struct Slot {
    identity: Identity,
    folder: PathBuf,
    name: String,
}

impl Slot {
    fn new(host: &HostContext, repository: &str, mapping: &str) -> Result<Self> {
        Ok(Self {
            identity: Identity {
                repository: repository.into(),
                mapping: mapping.into(),
            },
            folder: repository_state_path(host, repository)?.join("generated-inputs"),
            name: format!("{}.json", journal::digest(mapping.as_bytes())),
        })
    }

    fn load(&self) -> Result<Stored> {
        let Some(dir) = PrivateDir::open(&self.folder, false)? else {
            return Ok(Stored::default());
        };
        let name = OsStr::new(&self.name);
        let stat = match fs::statat(&dir.file, name, fs::AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Ok(Stored::default()),
            Err(_) => return Err(invalid_storage()),
        };
        let fp = Fingerprint::from_stat(&stat);
        if !super::record::owned_regular(&stat, &fp) || fp.mode & 0o7077 != 0 {
            return Err(invalid_storage());
        }
        let bytes =
            observation::read_at_checked(&dir.file, name, &dir.path.join(name), &fp, FILE_LIMIT)
                .map_err(|_| invalid_storage())?;
        let stored: StoredFile = serde_json::from_slice(&bytes).map_err(|_| invalid_storage())?;
        if stored.identity != self.identity {
            return Err(invalid_storage());
        }
        validate_values(&self.identity.mapping, &stored.values)?;
        Ok(Stored {
            fingerprint: Some(fp),
            values: stored.values,
        })
    }
}

fn validate_value(value: &str) -> Result<()> {
    if value.len() > VALUE_LIMIT {
        return Err(Error::Invalid(
            "Machine-local values can use up to 16 KiB each.".into(),
        ));
    }
    if value
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
    {
        return Err(Error::Invalid(
            "Use plain text for machine-local inputs. Remove control characters.".into(),
        ));
    }
    if observation::sensitive_content(value.as_bytes()) {
        return Err(Error::Invalid(
            "Use non-secret text for machine-local inputs. Remove credentials and private keys."
                .into(),
        ));
    }
    Ok(())
}

fn validate_values(id: &str, values: &Values) -> Result<()> {
    for value in values.values() {
        validate_value(value)?;
    }
    let context = template::Context::from([(
        "local".into(),
        template::Input::Map(inputs::text_map(values)),
    )]);
    template::validate_context(id, &context).map_err(renderer_error)
}

fn validate_resolved(host: &HostContext, file: &ManagedFile, proposed: &Values) -> Result<()> {
    let (context, _) = inputs::resolve(file, host, proposed)?;
    template::validate_context(&file.id, &context).map_err(renderer_error)
}

fn invalid_storage() -> Error {
    Error::Invalid(
        "Cannot read or save machine-local inputs safely. Check Dort's local state.".into(),
    )
}

fn stale() -> Error {
    Error::Invalid(
        "The generated file declaration or machine-local settings changed. Open a new review."
            .into(),
    )
}
