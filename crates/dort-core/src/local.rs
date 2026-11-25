use crate::{
    Error, Manifest, Result,
    context::HostContext,
    error::io_error,
    state::{PrivateDir, WriterLock},
    tools::inspection::Query,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub mod registry_edit;

fn schema_one() -> u32 {
    1
}

/// Bind the file's full identity but only parent identities: unrelated sibling
/// creation changes a parent clock without making this read unsafe.
fn same_read(before: &crate::observation::Entry, after: &crate::observation::Entry) -> bool {
    before.path == after.path
        && before.kind == after.kind
        && before.fingerprint == after.fingerprint
        && before.link_target == after.link_target
        && before.parents.len() == after.parents.len()
        && before.parents.iter().zip(&after.parents).all(|(a, b)| {
            a.path == b.path && crate::adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryRecord {
    pub root: PathBuf,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSettingsScope {
    pub query: Query,
    pub id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSettings {
    pub scope: ToolSettingsScope,
    pub program: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub npm_cli: Option<PathBuf>,
    pub prefix: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    #[serde(default = "schema_one")]
    pub schema: u32,
    #[serde(default)]
    pub active_profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    profile_choices: BTreeMap<String, BTreeMap<String, crate::profiles::Choice>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tool_settings: BTreeMap<PathBuf, Vec<ToolSettings>>,
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
            profile_choices: BTreeMap::new(),
            tool_settings: BTreeMap::new(),
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

    /// Bounded, stable private configuration read for source-bound Tools writes.
    /// Only the fingerprint/hash escapes; variable values remain private.
    pub(crate) fn load_bound(host: &HostContext) -> Result<(Self, crate::observation::Entry)> {
        let directory = PrivateDir::open(&host.config_dir, false)?
            .ok_or_else(|| Error::Invalid("registered local configuration required".into()))?;
        let before = crate::observation::inspect(&host.local_config_path(), false)?;
        let bytes = crate::journal::read_bytes(&directory, "local.toml")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Invalid("invalid local configuration (values redacted)".into()))?;
        let config: Self = toml::from_str(text)
            .map_err(|_| Error::Invalid("invalid local configuration (values redacted)".into()))?;
        config.validate()?;
        let mut after = crate::observation::inspect(&host.local_config_path(), false)?;
        if !same_read(&before, &after) {
            return Err(Error::Invalid(
                "local configuration changed during read".into(),
            ));
        }
        after.sha256 = Some(crate::journal::digest(&bytes));
        Ok((config, after))
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

    pub fn tool_settings(&self, manifest: &Path) -> &[ToolSettings] {
        self.tool_settings.get(manifest).map_or(&[], Vec::as_slice)
    }

    pub fn save_tool_settings(
        host: &HostContext,
        lock: &WriterLock,
        manifest: PathBuf,
        settings: ToolSettings,
    ) -> Result<()> {
        lock.check_context(host)?;
        crate::context::validate_absolute(&manifest)?;
        for path in std::iter::once(&settings.program)
            .chain(&settings.search_dirs)
            .chain(&settings.npm_cli)
            .chain(&settings.prefix)
        {
            crate::context::validate_absolute(path)?;
        }
        let mut local = Self::load(host)?;
        let rows = local.tool_settings.entry(manifest).or_default();
        rows.retain(|row| row.scope != settings.scope);
        rows.push(settings);
        local.save(host, lock)
    }

    pub fn profile_rows(
        &self,
        profiles: &crate::profiles::Profiles,
        platform: crate::platform::Platform,
        repository: Option<&Path>,
        temporary: &[String],
    ) -> Result<Vec<crate::profiles::Row>> {
        let saved = repository
            .and_then(|root| {
                self.repositories
                    .iter()
                    .find(|(_, record)| record.root == root)
            })
            .and_then(|(id, _)| self.profile_choices.get(id));
        profiles.rows(
            platform,
            &self.active_profiles,
            saved.unwrap_or(&BTreeMap::new()),
            temporary,
        )
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

#[derive(Serialize)]
pub struct ProfilePreview {
    repository: PathBuf,
    name: String,
    choice: crate::profiles::Choice,
    #[serde(skip)]
    manifest: crate::observation::Entry,
    #[serde(skip)]
    config: crate::observation::Entry,
}

impl ProfilePreview {
    pub fn prepare(
        host: &HostContext,
        repository: &Path,
        name: &str,
        choice: crate::profiles::Choice,
    ) -> Result<Self> {
        let (local, config) = LocalConfig::load_bound(host)?;
        if !local.repositories.values().any(|r| r.root == repository) {
            return Err(Error::Invalid(
                "Profile choices need a registered repository".into(),
            ));
        }
        let (manifest, entry) = crate::observation::read_manifest(&repository.join("dort.toml"))?;
        if name == "base" || !manifest.profiles.known().contains(name) {
            return Err(Error::Invalid(
                "Choose a known profile other than base".into(),
            ));
        }
        Ok(Self {
            repository: repository.to_owned(),
            name: name.into(),
            choice,
            manifest: entry,
            config,
        })
    }

    pub fn human(&self) -> String {
        format!(
            "Repository: {:?}\nProfile: {}\nSave: {}\n\nChanges the local profile choice only. Files and tools stay in place.",
            self.repository,
            self.name,
            self.choice.label()
        )
    }

    pub fn approve(self) -> ProfileApproval {
        ProfileApproval(self)
    }
}

pub struct ProfileApproval(ProfilePreview);

impl ProfileApproval {
    pub fn execute(self, host: &HostContext) -> Result<()> {
        let lock = WriterLock::acquire(host)?;
        let p = self.0;
        let fresh = ProfilePreview::prepare(host, &p.repository, &p.name, p.choice)?;
        if !crate::adoption::same(&p.manifest, &fresh.manifest, true)
            || !crate::adoption::same(&p.config, &fresh.config, true)
        {
            return Err(Error::Invalid(
                "Manifest or profile choices changed. Review again.".into(),
            ));
        }
        let (mut local, config) = LocalConfig::load_bound(host)?;
        if !same_read(&fresh.config, &config) || fresh.config.sha256 != config.sha256 {
            return Err(Error::Invalid(
                "Profile choices changed. Review again.".into(),
            ));
        }
        let id = local
            .repositories
            .iter()
            .find(|(_, r)| r.root == p.repository)
            .map(|(id, _)| id.clone())
            .ok_or(Error::NoRepository)?;
        local
            .profile_choices
            .entry(id)
            .or_default()
            .insert(p.name, p.choice);
        local.save(host, &lock)
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

pub(crate) fn registration_text(host: &HostContext, root: &Path) -> Result<(String, String)> {
    let mut config = LocalConfig::load(host)?;
    let id = config
        .repositories
        .iter()
        .find(|(_, r)| r.root == root)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| format!("{:x}", Sha256::digest(root.as_os_str().as_bytes())));
    config.repositories.insert(
        id.clone(),
        RepositoryRecord {
            root: root.to_owned(),
        },
    );
    config.active_repository = Some(id.clone());
    config.validate()?;
    let text = toml::to_string(&config)
        .map_err(|_| Error::Invalid("cannot encode registration (values redacted)".into()))?;
    Ok((id, text))
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
            "registry requires existing Git metadata; use bootstrap to create a repository".into(),
        ));
    }
    let path = root.join("dort.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| io_error("read repository manifest", &path, e))?;
    Manifest::parse(&text)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::same_read;
    use crate::observation::{Entry, Fingerprint, Kind, Parent};

    fn fingerprint(inode: u64, modified: i64) -> Fingerprint {
        Fingerprint {
            device: 1,
            inode,
            links: 1,
            mode: 0o100600,
            uid: 501,
            gid: 20,
            size: 10,
            modified: (modified, 0),
            changed: (modified, 0),
        }
    }

    fn entry(parent_modified: i64, parent_inode: u64) -> Entry {
        Entry {
            path: "/h/.config/dort/local.toml".into(),
            kind: Kind::Regular,
            fingerprint: Some(fingerprint(7, 100)),
            parents: vec![Parent {
                path: "/h/.config/dort".into(),
                fingerprint: fingerprint(parent_inode, parent_modified),
            }],
            link_target: None,
            sha256: None,
            sensitive: true,
        }
    }

    #[test]
    fn unrelated_parent_clock_changes_do_not_invalidate_a_config_read() {
        assert!(same_read(&entry(100, 3), &entry(999, 3)));
        assert!(!same_read(&entry(100, 3), &entry(100, 4)));
        let mut replaced = entry(100, 3);
        replaced.fingerprint = Some(fingerprint(8, 100));
        assert!(!same_read(&entry(100, 3), &replaced));
    }
}
