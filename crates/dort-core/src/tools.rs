//! Pure tool-policy primitives and separately approved provider inspection.
//! Policy parsing/comparison never inspects or executes providers.
pub mod binaries;
pub mod inspection;
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
    /// A mise major or major.minor prefix, not a SemVer caret requirement.
    MisePrefix {
        major: String,
        minor: Option<String>,
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
        if matches!(provider, Provider::Mise | Provider::Npm) && Version::parse(requested).is_ok() {
            return Ok(Self::Exact {
                version: requested.into(),
            });
        }
        if provider == Provider::Mise {
            let mut components = requested.split('.');
            let major = components.next().unwrap_or_default();
            let minor = components.next();
            if components.next().is_none()
                && [Some(major), minor]
                    .into_iter()
                    .flatten()
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            {
                return Ok(Self::MisePrefix {
                    major: major.into(),
                    minor: minor.map(str::to_owned),
                });
            }
        }
        Err(UnsupportedVersionPolicy)
    }

    /// Compare provider-reported metadata without running a binary or consulting a registry.
    ///
    /// The caller must establish the provider/package/executable identity separately. In
    /// particular, a PATH binary or a successful comparison is not proof of Dort ownership.
    /// Exact requests include build metadata. Numeric prefixes exclude prereleases unless
    /// requested as an exact version. Unverifiable observations are not version mismatches.
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
            Self::MisePrefix { major, minor } => {
                let Some(major) = canonical_component(major) else {
                    return VersionMatch::Unverifiable;
                };
                let minor = match minor.as_deref() {
                    Some(value) => match canonical_component(value) {
                        Some(value) => Some(value),
                        None => return VersionMatch::Unverifiable,
                    },
                    None => None,
                };
                let Ok(installed) = Version::parse(observed) else {
                    return VersionMatch::Unverifiable;
                };
                comparison(
                    installed.pre.is_empty()
                        && installed.major == major
                        && minor.is_none_or(|minor| installed.minor == minor),
                )
            }
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

fn canonical_component(value: &str) -> Option<u64> {
    // Keep legacy manifest acceptance, but never guess at overflow/zero-padded selectors.
    value
        .parse::<u64>()
        .ok()
        .filter(|number| number.to_string() == value)
}

fn valid_observed_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 256
        && version.as_bytes()[0].is_ascii_alphanumeric()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".:+_~-".contains(&byte))
}
