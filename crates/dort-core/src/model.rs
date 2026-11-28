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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum HostInput {
    Os,
    Arch,
    /// OS account name for the effective UID.
    User,
    /// OS hostname, not the friendly macOS computer name.
    Hostname,
    Home,
    /// Resolved configuration base without Dort's subdirectory.
    ConfigHome,
}
impl HostInput {
    pub const ALL: [Self; 6] = [
        Self::Os,
        Self::Arch,
        Self::User,
        Self::Hostname,
        Self::Home,
        Self::ConfigHome,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<HostInput>", into = "Vec<HostInput>")]
pub struct HostInputs(u8);

impl HostInputs {
    pub fn contains(self, input: HostInput) -> bool {
        self.0 & input.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = HostInput> {
        HostInput::ALL
            .into_iter()
            .filter(move |&input| self.contains(input))
    }
}
impl FromIterator<HostInput> for HostInputs {
    fn from_iter<T: IntoIterator<Item = HostInput>>(inputs: T) -> Self {
        Self(inputs.into_iter().fold(0, |set, input| set | input.bit()))
    }
}
impl From<HostInputs> for Vec<HostInput> {
    fn from(inputs: HostInputs) -> Self {
        inputs.iter().collect()
    }
}
impl TryFrom<Vec<HostInput>> for HostInputs {
    type Error = &'static str;

    fn try_from(inputs: Vec<HostInput>) -> Result<Self, Self::Error> {
        let allowlist: Self = inputs.iter().copied().collect();
        if allowlist.0.count_ones() as usize != inputs.len() {
            return Err("list each host input once");
        }
        Ok(allowlist)
    }
}

/// Older `syntax` key. Dort accepts any value, ignores it and leaves the line alone.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LegacySyntax;

impl<'de> Deserialize<'de> for LegacySyntax {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedFile {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(alias = "recipe", skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
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
    /// Older key. Dort accepts it and never writes it.
    #[serde(default, skip_serializing)]
    pub syntax: Option<LegacySyntax>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_inputs: Option<HostInputs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_inputs: Option<Vec<String>>,
    pub permissions: Option<String>,
}

pub(crate) enum FileContent<'a> {
    Source(&'a std::path::Path),
    Template(&'a str),
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
        match (self.source_for(platform), self.template.as_deref()) {
            (Some(path), None) => Some(FileContent::Source(path)),
            (None, Some(text)) if self.mode == FileMode::Template && self.overrides.is_empty() => {
                Some(FileContent::Template(text))
            }
            _ => None,
        }
    }
}
