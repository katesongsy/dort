//! Explicit exact mise/npm installs. Never executes command-preview DTOs.
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
mod closure;
pub mod npm;
mod process;
pub use attempts::{Attempt, Problem, diagnose};
pub use closure::{CloseApproval, ClosePreview, CloseRequest, CloseReview, close, prepare_close};

pub const TIMEOUT_SECONDS: u64 = 600;
pub const WARNING: &str = "Installs one exact mise version without activation. Provider/backend/package code and downloads can change data and caches. Not a sandbox or an atomic operation. A 10-minute deadline may stop a partially completed install; no rollback is performed.";

#[derive(Clone)]
pub struct Request {
    pub repository: PathBuf,
    pub profiles: Vec<String>,
    pub tool: String,
    pub executable: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub tool_search_dirs: Vec<PathBuf>,
    /// Explicit selected tools. Mise binds these to ONE snapshot; npm supplies
    /// separate explicit catalog groups. Never inferred or merged.
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
    /// Absent in schema-1 mise records; never filled by migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm: Option<npm::Scope>,
}
#[derive(Serialize)]
pub struct Review<'a> {
    pub operation: &'static str,
    pub scope: &'a Scope,
    pub arguments: Vec<String>,
    pub fresh_inspections: Vec<inspection::Description>,
    pub timeout_seconds: u64,
    pub warning: &'static str,
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
            operation: "Freshly inspect the displayed catalogs, install only this still-missing exact version, then inspect again. No intentional other-package upgrade request.",
            scope: &self.scope,
            arguments: arguments(&self.scope),
            fresh_inspections: if let Some(npm) = &self.scope.npm {
                npm.catalogs
                    .iter()
                    .map(|c| {
                        c.preview(&self.scope.home)
                            .expect("validated catalog")
                            .description()
                            .clone()
                    })
                    .collect()
            } else {
                vec![
                    inspection::Preview::new(
                        inspection::Query::MiseInstalled,
                        self.scope.executable.clone(),
                        self.scope.home.clone(),
                        self.scope.search_dirs.clone(),
                    )
                    .expect("validated catalog")
                    .description()
                    .clone(),
                ]
            },
            timeout_seconds: TIMEOUT_SECONDS,
            warning: if self.scope.npm.is_some() {
                npm::WARNING
            } else {
                WARNING
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
#[derive(Serialize)]
pub struct Report {
    pub scope: Scope,
    pub attempt_id: Option<String>,
    pub no_op: bool,
    pub outcome: Option<Outcome>,
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
fn arguments(scope: &Scope) -> Vec<String> {
    if scope.npm.is_some() {
        return npm::arguments(scope);
    }
    [
        "--no-config",
        "--no-env",
        "--no-hooks",
        "install",
        "--jobs=1",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([format!("{}@{}", scope.package, scope.version)])
    .collect()
}
fn same_source(a: &Entry, b: &Entry) -> bool {
    a.path == b.path
        && adoption::same(a, b, true)
        && a.parents.len() == b.parents.len()
        && a.parents.iter().zip(&b.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}
fn query(host: &HostContext, request: &Request) -> Result<inspection::Preview> {
    inspection::Preview::new(
        inspection::Query::MiseInstalled,
        request.executable.clone(),
        host.home.clone(),
        request.search_dirs.clone(),
    )
    .map_err(|_| invalid("invalid explicit mise inspection scope"))
}
fn sources(
    host: &HostContext,
    request: &Request,
    npm: Option<&npm::Scope>,
) -> Result<(Manifest, Scope, Sources)> {
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
    let mut profiles = local.active_profiles;
    for profile in &request.profiles {
        if !profiles.contains(profile) {
            profiles.push(profile.clone());
        }
    }
    let active = manifest.profiles.resolve(host.platform, &profiles)?;
    let tool = manifest
        .tools
        .iter()
        .find(|t| t.id == request.tool && crate::profiles::selected(&t.profiles, &active))
        .ok_or_else(|| invalid("selected tool is not active in this scope"))?;
    let spec = tool
        .install
        .get(&host.platform)
        .filter(|s| {
            s.provider
                == if npm.is_some() {
                    Provider::Npm
                } else {
                    Provider::Mise
                }
        })
        .ok_or_else(|| invalid("only exact mise and npm installations are supported"))?;
    let VersionPolicy::Exact { version } =
        VersionPolicy::parse(spec.provider, spec.version.as_deref())
            .map_err(|_| invalid("unsupported installation policy"))?
    else {
        return Err(invalid("installation requires an exact version"));
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
    } else {
        query(host, request)?;
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
    };
    if !attempts::valid_scope(&scope) {
        return Err(invalid("installation scope exceeds supported bounds"));
    }
    let sources = Sources {
        manifest: entry,
        local_config,
        executable,
        host_sha256: journal::digest(journal::encode(host)?.as_bytes()),
        npm: if scope.npm.is_some() {
            Some(npm::sources(&scope)?)
        } else {
            None
        },
    };
    Ok((manifest, scope, sources))
}
fn bounded_metadata(scope: &Scope, sources: &Sources) -> Result<()> {
    if journal::encode(&(scope, sources))?.len() > 48 * 1024 {
        return Err(invalid("installation scope exceeds metadata limit"));
    }
    Ok(())
}
/// Process-free. The supplied report is metadata, not authority. Binary names are
/// independently observed in explicit, separate directories; no candidates run.
pub fn prepare(
    host: &HostContext,
    request: Request,
    selected: &inspection::Report,
) -> Result<Preview> {
    let (manifest, scope, sources) = sources(host, &request, None)?;
    bounded_metadata(&scope, &sources)?;
    let r = evaluate(host, &manifest, &scope, selected)?;
    if r.action != planning::PackageAction::ReviewMissingPackage || !r.blockers.is_empty() {
        return Err(invalid(
            "only an observed missing exact mise package with clear prerequisites can be reviewed",
        ));
    }
    Ok(Preview {
        request,
        scope,
        sources,
    })
}
fn evaluate(
    host: &HostContext,
    manifest: &Manifest,
    scope: &Scope,
    report: &inspection::Report,
) -> Result<planning::Row> {
    if report.description.query != inspection::Query::MiseInstalled
        || report.description.executable != scope.executable
        || report.description.home != scope.home
        || report.description.trusted_search_path != scope.search_dirs
    {
        return Err(invalid("selected snapshot has a different provider scope"));
    }
    evaluate_catalogs(host, manifest, scope, &[report])
}
fn evaluate_catalogs(
    host: &HostContext,
    manifest: &Manifest,
    scope: &Scope,
    reports: &[&inspection::Report],
) -> Result<planning::Row> {
    let binary_report = binaries::observe(manifest, &scope.profiles, &scope.tool_search_dirs)?;
    let catalogs = if let Some(npm) = &scope.npm {
        npm::catalogs(scope, npm, reports)?
    } else {
        scope
            .bound_tools
            .iter()
            .map(|id| (id.clone(), reports[0]))
            .collect()
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
fn observe(
    host: &HostContext,
    manifest: &Manifest,
    scope: &Scope,
    reports: &[&inspection::Report],
) -> (PackageState, BinaryState) {
    match evaluate_catalogs(host, manifest, scope, reports) {
        Ok(row) => {
            let package = match row.package {
                planning::PackageFinding::Present { versions }
                    if versions
                        .iter()
                        .any(|v| v.policy_match == VersionMatch::Satisfied) =>
                {
                    PackageState::ExactPresent
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
            (package, binary)
        }
        Err(_) => (PackageState::Unavailable, BinaryState::Unavailable),
    }
}
fn collect(host: &HostContext, old: &Preview) -> Result<Vec<inspection::Report>> {
    let (manifest, scope, _) = revalidate(host, old)?;
    if let Some(npm) = &scope.npm {
        npm.catalogs
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
        Ok(vec![query(host, &old.request)?.approve().inspect()])
    }
}
fn revalidate(host: &HostContext, old: &Preview) -> Result<(Manifest, Scope, Sources)> {
    let (manifest, scope, fresh) = sources(host, &old.request, old.scope.npm.as_ref())?;
    if journal::encode(&scope)? != journal::encode(&old.scope)?
        || fresh.host_sha256 != old.sources.host_sha256
        || !same_source(&fresh.manifest, &old.sources.manifest)
        || !same_source(&fresh.local_config, &old.sources.local_config)
        || !same_source(&fresh.executable, &old.sources.executable)
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
    let (manifest, scope, fresh) = revalidate(host, &old)?;
    observer(Event::FreshInspection);
    let before = collect(host, &old)?;
    let before_refs = before.iter().collect::<Vec<_>>();
    let row = evaluate_catalogs(host, &manifest, &scope, &before_refs)?;
    revalidate(host, &old)?;
    if row.action == planning::PackageAction::NoChange {
        return Ok(Report {
            scope,
            attempt_id: None,
            no_op: true,
            outcome: None,
            possible_partial_effects: false,
            readiness_verified: false,
        });
    }
    if row.action != planning::PackageAction::ReviewMissingPackage || !row.blockers.is_empty() {
        return Err(invalid(
            "fresh inspection refused installation; inspect current scope and review again",
        ));
    }
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
    let (mut package, mut binary) = if revalidate(host, &old).is_ok() {
        match collect(host, &old) {
            Ok(after) => observe(host, &manifest, &scope, &after.iter().collect::<Vec<_>>()),
            Err(_) => (PackageState::Unavailable, BinaryState::Unavailable),
        }
    } else {
        // Do not run a provider whose binding changed during installation.
        (PackageState::Unavailable, BinaryState::Unavailable)
    };
    if revalidate(host, &old).is_err() {
        package = PackageState::Unavailable;
        binary = BinaryState::Unavailable;
    }
    let outcome = Outcome {
        process,
        package,
        binary,
    };
    attempts::finish(host, lock, &attempt, outcome).map_err(|_| crate::Error::Invalid(format!(
        "Tools attempt {} has no durable outcome; inspect and close it before retrying (possible partial effects)", attempt.id
    )))?;
    observer(Event::OutcomeDurable);
    Ok(Report {
        scope,
        attempt_id: Some(attempt.id),
        no_op: false,
        outcome: Some(outcome),
        possible_partial_effects: process != ProcessOutcome::Success
            || package != PackageState::ExactPresent,
        readiness_verified: false,
    })
}
