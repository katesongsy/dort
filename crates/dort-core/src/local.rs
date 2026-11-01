use crate::{
    Error, Manifest, Result,
    context::HostContext,
    error::io_error,
    state::{PrivateDir, WriterLock},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

fn schema_one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryRecord {
    pub root: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    #[serde(default = "schema_one")]
    pub schema: u32,
    #[serde(default)]
    pub active_profiles: Vec<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, toml::Value>,
    pub active_repository: Option<String>,
    #[serde(default)]
    pub repositories: BTreeMap<String, RepositoryRecord>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            schema: 1,
            active_profiles: vec![],
            variables: BTreeMap::new(),
            active_repository: None,
            repositories: BTreeMap::new(),
        }
    }
}

impl LocalConfig {
    /// No Debug implementation: variable values must not leak through debug logging.
    pub fn load(host: &HostContext) -> Result<Self> {
        let Some(directory) = PrivateDir::open(&host.config_dir, false)? else {
            return Ok(Self::default());
        };
        let Some(text) = directory.read("local.toml")? else {
            return Ok(Self::default());
        };
        let config: Self = toml::from_str(&text)
            .map_err(|_| Error::Invalid("invalid local configuration (values redacted)".into()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            return Err(Error::Invalid(
                "unsupported local configuration schema".into(),
            ));
        }
        for (id, record) in &self.repositories {
            validate_repository_id(id)?;
            crate::context::validate_absolute(&record.root)?;
        }
        if self
            .active_repository
            .as_ref()
            .is_some_and(|id| !self.repositories.contains_key(id))
        {
            return Err(Error::Invalid("active repository is not registered".into()));
        }
        let roots: std::collections::BTreeSet<_> =
            self.repositories.values().map(|r| &r.root).collect();
        if roots.len() != self.repositories.len() {
            return Err(Error::Invalid(
                "repository root is registered more than once".into(),
            ));
        }
        Ok(())
    }

    fn save(&self, host: &HostContext, lock: &WriterLock) -> Result<()> {
        lock.check_context(host)?;
        self.validate()?;
        let text = toml::to_string(self).map_err(|_| {
            Error::Invalid("could not encode local configuration (values redacted)".into())
        })?;
        let directory =
            PrivateDir::open(&host.config_dir, true)?.expect("created config directory");
        directory.replace("local.toml", &text)
    }

    pub fn select_repository(&self, explicit: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = explicit {
            return path
                .canonicalize()
                .map_err(|e| io_error("resolve explicit repository", path, e));
        }
        self.active_repository
            .as_ref()
            .and_then(|id| self.repositories.get(id))
            .map(|r| r.root.clone())
            .ok_or(Error::NoRepository)
    }
}

/// Foundation APIs only: callers must hold the application lock.
/// These operations register metadata, never initialise Git or touch live dotfiles.
pub struct RepositoryRegistry;

impl RepositoryRegistry {
    pub fn register(host: &HostContext, lock: &WriterLock, root: &Path) -> Result<String> {
        lock.check_context(host)?;
        crate::journal::ensure_writable(host)?;
        let root = inspect_repository(host, root)?;
        let mut config = LocalConfig::load(host)?;
        let id = config
            .repositories
            .iter()
            .find(|(_, r)| r.root == root)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(root.as_os_str().as_bytes())));
        config
            .repositories
            .insert(id.clone(), RepositoryRecord { root });
        config.active_repository = Some(id.clone());
        config.save(host, lock)?;
        Ok(id)
    }

    /// Preserve the state ID after a deliberate move. Refuse forks or missing manifests.
    pub fn relocate(host: &HostContext, lock: &WriterLock, id: &str, root: &Path) -> Result<()> {
        lock.check_context(host)?;
        crate::journal::ensure_writable(host)?;
        let mut config = LocalConfig::load(host)?;
        let old = config
            .repositories
            .get(id)
            .ok_or_else(|| Error::Invalid("unknown repository ID".into()))?;
        if old
            .root
            .try_exists()
            .map_err(|e| io_error("inspect previous repository", &old.root, e))?
        {
            return Err(Error::Invalid(
                "old repository still exists; refusing ambiguous relocation".into(),
            ));
        }
        let root = inspect_repository(host, root)?;
        if config.repositories.values().any(|r| r.root == root) {
            return Err(Error::Invalid(
                "new repository root is already registered".into(),
            ));
        }
        config
            .repositories
            .insert(id.into(), RepositoryRecord { root });
        config.save(host, lock)
    }
}

pub fn repository_state_path(host: &HostContext, id: &str) -> Result<PathBuf> {
    validate_repository_id(id)?;
    Ok(host.state_dir.join("repositories").join(id))
}

fn validate_repository_id(id: &str) -> Result<()> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("invalid local repository ID".into()));
    }
    Ok(())
}

fn inspect_repository(host: &HostContext, root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|e| io_error("resolve repository", root, e))?;
    if root.to_str().is_none() {
        return Err(Error::Invalid(
            "non-UTF-8 repository paths cannot be stored in TOML yet".into(),
        ));
    }
    if root == host.home
        || root.parent().is_none()
        || root.starts_with(&host.config_dir)
        || host.config_dir.starts_with(&root)
        || root.starts_with(&host.state_dir)
        || host.state_dir.starts_with(&root)
    {
        return Err(Error::Invalid(
            "repository cannot be HOME or filesystem root, or overlap Dort metadata".into(),
        ));
    }
    if !root
        .join(".git")
        .try_exists()
        .map_err(|e| io_error("inspect Git metadata", &root, e))?
    {
        return Err(Error::Invalid(
            "repository must already have Git metadata; Git initialisation is not implemented yet"
                .into(),
        ));
    }
    let path = root.join("dort.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| io_error("read repository manifest", &path, e))?;
    Manifest::parse(&text)?;
    Ok(root)
}
