use crate::{Error, Result, model::base_profiles, platform::Platform};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    #[serde(default)]
    pub available: Vec<String>,
    #[serde(default = "base_profiles")]
    pub default: Vec<String>,
    #[serde(default)]
    pub platform: BTreeMap<Platform, Vec<String>>,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            available: vec![],
            default: base_profiles(),
            platform: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    #[default]
    Defaults,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Base,
    Temporary,
    Saved,
    Legacy,
    Platform,
    RepositoryDefault,
    NotDefault,
}

impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "always on",
            Self::Temporary => "temporary for this run",
            Self::Saved => "saved choice",
            Self::Legacy => "legacy local setting",
            Self::Platform => "platform default",
            Self::RepositoryDefault => "repository default",
            Self::NotDefault => "not a default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub name: String,
    pub on: bool,
    pub reason: Reason,
    pub choice: Choice,
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Defaults => "Use defaults",
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

impl Row {
    pub fn label(&self) -> String {
        format!(
            "{}: {} ({})",
            self.name,
            if self.on { "On" } else { "Off" },
            self.reason.label()
        )
    }
}

pub fn active(rows: &[Row]) -> BTreeSet<String> {
    rows.iter()
        .filter(|row| row.on)
        .map(|row| row.name.clone())
        .collect()
}

impl Profiles {
    pub fn known(&self) -> BTreeSet<String> {
        self.available
            .iter()
            .cloned()
            .chain(["base".into()])
            .collect()
    }

    pub fn resolve(&self, platform: Platform, local: &[String]) -> Result<BTreeSet<String>> {
        Ok(active(&self.rows(
            platform,
            local,
            &BTreeMap::new(),
            &[],
        )?))
    }

    pub fn rows(
        &self,
        platform: Platform,
        legacy: &[String],
        saved: &BTreeMap<String, Choice>,
        temporary: &[String],
    ) -> Result<Vec<Row>> {
        let known = self.known();
        let platform_defaults = self.platform.get(&platform);
        let unknown: BTreeSet<_> = self
            .default
            .iter()
            .chain(platform_defaults.into_iter().flatten())
            .chain(legacy)
            .chain(temporary)
            .filter(|name| !known.contains(*name))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(Error::Invalid(format!(
                "unknown active profiles: {}",
                unknown.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }
        Ok(known
            .into_iter()
            .map(|name| {
                let choice = saved.get(&name).copied().unwrap_or_default();
                let (on, reason) = if name == "base" {
                    (true, Reason::Base)
                } else if temporary.contains(&name) {
                    (true, Reason::Temporary)
                } else if choice != Choice::Defaults {
                    (choice == Choice::On, Reason::Saved)
                } else if !saved.contains_key(&name) && legacy.contains(&name) {
                    (true, Reason::Legacy)
                } else if platform_defaults.is_some_and(|names| names.contains(&name)) {
                    (true, Reason::Platform)
                } else if self.default.contains(&name) {
                    (true, Reason::RepositoryDefault)
                } else {
                    (false, Reason::NotDefault)
                };
                Row {
                    name,
                    on,
                    reason,
                    choice,
                }
            })
            .collect())
    }
}

pub fn selected(profiles: &[String], active: &BTreeSet<String>) -> bool {
    profiles.iter().any(|p| active.contains(p))
}
