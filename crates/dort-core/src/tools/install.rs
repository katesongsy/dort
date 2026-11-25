//! Homebrew and npm installs with reviewed previews.
use super::{VersionMatch, VersionPolicy, binaries, inspection, planning};
use crate::{
    Error, Manifest, Result, adoption,
    context::HostContext,
    journal,
    local::LocalConfig,
    model::Provider,
    observation::{self, Entry, Kind},
    state::WriterLock,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

mod attempts;
pub mod brew;
mod closure;
pub mod npm;
mod outcomes;
mod process;
pub use attempts::{Attempt, Problem, diagnose};
pub use closure::{CloseApproval, ClosePreview, CloseRequest, CloseReview, close, prepare_close};
pub use outcomes::{NextStep, Status, Summary};

pub const TIMEOUT_SECONDS: u64 = 600;
/// Bound on the metadata a plan records for one install.
///
/// Every program and directory entry carries its own chain of ancestor
/// fingerprints, so one npm tool with a backed-up runtime measures about 49 KB
/// from a nine-deep temporary HOME. Keep the bound well above that.
const METADATA_LIMIT: usize = 256 * 1024;

#[derive(Clone)]
pub struct Request {
    pub repository: PathBuf,
    pub profiles: Vec<String>,
    pub tool: String,
    pub executable: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub tool_search_dirs: Vec<PathBuf>,
    /// Explicit selected tools with independent catalog groups, never merged.
    pub bound_tools: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub platform: crate::platform::Platform,
    pub architecture: crate::platform::Architecture,
    pub repository: PathBuf,
    pub repository_id: String,
    pub profiles: BTreeSet<String>,
    pub tool: String,
    pub binary: String,
    pub package: String,
    pub version: String,
    pub executable: PathBuf,
    pub home: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub tool_search_dirs: Vec<PathBuf>,
    pub bound_tools: BTreeSet<String>,
    /// Present only for npm; other provider records omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm: Option<npm::Scope>,
    /// Schema 3 only. Provider-current selection is not an exact pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brew: Option<brew::Scope>,
}
#[derive(Serialize)]
pub struct Review<'a> {
    pub operation: &'static str,
    pub scope: &'a Scope,
    pub arguments: Vec<String>,
    /// The selected provider installer.
    pub installer: &'a std::path::Path,
    pub fresh_inspections: Vec<inspection::Description>,
    pub timeout_seconds: u64,
    pub warning: &'static str,
    pub fresh_probes: Vec<Vec<String>>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sources {
    manifest: Entry,
    local_config: Entry,
    executable: Entry,
    host_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    npm: Option<npm::Sources>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brew: Option<brew::Sources>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brew_probe: Option<brew::Probe>,
}
pub struct Preview {
    request: Request,
    scope: Scope,
    sources: Sources,
}
pub struct Approval(Preview);
impl Preview {
    /// Compare session provenance with the independently captured core manifest.
    pub fn manifest_matches(&self, entry: &Entry) -> bool {
        same_source(&self.sources.manifest, entry)
    }
    pub fn review(&self) -> Review<'_> {
        Review {
            operation: "Freshly inspect the displayed scopes, conditionally install this still-missing package, then inspect again. No intentional other top-level upgrade request.",
            scope: &self.scope,
            arguments: arguments(&self.scope),
            installer: &self.scope.executable,
            fresh_inspections: self
                .scope
                .brew
                .as_ref()
                .map(|b| &b.catalogs)
                .or_else(|| self.scope.npm.as_ref().map(|n| &n.catalogs))
                .expect("validated provider catalogs")
                .iter()
                .map(|c| {
                    c.preview(&self.scope.home)
                        .expect("validated catalog")
                        .description()
                        .clone()
                })
                .collect(),
            fresh_probes: self
                .scope
                .brew
                .as_ref()
                .map(|_| brew::probe_arguments(&self.scope.package))
                .unwrap_or_default(),
            timeout_seconds: TIMEOUT_SECONDS,
            warning: if self.scope.brew.is_some() {
                brew::WARNING
            } else {
                npm::WARNING
            },
        }
    }
    pub fn approve(self) -> Approval {
        Approval(self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutcome {
    Success,
    Failed,
    Unavailable,
    Pipe,
    Wait,
    Timeout,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageState {
    ExactPresent,
    ProviderCurrentPresent,
    Absent,
    Conflict,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryState {
    Missing,
    Candidates,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub process: ProcessOutcome,
    pub package: PackageState,
    pub binary: BinaryState,
}
pub struct Report {
    pub scope: Scope,
    pub attempt_id: Option<String>,
    pub no_op: bool,
    pub outcome: Option<Outcome>,
    pub observed_versions: Vec<String>,
    pub possible_partial_effects: bool,
    pub readiness_verified: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    FreshValidation,
    FreshInspection,
    IntentDurable,
    Installing,
    PostInspection,
    OutcomeDurable,
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}
/// Static messages stay the default; reviewed scope paths may be named exactly so
/// the user can fix them. Provider payloads never pass through this helper.
fn scope_invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn arguments(scope: &Scope) -> Vec<String> {
    if scope.brew.is_some() {
        brew::arguments(&scope.package)
    } else {
        npm::arguments(scope)
    }
}
fn same_source(a: &Entry, b: &Entry) -> bool {
    a.path == b.path
        && adoption::same(a, b, true)
        && a.parents.len() == b.parents.len()
        && a.parents.iter().zip(&b.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}
fn sources(
    host: &HostContext,
    request: &Request,
    npm: Option<&npm::Scope>,
    brew: Option<&brew::Scope>,
) -> Result<(Manifest, Scope, Sources)> {
    if [npm.is_some(), brew.is_some()]
        .into_iter()
        .filter(|p| *p)
        .count()
        != 1
    {
        return Err(invalid("installation requires one supported provider"));
    }
    let (local, local_config) = LocalConfig::load_bound(host)?;
    let repository_id = local
        .repositories
        .iter()
        .find(|(_, r)| r.root == request.repository)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| invalid("tool installation requires a registered repository"))?;
    let (manifest, entry) = observation::read_manifest(&request.repository.join("dort.toml"))
        .map_err(|_| {
            invalid("installation manifest unavailable, unsafe or invalid; contents withheld")
        })?;
    let rows = local.profile_rows(
        &manifest.profiles,
        host.platform,
        Some(&request.repository),
        &request.profiles,
    )?;
    let active = crate::profiles::active(&rows);
    let tool = manifest
        .tools
        .iter()
        .find(|t| t.id == request.tool && crate::profiles::selected(&t.profiles, &active))
        .ok_or_else(|| invalid("selected tool is not active in this scope"))?;
    let spec = &tool.install;
    let provider = if brew.is_some() {
        Provider::Brew
    } else {
        Provider::Npm
    };
    if spec.provider != provider {
        return Err(invalid("unsupported installation provider"));
    }
    let policy = VersionPolicy::parse(spec.provider, spec.version.as_deref())
        .map_err(|_| invalid("unsupported installation policy"))?;
    let current_policy = brew.map(|b| &b.policy);
    let version = match (&policy, current_policy) {
        (VersionPolicy::ProviderDefault {}, Some(p)) if p == &policy => {
            "provider_default".to_owned()
        }
        (VersionPolicy::Latest {}, Some(p)) if p == &policy => "latest".to_owned(),
        (VersionPolicy::Exact { version }, None) => version.clone(),
        _ => {
            return Err(invalid(
                "npm requires an exact version; brew requires provider-current policy",
            ));
        }
    };
    if request.bound_tools.is_empty()
        || request.bound_tools.len() > 256
        || !request.bound_tools.contains(&request.tool)
        || request.bound_tools.iter().any(|id| {
            !manifest
                .tools
                .iter()
                .any(|t| &t.id == id && crate::profiles::selected(&t.profiles, &active))
        })
    {
        return Err(invalid(
            "installation requires bounded explicit active tool bindings",
        ));
    }
    if let Some(npm) = npm {
        if spec.runtime_tool.as_ref() != Some(&npm.binding.runtime_tool)
            || request.executable != npm.binding.node
        {
            return Err(invalid(
                "npm installation runtime binding differs from declaration",
            ));
        }
        inspection::Preview::npm(
            &manifest,
            npm.binding.clone(),
            host.home.clone(),
            request.search_dirs.clone(),
        )
        .map_err(|_| invalid("invalid explicit npm installation binding"))?;
    } else if brew.is_some() {
        inspection::Preview::new(
            inspection::Query::BrewFormulae,
            request.executable.clone(),
            host.home.clone(),
            request.search_dirs.clone(),
        )
        .map_err(|_| invalid("invalid explicit Homebrew scope"))?;
    } else {
        return Err(invalid("unsupported installation provider"));
    }
    binaries::validate(&manifest, &active, &request.tool_search_dirs)?;
    let executable = observation::inspect(&request.executable, false)?;
    if executable.kind != Kind::Regular
        || executable
            .fingerprint
            .as_ref()
            .is_none_or(|f| f.mode & 0o111 == 0 || f.links != 1)
    {
        return Err(invalid(
            "installation needs a regular executable at the explicit canonical path",
        ));
    }
    let scope = Scope {
        platform: host.platform,
        architecture: host.architecture,
        repository: request.repository.clone(),
        repository_id,
        profiles: active,
        tool: tool.id.clone(),
        binary: tool.binary.clone(),
        package: spec.package.clone(),
        version,
        executable: request.executable.clone(),
        home: host.home.clone(),
        search_dirs: request.search_dirs.clone(),
        tool_search_dirs: request.tool_search_dirs.clone(),
        bound_tools: request.bound_tools.clone(),
        npm: npm.cloned(),
        brew: brew.cloned(),
    };
    if !attempts::valid_scope(&scope) {
        return Err(invalid("installation scope exceeds supported bounds"));
    }
    let sources = Sources {
        manifest: entry,
        local_config,
        executable,
        host_sha256: journal::digest(journal::encode(host)?.as_bytes()),
        brew_probe: None,
        brew: if scope.brew.is_some() {
            Some(brew::sources(&scope)?)
        } else {
            None
        },
        npm: if scope.npm.is_some() {
            Some(npm::sources(&scope)?)
        } else {
            None
        },
    };
    Ok((manifest, scope, sources))
}
fn bounded_metadata(scope: &Scope, sources: &Sources) -> Result<()> {
    let size = journal::encode(&(scope, sources))?.len();
    if size > METADATA_LIMIT {
        return Err(scope_invalid(format!(
            "installation metadata is {size} bytes, over the {METADATA_LIMIT} byte limit"
        )));
    }
    Ok(())
}
fn evaluate_catalogs(
    host: &HostContext,
    manifest: &Manifest,
    scope: &Scope,
    reports: &[&inspection::Report],
) -> Result<planning::Row> {
    let binary_report = binaries::observe(manifest, &scope.profiles, &scope.tool_search_dirs)?;
    let catalogs = if let Some(brew) = &scope.brew {
        brew::catalogs(scope, brew, reports)?
    } else if let Some(npm) = &scope.npm {
        npm::catalogs(scope, npm, reports)?
    } else {
        return Err(invalid("installation requires supported provider catalogs"));
    };
    let observed = planning::Observations {
        catalogs,
        binaries: binary_report
            .tools
            .into_iter()
            .map(|(id, t)| (id, t.observation))
            .collect(),
    };
    planning::plan(manifest, host.platform, &scope.profiles, &observed)?
        .rows
        .into_iter()
        .find(|r| r.id == scope.tool)
        .ok_or_else(|| invalid("selected installation row disappeared"))
}
fn reviewable(row: &planning::Row, environment: bool) -> bool {
    matches!(row.package, planning::PackageFinding::Absent)
        && row.blockers.iter().all(|blocker| {
            environment
                && matches!(
                    blocker,
                    planning::Blocker::DependencyNeedsReview { .. }
                        | planning::Blocker::DependencyBlocked { .. }
                )
        })
}

pub(crate) fn preview_binding(preview: &Preview) -> Result<String> {
    let sources = &preview.sources;
    let brew = sources.brew.as_ref().map(brew::effect_sources);
    let npm = sources.npm.as_ref().map(npm::effect_sources);
    Ok(journal::digest(
        adoption::canonical(&(
            &preview.scope,
            sources.manifest.clone().stable_parents(),
            sources.local_config.clone().stable_parents(),
            sources.executable.clone().stable_parents(),
            &sources.host_sha256,
            brew,
            npm,
        ))?
        .as_bytes(),
    ))
}

fn row_versions(row: &planning::Row) -> Vec<String> {
    match &row.package {
        planning::PackageFinding::Present { versions } => {
            versions.iter().map(|v| v.version.clone()).collect()
        }
        _ => Vec::new(),
    }
}
fn observe(
    host: &HostContext,
    manifest: &Manifest,
    scope: &Scope,
    reports: &[&inspection::Report],
) -> (PackageState, BinaryState, Vec<String>) {
    match evaluate_catalogs(host, manifest, scope, reports) {
        Ok(row) => {
            let versions = if scope.brew.is_some() {
                row_versions(&row)
            } else {
                Vec::new()
            };
            let package = match row.package {
                planning::PackageFinding::Present { versions }
                    if versions
                        .iter()
                        .any(|v| v.policy_match == VersionMatch::Satisfied) =>
                {
                    if scope.brew.is_some() {
                        PackageState::ProviderCurrentPresent
                    } else {
                        PackageState::ExactPresent
                    }
                }
                planning::PackageFinding::Absent => PackageState::Absent,
                planning::PackageFinding::Unavailable { .. }
                | planning::PackageFinding::NotObserved => PackageState::Unavailable,
                _ => PackageState::Conflict,
            };
            let binary = match row.binary_observation {
                planning::BinaryObservation::Missing => BinaryState::Missing,
                planning::BinaryObservation::Candidates { .. } => BinaryState::Candidates,
                _ => BinaryState::Unavailable,
            };
            (package, binary, versions)
        }
        Err(_) => (
            PackageState::Unavailable,
            BinaryState::Unavailable,
            Vec::new(),
        ),
    }
}
fn collect(host: &HostContext, old: &Preview) -> Result<Vec<inspection::Report>> {
    let (manifest, scope, _) = revalidate(host, old)?;
    if let Some(catalogs) = scope
        .brew
        .as_ref()
        .map(|b| &b.catalogs)
        .or_else(|| scope.npm.as_ref().map(|n| &n.catalogs))
    {
        catalogs
            .iter()
            .map(|c| {
                revalidate(host, old)?;
                if let Some(b) = &c.npm_binding {
                    inspection::Preview::npm(
                        &manifest,
                        b.clone(),
                        scope.home.clone(),
                        c.search_dirs.clone(),
                    )
                    .map_err(|_| invalid("installation catalog runtime no longer declared"))?;
                }
                let report = c.preview(&scope.home)?.approve().inspect();
                revalidate(host, old)?;
                Ok(report)
            })
            .collect()
    } else {
        Err(invalid("installation requires supported provider catalogs"))
    }
}
fn revalidate(host: &HostContext, old: &Preview) -> Result<(Manifest, Scope, Sources)> {
    let (manifest, scope, fresh) = sources(
        host,
        &old.request,
        old.scope.npm.as_ref(),
        old.scope.brew.as_ref(),
    )?;
    if journal::encode(&scope)? != journal::encode(&old.scope)?
        || fresh.host_sha256 != old.sources.host_sha256
        || !same_source(&fresh.manifest, &old.sources.manifest)
        || !same_source(&fresh.local_config, &old.sources.local_config)
        || !same_source(&fresh.executable, &old.sources.executable)
        || match (&fresh.brew, &old.sources.brew) {
            (Some(a), Some(b)) => !brew::same(a, b),
            (None, None) => false,
            _ => true,
        }
        || match (&fresh.npm, &old.sources.npm) {
            (Some(a), Some(b)) => !npm::same(a, b),
            (None, None) => false,
            _ => true,
        }
    {
        return Err(invalid(
            "installation scope changed; request a fresh review",
        ));
    }
    Ok((manifest, scope, fresh))
}
/// Call only under the application writer lock after consuming a real preview.
pub fn execute(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    mut observer: impl FnMut(Event),
) -> Result<Report> {
    lock.check_context(host)?;
    journal::ensure_writable(host)?;
    attempts::ensure_clear(host)?;
    let mut old = approval.0;
    observer(Event::FreshValidation);
    let (manifest, scope, mut fresh) = revalidate(host, &old)?;
    observer(Event::FreshInspection);
    let brew_probe = if scope.brew.is_some() {
        Some(brew::probe(&scope, || revalidate(host, &old).map(|_| ()))?)
    } else {
        None
    };
    let before = collect(host, &old)?;
    let before_refs = before.iter().collect::<Vec<_>>();
    let row = evaluate_catalogs(host, &manifest, &scope, &before_refs)?;
    revalidate(host, &old)?;
    if let Some(probe) = &brew_probe
        && row_versions(&row) != probe.installed_versions
    {
        return Err(invalid(
            "Homebrew target and installed catalog observations disagree; inspect again",
        ));
    }
    if row.action == planning::PackageAction::NoChange {
        let observed_versions = brew_probe
            .as_ref()
            .map(|p| p.installed_versions.clone())
            .unwrap_or_default();
        return Ok(Report {
            scope,
            attempt_id: None,
            no_op: true,
            outcome: None,
            observed_versions,
            possible_partial_effects: false,
            readiness_verified: false,
        });
    }
    if row.action != planning::PackageAction::ReviewMissingPackage || !row.blockers.is_empty() {
        return Err(invalid(
            "fresh inspection refused installation; inspect current scope and review again",
        ));
    }
    fresh.brew_probe = brew_probe;
    bounded_metadata(&scope, &fresh)?;
    let attempt = attempts::begin(host, lock, scope.clone(), fresh)?;
    observer(Event::IntentDurable);
    // Fail closed if source bindings changed while intent was made durable.
    revalidate(host, &old)?;
    revalidate(host, &old)?;
    journal::ensure_writable(host)?;
    attempts::verify_pending(host, &attempt)?;
    let ready = evaluate_catalogs(host, &manifest, &scope, &before_refs)?;
    if ready.action != planning::PackageAction::ReviewMissingPackage || !ready.blockers.is_empty() {
        return Err(invalid(
            "binary or prerequisite scope changed before installation; inspect and close the pending attempt",
        ));
    }
    if let Some(bound) = &old.sources.npm {
        old.sources.npm = Some(npm::create_directories(&scope, bound)?);
        revalidate(host, &old)?;
        journal::ensure_writable(host)?;
        attempts::verify_pending(host, &attempt)?;
    }
    observer(Event::Installing);
    revalidate(host, &old)?;
    journal::ensure_writable(host)?;
    attempts::verify_pending(host, &attempt)?;
    let ready = evaluate_catalogs(host, &manifest, &scope, &before_refs)?;
    if ready.action != planning::PackageAction::ReviewMissingPackage || !ready.blockers.is_empty() {
        return Err(invalid(
            "binary or prerequisite scope changed before spawn; inspect the pending attempt",
        ));
    }
    let process = process::run(&scope, std::time::Duration::from_secs(TIMEOUT_SECONDS));
    observer(Event::PostInspection);
    let (mut package, mut binary, mut observed_versions) = if revalidate(host, &old).is_ok() {
        match collect(host, &old) {
            Ok(after) => observe(host, &manifest, &scope, &after.iter().collect::<Vec<_>>()),
            Err(_) => (
                PackageState::Unavailable,
                BinaryState::Unavailable,
                Vec::new(),
            ),
        }
    } else {
        // Do not run a provider whose binding changed during installation.
        (
            PackageState::Unavailable,
            BinaryState::Unavailable,
            Vec::new(),
        )
    };
    if revalidate(host, &old).is_err() {
        package = PackageState::Unavailable;
        binary = BinaryState::Unavailable;
        observed_versions.clear();
    }
    if scope.brew.is_some() && revalidate(host, &old).is_ok() {
        match brew::formula_info(&scope) {
            Ok((_, versions)) if revalidate(host, &old).is_ok() => {
                let expected = if versions.is_empty() {
                    PackageState::Absent
                } else {
                    PackageState::ProviderCurrentPresent
                };
                if package != expected || observed_versions != versions {
                    package = PackageState::Unavailable;
                    observed_versions.clear();
                }
            }
            _ => {
                package = PackageState::Unavailable;
                binary = BinaryState::Unavailable;
                observed_versions.clear();
            }
        }
    }
    let outcome = Outcome {
        process,
        package,
        binary,
    };
    attempts::finish(host, lock, &attempt, outcome, observed_versions.clone()).map_err(|_| crate::Error::Invalid(format!(
        "Tools attempt {} has no durable outcome; inspect and close it before retrying (possible partial effects)", attempt.id
    )))?;
    observer(Event::OutcomeDurable);
    Ok(Report {
        scope,
        attempt_id: Some(attempt.id),
        no_op: false,
        outcome: Some(outcome),
        observed_versions,
        possible_partial_effects: process != ProcessOutcome::Success
            || !matches!(
                package,
                PackageState::ExactPresent | PackageState::ProviderCurrentPresent
            ),
        readiness_verified: false,
    })
}
