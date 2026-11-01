use crate::{
    Error, Result,
    platform::{Architecture, Platform},
};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

/// All machine-specific paths are explicit; core services never discover HOME themselves.
#[derive(Debug, Clone, Serialize)]
pub struct HostContext {
    pub home: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub platform: Platform,
    pub architecture: Architecture,
}

impl HostContext {
    pub fn new(
        home: PathBuf,
        xdg_config_home: Option<PathBuf>,
        xdg_state_home: Option<PathBuf>,
        platform: Platform,
        architecture: Architecture,
    ) -> Result<Self> {
        validate_absolute(&home)?;
        if home.parent().is_none() {
            return Err(Error::Invalid("HOME cannot be the filesystem root".into()));
        }
        let config_base = xdg_config_home.unwrap_or_else(|| home.join(".config"));
        let state_base = xdg_state_home.unwrap_or_else(|| home.join(".local/state"));
        validate_absolute(&config_base)?;
        validate_absolute(&state_base)?;
        let config_dir = config_base.join("dort");
        let state_dir = state_base.join("dort");
        if config_dir == state_dir
            || config_dir.starts_with(&state_dir)
            || state_dir.starts_with(&config_dir)
        {
            return Err(Error::Invalid(
                "Dort config and state directories must be separate".into(),
            ));
        }
        Ok(Self {
            home,
            config_dir,
            state_dir,
            platform,
            architecture,
        })
    }

    /// Called at the application boundary only. Empty XDG values mean unset.
    pub fn from_env() -> Result<Self> {
        fn env_path(name: &str) -> Option<PathBuf> {
            std::env::var_os(name)
                .filter(|s: &OsString| !s.is_empty())
                .map(PathBuf::from)
        }
        let home = env_path("HOME").ok_or_else(|| Error::Invalid("HOME must be set".into()))?;
        Self::new(
            home,
            env_path("XDG_CONFIG_HOME"),
            env_path("XDG_STATE_HOME"),
            Platform::detect()?,
            Architecture::detect()?,
        )
    }

    pub fn local_config_path(&self) -> PathBuf {
        self.config_dir.join("local.toml")
    }

    /// Lexical validation only; descriptor-safe live-file inspection is a later slice.
    pub fn target_path(&self, target: &Path, repository: Option<&Path>) -> Result<PathBuf> {
        let path = if let Ok(suffix) = target.strip_prefix("~") {
            self.home.join(suffix)
        } else {
            target.to_path_buf()
        };
        validate_absolute(&path)?;
        if path == self.home || !path.starts_with(&self.home) {
            return Err(Error::Invalid("managed targets must be below HOME".into()));
        }
        if path.starts_with(&self.config_dir)
            || self.config_dir.starts_with(&path)
            || path.starts_with(&self.state_dir)
            || self.state_dir.starts_with(&path)
            || repository.is_some_and(|repo| path.starts_with(repo) || repo.starts_with(&path))
        {
            return Err(Error::Invalid(
                "managed targets cannot overlap Dort state or its repository".into(),
            ));
        }
        Ok(path)
    }
}

pub(crate) fn validate_absolute(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .any(|b| b.is_ascii_control())
        || path
            .as_os_str()
            .as_encoded_bytes()
            .split(|b| *b == b'/')
            .any(|part| part == b"." || part == b"..")
    {
        return Err(Error::Invalid(format!(
            "expected an absolute path without traversal: {}",
            path.display()
        )));
    }
    Ok(())
}
