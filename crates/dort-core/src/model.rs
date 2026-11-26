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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
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

pub(crate) enum FileContent<'a> {
    Source(&'a std::path::Path),
    Recipe(&'a str),
}

impl ManagedFile {
    pub fn source_for(&self, platform: Platform) -> Option<&std::path::Path> {
        self.source
            .as_ref()
            .map(|source| self.overrides.get(&platform).unwrap_or(source).as_path())
    }

    pub(crate) fn source_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.source.iter().chain(self.overrides.values())
    }

    pub(crate) fn content_for(&self, platform: Platform) -> Option<FileContent<'_>> {
        match (self.source_for(platform), self.recipe.as_deref()) {
            (Some(path), None) => Some(FileContent::Source(path)),
            (None, Some(text)) if self.mode == FileMode::Template && self.overrides.is_empty() => {
                Some(FileContent::Recipe(text))
            }
            _ => None,
        }
    }
}
