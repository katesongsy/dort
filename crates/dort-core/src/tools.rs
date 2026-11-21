//! Pure tool-policy primitives and separately approved provider inspection.
//! Policy parsing/comparison never inspects or executes providers.
pub mod binaries;
pub mod declaration;
pub mod inspection;
pub mod install;
pub mod install_preview;
pub mod planning;
use crate::model::Provider;
use semver::Version;
use serde::{Deserialize, Serialize};

/// A provider-enforceable request, using the existing manifest version grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VersionPolicy {
    ProviderDefault {},
    /// Select the current release when installing a missing package, not an upgrade request.
    Latest {},
    Exact {
        version: String,
    },
}

/// Comparison only: this is neither proof of installation nor an execution approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionMatch {
    Satisfied,
    Mismatch,
    /// Missing, malformed or unsupported observations must not become upgrade/install guesses.
    Unverifiable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("provider cannot enforce this version policy")]
pub struct UnsupportedVersionPolicy;

impl VersionPolicy {
    pub fn parse(
        provider: Provider,
        requested: Option<&str>,
    ) -> Result<Self, UnsupportedVersionPolicy> {
        let Some(requested) = requested else {
            return Ok(Self::ProviderDefault {});
        };
        if requested == "latest" {
            return Ok(Self::Latest {});
        }
        if provider == Provider::Npm && Version::parse(requested).is_ok() {
            return Ok(Self::Exact {
                version: requested.into(),
            });
        }
        Err(UnsupportedVersionPolicy)
    }

    /// Compare provider-reported metadata without running a binary or consulting a registry.
    ///
    /// The caller must establish the provider/package/executable identity separately. In
    /// particular, a PATH binary or a successful comparison is not proof of Dort ownership.
    /// Exact requests include build metadata. Unverifiable observations are not mismatches.
    pub fn matches_version(&self, observed: Option<&str>) -> VersionMatch {
        let Some(observed) = observed.filter(|version| valid_observed_version(version)) else {
            return VersionMatch::Unverifiable;
        };
        match self {
            Self::ProviderDefault {} | Self::Latest {} => VersionMatch::Satisfied,
            Self::Exact { version } => match (Version::parse(version), Version::parse(observed)) {
                (Ok(requested), Ok(installed)) => comparison(requested == installed),
                _ => VersionMatch::Unverifiable,
            },
        }
    }
}

fn comparison(satisfied: bool) -> VersionMatch {
    if satisfied {
        VersionMatch::Satisfied
    } else {
        VersionMatch::Mismatch
    }
}

fn valid_observed_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 256
        && version.as_bytes()[0].is_ascii_alphanumeric()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".:+_~-".contains(&byte))
}
