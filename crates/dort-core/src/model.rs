use crate::platform::Platform;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub fn base_profiles() -> Vec<String> {
    vec!["base".into()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub binary: String,
    #[serde(default = "base_profiles")]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    pub install: InstallSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Brew,
    Npm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallSpec {
    pub provider: Provider,
    pub package: String,
    pub version: Option<String>,
    pub runtime_tool: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    Link,
    Template,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedFile {
    pub id: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub mode: FileMode,
    #[serde(default = "base_profiles")]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub overrides: BTreeMap<Platform, PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    pub permissions: Option<String>,
}

impl ManagedFile {
    pub(crate) fn source_for(&self, platform: Platform) -> &std::path::Path {
        self.overrides.get(&platform).unwrap_or(&self.source)
    }
}
