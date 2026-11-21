//! Pure, conservative package-claim classification. No I/O or execution authority.
//! Package policy matches never establish binary identity, activation or ownership.
use super::{VersionMatch, VersionPolicy, inspection};
use crate::{
    Error, Manifest, Result,
    model::{InstallSpec, Provider},
    platform::Platform,
    profiles::selected,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub const MAX_SELECTED_TOOLS: usize = 4096;
pub const MAX_VERSION_CLAIMS: usize = 65_536;
const MAX_CATALOG_RECORDS: usize = 32_768;

/// Metadata observations supplied explicitly by the application. Candidates are
/// unattributed: a path/name alone never establishes a provider or version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BinaryObservation {
    #[default]
    NotObserved,
    Unavailable,
    Missing,
    Candidates {
        paths: Vec<PathBuf>,
    },
}
/// Each selected tool is explicitly bound to one chosen provider query scope.
/// No implicit merging of providers, homes, executables or npm prefixes occurs.
#[derive(Default)]
pub struct Observations<'a> {
    pub catalogs: BTreeMap<String, &'a inspection::Report>,
    pub binaries: BTreeMap<String, BinaryObservation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeIssue {
    WrongProvider,
    InvalidRuntimeBinding,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionClaim {
    pub version: String,
    pub policy_match: VersionMatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PackageFinding {
    NotObserved,
    Unavailable {
        reason: inspection::Failure,
    },
    ScopeMismatch {
        reason: ScopeIssue,
    },
    /// A related qualified/tapped/backend name is not proof of the requested identity.
    AmbiguousIdentity,
    Absent,
    /// Unsafe metadata is withheld, never copied into a plan/report.
    InvalidMetadata,
    Present {
        versions: Vec<VersionClaim>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Blocker {
    ProviderNotObserved,
    ProviderUnavailable,
    ProviderScopeMismatch,
    AmbiguousPackageIdentity,
    InvalidProviderMetadata,
    VersionMismatch,
    VersionUnverifiable,
    BinaryNotObserved,
    BinaryObservationUnavailable,
    UnattributedBinaryCandidates,
    RuntimeBindingUnverified,
    DependencyInactive {
        tool: String,
    },
    DependencyBlocked {
        tool: String,
    },
    /// A dependency needs its own review and subsequent fresh observation first.
    DependencyNeedsReview {
        tool: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageAction {
    /// The provider claims a matching installed version. Not overall tool readiness.
    NoChange,
    /// A discussion/planning proposal only; no arguments, approval or executor.
    ReviewMissingPackage,
    Blocked,
}
#[derive(Debug, Serialize)]
pub struct Declaration {
    pub provider: Provider,
    pub package: String,
    pub policy: VersionPolicy,
    pub runtime_tool: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct CatalogScope {
    pub query: inspection::Query,
    pub executable: PathBuf,
    pub home: PathBuf,
    pub npm_binding: Option<inspection::NpmBinding>,
}
#[derive(Debug, Serialize)]
pub struct Row {
    pub id: String,
    pub binary: String,
    pub dependencies: Vec<String>,
    pub declaration: Option<Declaration>,
    pub catalog_scope: Option<CatalogScope>,
    pub package: PackageFinding,
    pub binary_observation: BinaryObservation,
    pub action: PackageAction,
    pub blockers: Vec<Blocker>,
    pub install_preview: super::install_preview::Preview,
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub platform: Platform,
    pub active_profiles: BTreeSet<String>,
    /// Deterministic dependency-first order, with ID order for independent nodes.
    pub rows: Vec<Row>,
}
impl Plan {
    pub fn has_blockers(&self) -> bool {
        self.rows.iter().any(|row| !row.blockers.is_empty())
    }
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}

/// Classify supplied facts only. Missing observations are not evidence of absence.
/// This intentionally does not call validate_for_host (which also checks file targets),
/// discover executables, request versions or resolve remote latest. Install command
/// descriptions are metadata only, never executable capabilities.
/// A serialized result cannot authorize any process, package or manifest write.
pub fn plan(
    manifest: &Manifest,
    platform: Platform,
    active: &BTreeSet<String>,
    observed: &Observations<'_>,
) -> Result<Plan> {
    manifest.validate()?;
    if !active.contains("base") || !active.is_subset(&manifest.profiles.known()) {
        return Err(invalid(
            "tool planning requires resolved, known profiles including base",
        ));
    }
    let tools: BTreeMap<_, _> = manifest
        .tools
        .iter()
        .filter(|t| selected(&t.profiles, active))
        .map(|t| (t.id.clone(), t))
        .collect();
    if tools.len() > MAX_SELECTED_TOOLS {
        return Err(invalid("tool planning selected-tool limit exceeded"));
    }
    if observed
        .catalogs
        .keys()
        .chain(observed.binaries.keys())
        .any(|id| !tools.contains_key(id))
    {
        return Err(invalid(
            "tool observation must refer to a selected manifest tool",
        ));
    }
    validate_observations(observed)?;
    let order = manifest.order(platform, &tools.keys().cloned().collect())?;
    let mut rows: Vec<Row> = vec![];
    let mut claim_count = 0usize;
    for id in order {
        let tool = tools[&id];
        let deps: Vec<_> = manifest
            .dependencies(tool, platform)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let binary = observed.binaries.get(&id).cloned().unwrap_or_default();
        let report = observed.catalogs.get(&id).copied();
        let spec = &tool.install;
        let declaration = Some(Declaration {
            provider: spec.provider,
            package: spec.package.clone(),
            policy: VersionPolicy::parse(spec.provider, spec.version.as_deref())
                .map_err(|_| invalid("unsupported tool version policy"))?,
            runtime_tool: spec.runtime_tool.clone(),
        });
        let package = classify(
            spec,
            &declaration.as_ref().expect("declared tool").policy,
            report,
        );
        if let PackageFinding::Present { versions } = &package {
            claim_count += versions.len();
            if claim_count > MAX_VERSION_CLAIMS {
                return Err(invalid("tool planning version-claim limit exceeded"));
            }
        }
        let mut blockers = package_blockers(&package);
        if declaration
            .as_ref()
            .is_some_and(|d| !comparable_policy(&d.policy))
            && !blockers.contains(&Blocker::VersionUnverifiable)
        {
            blockers.push(Blocker::VersionUnverifiable);
        }
        let satisfied = matches!(&package, PackageFinding::Present { versions } if versions.iter().any(|v| v.policy_match == VersionMatch::Satisfied));
        if matches!(package, PackageFinding::Absent) {
            match &binary {
                BinaryObservation::NotObserved => blockers.push(Blocker::BinaryNotObserved),
                BinaryObservation::Unavailable => {
                    blockers.push(Blocker::BinaryObservationUnavailable)
                }
                BinaryObservation::Candidates { .. } => {
                    blockers.push(Blocker::UnattributedBinaryCandidates)
                }
                BinaryObservation::Missing => (),
            }
            if spec.provider == Provider::Npm && !runtime_candidate_matches(spec, report, observed)
            {
                blockers.push(Blocker::RuntimeBindingUnverified);
            }
        }
        for dependency in &deps {
            if !tools.contains_key(dependency) {
                blockers.push(Blocker::DependencyInactive {
                    tool: dependency.clone(),
                });
            } else if let Some(row) = rows.iter().find(|row| &row.id == dependency) {
                if !row.blockers.is_empty() {
                    blockers.push(Blocker::DependencyBlocked {
                        tool: dependency.clone(),
                    });
                } else if row.action == PackageAction::ReviewMissingPackage {
                    blockers.push(Blocker::DependencyNeedsReview {
                        tool: dependency.clone(),
                    });
                }
            }
        }
        let action = if satisfied {
            PackageAction::NoChange
        } else if matches!(package, PackageFinding::Absent) && blockers.is_empty() {
            PackageAction::ReviewMissingPackage
        } else {
            PackageAction::Blocked
        };
        rows.push(Row {
            id: id.clone(),
            binary: tool.binary.clone(),
            dependencies: deps,
            declaration,
            catalog_scope: report.map(|r| CatalogScope {
                query: r.description.query,
                executable: r.description.executable.clone(),
                home: r.description.home.clone(),
                npm_binding: r.description.npm_binding.clone(),
            }),
            package,
            binary_observation: binary,
            action,
            blockers,
            install_preview: Default::default(),
        });
    }
    let mut preview_bytes = 0usize;
    for row in &mut rows {
        let paths = observed
            .catalogs
            .get(&row.id)
            .map_or(&[][..], |r| r.description.trusted_search_path.as_slice());
        row.install_preview = super::install_preview::describe(row, paths, &mut preview_bytes);
    }
    Ok(Plan {
        platform,
        active_profiles: active.clone(),
        rows,
    })
}

fn validate_observations(observed: &Observations<'_>) -> Result<()> {
    let valid = |path: &PathBuf| inspection::absolute(path);
    for binary in observed.binaries.values() {
        if let BinaryObservation::Candidates { paths } = binary
            && (paths.is_empty() || paths.len() > 1024 || !paths.iter().all(valid))
        {
            return Err(invalid(
                "binary candidates require a bounded nonempty list of absolute UTF-8 paths without dot components",
            ));
        }
    }
    for report in observed.catalogs.values() {
        if let inspection::Observation::Complete { installations } = &report.observation
            && installations.len() > MAX_CATALOG_RECORDS
        {
            return Err(invalid("catalog record limit exceeded"));
        }
        let d = &report.description;
        if !valid(&d.executable)
            || !valid(&d.home)
            || d.trusted_search_path.is_empty()
            || !d.trusted_search_path.iter().all(valid)
            || std::env::join_paths(&d.trusted_search_path).is_err()
            || d.npm_binding
                .as_ref()
                .is_some_and(|b| !valid(&b.node) || !valid(&b.npm_cli) || !valid(&b.prefix))
        {
            return Err(invalid(
                "catalog scope requires explicit absolute UTF-8 paths without dot components",
            ));
        }
    }
    Ok(())
}
fn classify(
    spec: &InstallSpec,
    policy: &VersionPolicy,
    report: Option<&inspection::Report>,
) -> PackageFinding {
    let Some(report) = report else {
        return PackageFinding::NotObserved;
    };
    let d = &report.description;
    if d.query.provider() != spec.provider {
        return PackageFinding::ScopeMismatch {
            reason: ScopeIssue::WrongProvider,
        };
    }
    if (spec.provider == Provider::Npm
        && d.npm_binding.as_ref().is_none_or(|b| {
            Some(b.runtime_tool.as_str()) != spec.runtime_tool.as_deref() || b.node != d.executable
        }))
        || (spec.provider != Provider::Npm && d.npm_binding.is_some())
    {
        return PackageFinding::ScopeMismatch {
            reason: ScopeIssue::InvalidRuntimeBinding,
        };
    }
    let installations = match &report.observation {
        inspection::Observation::Unavailable { reason } => {
            return PackageFinding::Unavailable { reason: *reason };
        }
        inspection::Observation::Complete { installations } => installations,
    };
    let mut versions: BTreeSet<String> = BTreeSet::new();
    let mut related = false;
    for entry in installations {
        if entry.package == spec.package {
            if !super::valid_observed_version(&entry.version) {
                return PackageFinding::InvalidMetadata;
            }
            versions.insert(entry.version.clone());
        } else if related_identity(spec.provider, &spec.package, &entry.package) {
            related = true;
        }
    }
    if versions.is_empty() {
        if related {
            PackageFinding::AmbiguousIdentity
        } else {
            PackageFinding::Absent
        }
    } else {
        PackageFinding::Present {
            versions: versions
                .into_iter()
                .map(|version| VersionClaim {
                    policy_match: policy.matches_version(Some(&version)),
                    version,
                })
                .collect(),
        }
    }
}
fn related_identity(provider: Provider, requested: &str, reported: &str) -> bool {
    match provider {
        Provider::Brew => requested.rsplit('/').next() == reported.rsplit('/').next(),
        Provider::Npm => false,
    }
}
fn comparable_policy(policy: &VersionPolicy) -> bool {
    match policy {
        VersionPolicy::ProviderDefault {} | VersionPolicy::Latest {} => true,
        VersionPolicy::Exact { version } => super::valid_observed_version(version),
    }
}
fn package_blockers(package: &PackageFinding) -> Vec<Blocker> {
    use PackageFinding::*;
    match package {
        NotObserved => vec![Blocker::ProviderNotObserved],
        Unavailable { .. } => vec![Blocker::ProviderUnavailable],
        ScopeMismatch { .. } => vec![Blocker::ProviderScopeMismatch],
        AmbiguousIdentity => vec![Blocker::AmbiguousPackageIdentity],
        InvalidMetadata => vec![Blocker::InvalidProviderMetadata],
        Absent => vec![],
        Present { versions }
            if versions
                .iter()
                .any(|v| v.policy_match == VersionMatch::Satisfied) =>
        {
            vec![]
        }
        Present { versions }
            if versions
                .iter()
                .any(|v| v.policy_match == VersionMatch::Unverifiable) =>
        {
            vec![Blocker::VersionUnverifiable]
        }
        Present { .. } => vec![Blocker::VersionMismatch],
    }
}
fn runtime_candidate_matches(
    spec: &InstallSpec,
    report: Option<&inspection::Report>,
    observed: &Observations<'_>,
) -> bool {
    let Some(runtime) = spec.runtime_tool.as_ref() else {
        return false;
    };
    let Some(binding) = report.and_then(|r| r.description.npm_binding.as_ref()) else {
        return false;
    };
    // Exactly one explicitly observed candidate must be the selected Node. This is
    // a necessary scope check for a proposal, not ownership/version verification.
    matches!(observed.binaries.get(runtime), Some(BinaryObservation::Candidates { paths }) if paths.len() == 1 && paths[0] == binding.node)
}
