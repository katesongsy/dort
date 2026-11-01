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

impl Profiles {
    pub fn known(&self) -> BTreeSet<String> {
        self.available
            .iter()
            .cloned()
            .chain(["base".into()])
            .collect()
    }

    pub fn resolve(&self, platform: Platform, local: &[String]) -> Result<BTreeSet<String>> {
        let active: BTreeSet<_> = ["base".to_owned()]
            .into_iter()
            .chain(self.default.iter().cloned())
            .chain(self.platform.get(&platform).into_iter().flatten().cloned())
            .chain(local.iter().cloned())
            .collect();
        let unknown: Vec<_> = active.difference(&self.known()).cloned().collect();
        if !unknown.is_empty() {
            return Err(Error::Invalid(format!(
                "unknown active profiles: {}",
                unknown.join(", ")
            )));
        }
        Ok(active)
    }
}

pub fn selected(profiles: &[String], active: &BTreeSet<String>) -> bool {
    profiles.iter().any(|p| active.contains(p))
}
