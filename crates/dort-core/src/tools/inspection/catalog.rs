//! Package-manager output parsers.
use super::{Failure, Installation, LIMIT, Query};
use serde::Deserialize;
use std::collections::BTreeMap;

const MAX_RECORDS: usize = 32_768;
fn token(s: &str, limit: usize) -> bool {
    !s.is_empty()
        && s.len() <= limit
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-:@/~".contains(&b))
        && s.as_bytes()[0].is_ascii_alphanumeric()
}
fn installation(package: &str, version: &str) -> Result<Installation, Failure> {
    let package_token = package.strip_prefix('@').unwrap_or(package);
    if !token(package_token, 255) || package.len() > 256 || !token(version, 256) {
        return Err(Failure::Malformed);
    }
    Ok(Installation {
        package: package.into(),
        version: version.into(),
    })
}
#[derive(Deserialize)]
struct Brew {
    formulae: Vec<Formula>,
}
#[derive(Deserialize)]
struct Formula {
    full_name: String,
    installed: Vec<BrewVersion>,
}
#[derive(Deserialize)]
struct BrewVersion {
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Npm {
    #[serde(default)]
    dependencies: BTreeMap<String, NpmVersion>,
    #[serde(default)]
    problems: Vec<serde::de::IgnoredAny>,
    invalid: Option<serde::de::IgnoredAny>,
    error: Option<serde::de::IgnoredAny>,
    #[serde(rename = "name")]
    _name: Option<serde::de::IgnoredAny>,
    #[serde(rename = "version")]
    _version: Option<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
struct NpmVersion {
    version: String,
    #[serde(default)]
    missing: bool,
    invalid: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    problems: Vec<serde::de::IgnoredAny>,
}
pub(super) fn parse(query: Query, bytes: &[u8]) -> Result<Vec<Installation>, Failure> {
    if bytes.len() > LIMIT {
        return Err(Failure::OutputLimit);
    }
    let mut entries = Vec::new();
    match query {
        Query::BrewFormulae => {
            let report: Brew = serde_json::from_slice(bytes).map_err(|_| Failure::Malformed)?;
            if report.formulae.len() > MAX_RECORDS {
                return Err(Failure::Malformed);
            }
            for formula in report.formulae {
                if !token(&formula.full_name, 256) || formula.installed.is_empty() {
                    return Err(Failure::Malformed);
                }
                for version in formula.installed {
                    push(&mut entries, &formula.full_name, &version.version)?;
                }
            }
        }
        Query::NpmGlobals => {
            let report: Npm = serde_json::from_slice(bytes).map_err(|_| Failure::Malformed)?;
            if !report.problems.is_empty() || report.invalid.is_some() || report.error.is_some() {
                return Err(Failure::IncompletePackages);
            }
            for (package, version) in report.dependencies {
                if version.missing || version.invalid.is_some() || !version.problems.is_empty() {
                    return Err(Failure::IncompletePackages);
                }
                // Global top-level packages only. Never retain resolved URLs, scripts,
                // transitive trees or provider diagnostic strings.
                push(&mut entries, &package, &version.version)?;
            }
        }
    }
    entries.sort_by(|a, b| (&a.package, &a.version).cmp(&(&b.package, &b.version)));
    // Duplicate identical observations do not turn into multiple installations.
    entries.dedup();
    Ok(entries)
}
fn push(entries: &mut Vec<Installation>, package: &str, version: &str) -> Result<(), Failure> {
    if entries.len() >= MAX_RECORDS {
        return Err(Failure::Malformed);
    }
    entries.push(installation(package, version)?);
    Ok(())
}
