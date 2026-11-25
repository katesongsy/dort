//! Declaration-driven checks and installs shared by the CLI and TUI.
use super::{inspection_defaults, installation, reconciliation, tools};
use dort_core::{
    Error, Result,
    context::HostContext,
    model::Provider,
    observation::{self, Entry, Fingerprint},
    state::WriterLock,
    tools::{VersionMatch, inspection::Query, install, planning::PackageFinding},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MAX_INSTALLS: usize = 32;

pub(crate) fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Brew => "Homebrew",
        Provider::Npm => "npm",
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Source {
    fingerprint: Option<Fingerprint>,
    hash: Option<String>,
    configuration: Option<Fingerprint>,
}
impl Source {
    pub fn capture(host: &HostContext, entry: &Entry) -> Result<Self> {
        Ok(Self {
            fingerprint: entry.fingerprint.clone(),
            hash: entry.sha256.clone(),
            configuration: observation::inspect(&host.local_config_path(), false)?.fingerprint,
        })
    }
}
#[derive(Clone)]
pub struct Input {
    pub manifest: PathBuf,
    pub profiles: Vec<String>,
    pub detection: inspection_defaults::Inputs,
}
#[derive(Serialize)]
pub struct Issue {
    pub tool: String,
    pub reason: String,
    pub next_step: String,
}
struct CheckScope {
    id: String,
    tools: Vec<String>,
    preview: tools::Preview,
}
pub struct CheckPreview {
    environment: bool,
    prepared: reconciliation::Prepared,
    source: Source,
    scopes: Vec<CheckScope>,
    bindings: BTreeMap<String, String>,
    issues: Vec<Issue>,
}
pub struct CheckApproval(CheckPreview);
#[derive(Serialize)]
pub struct ScopeReview<'a> {
    pub id: &'a str,
    pub tools: &'a [String],
    pub query: tools::Review<'a>,
}
#[derive(Serialize)]
pub struct CheckReview<'a> {
    pub scope: &'a reconciliation::Scope,
    pub bindings: &'a BTreeMap<String, String>,
    pub inspections: Vec<ScopeReview<'a>>,
    pub needs_setup: &'a [Issue],
    pub warning: &'static str,
}
#[derive(Serialize)]
pub struct Checked {
    pub reconciliation: reconciliation::Report,
    pub snapshots: Vec<reconciliation::Snapshot>,
    pub needs_setup: Vec<Issue>,
    #[serde(skip)]
    pub source: Source,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Installed,
    Missing,
    VersionDiffers,
    CouldNotCheck,
    NeedsAttention,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Missing => "Missing",
            Self::VersionDiffers => "Version differs",
            Self::CouldNotCheck => "Couldn't check",
            Self::NeedsAttention => "Needs attention",
        }
    }
}
pub fn status(row: &dort_core::tools::planning::Row) -> Status {
    match &row.package {
        PackageFinding::Absent => Status::Missing,
        PackageFinding::Present { versions } => {
            if versions
                .iter()
                .any(|v| v.policy_match == VersionMatch::Satisfied)
            {
                Status::Installed
            } else if !versions.is_empty()
                && versions
                    .iter()
                    .all(|v| v.policy_match == VersionMatch::Mismatch)
            {
                Status::VersionDiffers
            } else {
                Status::CouldNotCheck
            }
        }
        PackageFinding::NotObserved
        | PackageFinding::Unavailable { .. }
        | PackageFinding::ScopeMismatch { .. } => Status::CouldNotCheck,
        _ => Status::NeedsAttention,
    }
}
fn query(provider: Provider) -> Query {
    match provider {
        Provider::Brew => Query::BrewFormulae,
        Provider::Npm => Query::NpmGlobals,
    }
}
fn scope_id(query: Query) -> &'static str {
    match query {
        Query::BrewFormulae => "check-brew",
        Query::NpmGlobals => "check-npm",
    }
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}
fn withheld(error: Error) -> Error {
    match error {
        Error::Parse(_) => invalid("Invalid TOML; contents withheld."),
        other => other,
    }
}
fn read_manifest(path: &Path) -> Result<(dort_core::Manifest, Entry)> {
    observation::read_manifest(path).map_err(|_| {
        invalid("Declared tools manifest unavailable, unsafe or invalid; contents withheld.")
    })
}

/// Resolve captured PATH entries without running programs. Existing unresolved paths
/// remain in scope so failed observations cannot become a healthy absence.
fn directories(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for path in paths
        .iter()
        .filter(|p| p.is_absolute() && p.to_str().is_some())
    {
        let physical = match path.canonicalize() {
            Ok(physical) => physical,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => path.clone(),
        };
        if !dirs.contains(&physical) {
            dirs.push(physical);
        }
    }
    if dirs.is_empty() {
        return Err(invalid(
            "No physical search directories found; choose tool directories in Tool settings.",
        ));
    }
    if dirs.len() > dort_core::tools::binaries::MAX_DIRECTORIES {
        return Err(invalid(
            "Too many search directories; choose a bounded scope in Tool settings.",
        ));
    }
    Ok(dirs)
}
impl Input {
    /// Process-free preparation; approvals expose every automatically selected scope.
    pub fn prepare(&self, host: &HostContext) -> Result<CheckPreview> {
        let (manifest, entry) = read_manifest(&self.manifest)?;
        let dirs = directories(&self.detection.search_dirs)?;
        let prepared = reconciliation::Request {
            manifest: self.manifest.clone(),
            tool_search_dirs: dirs.clone(),
            profiles: self.profiles.clone(),
        }
        .prepare(host)
        .map_err(withheld)?;
        let source = Source::capture(host, &entry)?;
        let (_, after) = read_manifest(&self.manifest)?;
        if source != Source::capture(host, &after)? {
            return Err(invalid("Declarations changed; check again."));
        }
        let mut inputs = self.detection.clone();
        inputs.search_dirs = dirs;
        let defaults = inputs.detect(host, Some(self.manifest.clone()));
        let mut scopes = Vec::new();
        let mut bindings = BTreeMap::new();
        let mut issues = Vec::new();
        for mut choice in defaults.choices {
            let Some(base) = choice.request.take() else {
                continue;
            };
            let selected: Vec<_> = prepared
                .tools
                .iter()
                .filter(|t| t.provider.is_some_and(|p| query(p) == base.query))
                .collect();
            if selected.is_empty() {
                continue;
            }
            let groups = if base.query == Query::NpmGlobals {
                let mut groups: BTreeMap<Option<String>, Vec<_>> = BTreeMap::new();
                for tool in selected {
                    let runtime = manifest
                        .tools
                        .iter()
                        .find(|t| t.id == tool.id)
                        .map(|t| &t.install)
                        .and_then(|s| s.runtime_tool.clone());
                    groups.entry(runtime).or_default().push(tool);
                }
                groups.into_iter().collect::<Vec<_>>()
            } else {
                vec![(None, selected)]
            };
            let multiple = groups.len() > 1;
            for (index, (runtime, tools)) in groups.into_iter().enumerate() {
                let saved = self.detection.overrides.iter().find(|r| {
                    r.query == base.query
                        && (base.query != Query::NpmGlobals || r.runtime_tool == runtime)
                });
                let mut request = saved.cloned().unwrap_or_else(|| base.clone());
                if request.query == Query::NpmGlobals {
                    request.runtime_tool = runtime;
                    request.manifest = Some(self.manifest.clone());
                }
                let ambiguous = request.query == Query::NpmGlobals && multiple && saved.is_none();
                let preview = if ambiguous {
                    None
                } else {
                    request.clone().prepare(host).ok()
                };
                let id = if request.query == Query::NpmGlobals && multiple {
                    format!("{}-{}", scope_id(request.query), index + 1)
                } else {
                    scope_id(request.query).to_owned()
                };
                if let Some(preview) = preview {
                    let mut ids = Vec::new();
                    for tool in tools {
                        bindings.insert(tool.id.clone(), id.clone());
                        ids.push(tool.id.clone());
                    }
                    scopes.push(CheckScope {
                        id,
                        tools: ids,
                        preview,
                    });
                } else {
                    for tool in tools {
                        issues.push(Issue {
                            tool: tool.id.clone(),
                            reason: if ambiguous {
                                "Multiple Node runtimes need separate Node/npm settings.".into()
                            } else {
                                choice.reason.into()
                            },
                            next_step: "Open this tool's settings, then check installed again."
                                .into(),
                        });
                    }
                }
            }
        }
        if scopes.len() > reconciliation::MAX_SNAPSHOTS {
            return Err(invalid(
                "Too many package-manager scopes for one check; select fewer profiles.",
            ));
        }
        Ok(CheckPreview {
            environment: false,
            prepared,
            source,
            scopes,
            bindings,
            issues,
        })
    }
}
impl CheckPreview {
    pub fn saves_environment_check(&self) -> bool {
        self.environment
    }

    pub fn for_environment(mut self) -> Self {
        self.environment = true;
        self
    }

    pub fn review(&self) -> CheckReview<'_> {
        CheckReview {
            scope: &self.prepared.scope,
            bindings: &self.bindings,
            inspections: self
                .scopes
                .iter()
                .map(|s| ScopeReview {
                    id: &s.id,
                    tools: &s.tools,
                    query: s.preview.review(),
                })
                .collect(),
            needs_setup: &self.issues,
            warning: if self.environment {
                "Environment check runs the displayed managers and saves observations for 10 minutes. Checks may change caches. No packages are installed."
            } else {
                "Checks can run package-manager configuration and plugins, and change caches. No packages are installed. Command lookup reads metadata only."
            },
        }
    }
    pub fn approve(self) -> CheckApproval {
        CheckApproval(self)
    }
}
impl CheckApproval {
    pub fn review(&self) -> CheckReview<'_> {
        self.0.review()
    }
    pub fn check(self, host: &HostContext) -> Result<Checked> {
        let p = self.0;
        let (_, entry) = read_manifest(&p.prepared.scope.manifest)?;
        if p.source != Source::capture(host, &entry)? {
            return Err(invalid(
                "Declaration or profiles changed. Review the check again.",
            ));
        }
        let snapshots: Vec<_> = p
            .scopes
            .into_iter()
            .map(|s| reconciliation::Snapshot {
                id: s.id,
                report: Arc::new(s.preview.approve().inspect()),
            })
            .collect();
        if reconciliation::snapshot_records(&snapshots) > reconciliation::MAX_SNAPSHOT_RECORDS {
            return Err(invalid(
                "Check results are too large. Use a smaller profile.",
            ));
        }
        let reconciliation = p
            .prepared
            .reconcile(&snapshots, p.bindings)
            .map_err(withheld)?;
        let (_, entry) = read_manifest(&reconciliation.scope.manifest)?;
        if p.source != Source::capture(host, &entry)? {
            return Err(invalid("Declaration or profiles changed. Check again."));
        }
        if p.environment {
            let scope = &reconciliation.scope;
            let root = scope
                .manifest
                .parent()
                .ok_or_else(|| invalid("Choose a registered repository."))?;
            let reports = snapshots.iter().map(|snapshot| {
                let observed = &snapshot.report.provider;
                (
                    reconciliation
                        .bindings
                        .iter()
                        .filter(|(_, id)| **id == snapshot.id)
                        .map(|(tool, _)| tool.clone())
                        .collect(),
                    dort_core::tools::inspection::Report {
                        description: observed.description.clone(),
                        observation: observed.observation.clone(),
                    },
                )
            });
            dort_core::environment::save_check(
                host,
                root,
                &scope.temporary_profiles,
                scope.tool_search_dirs.clone(),
                reports,
                || {
                    let (_, entry) = read_manifest(&scope.manifest)?;
                    if p.source != Source::capture(host, &entry)? {
                        return Err(invalid("Declaration or profiles changed. Check again."));
                    }
                    Ok(())
                },
            )?;
        }
        Ok(Checked {
            reconciliation,
            snapshots,
            needs_setup: p.issues,
            source: p.source,
        })
    }
}
impl Checked {
    pub fn available(&self) -> bool {
        self.needs_setup.is_empty()
            && self.reconciliation.binaries.available()
            && self.snapshots.iter().all(|s| s.report.available())
    }
    pub fn missing(&self) -> usize {
        self.reconciliation
            .plan
            .rows
            .iter()
            .filter(|r| matches!(r.package, PackageFinding::Absent))
            .count()
    }
    pub fn installable(&self) -> usize {
        self.reconciliation
            .plan
            .rows
            .iter()
            .filter(|r| installation::eligible(r))
            .count()
    }
    pub fn prepare_missing(&self, host: &HostContext) -> Result<InstallPreview> {
        let (_, entry) = read_manifest(&self.reconciliation.scope.manifest)?;
        if self.source != Source::capture(host, &entry)? {
            return Err(invalid(
                "Declarations changed; check installed again before installation.",
            ));
        }
        let mut items = Vec::new();
        let mut deferred = Vec::new();
        for row in &self.reconciliation.plan.rows {
            if !matches!(row.package, PackageFinding::Absent) {
                continue;
            }
            if !installation::eligible(row) {
                deferred.extend(install_issues(row));
                continue;
            }
            match installation::Request::selected(&self.reconciliation, &row.id, &self.snapshots)
                .and_then(|r| r.prepare(host))
            {
                Ok(preview) => items.push(InstallItem {
                    tool: row.id.clone(),
                    preview,
                }),
                Err(error) => deferred.push(Issue {
                    tool: row.id.clone(),
                    reason: withheld(error).to_string(),
                    next_step: "Fix the path shown above, or choose another in Tool settings, then check installed again.".into(),
                }),
            }
        }
        if items.len() > MAX_INSTALLS {
            return Err(invalid(
                "At most 32 tools can share an install review; select a smaller profile.",
            ));
        }
        Ok(InstallPreview {
            items,
            deferred,
            source: self.source.clone(),
            manifest: self.reconciliation.scope.manifest.clone(),
        })
    }
}
/// Typed missing-package exclusions, shared by review and per-tool details.
pub fn install_issues(row: &dort_core::tools::planning::Row) -> Vec<Issue> {
    use dort_core::tools::{VersionPolicy, planning::Blocker};
    let mut issues = Vec::new();
    let mut add = |reason: String, next: &str| {
        issues.push(Issue {
            tool: row.id.clone(),
            reason,
            next_step: next.into(),
        })
    };
    if let Some(d) = &row.declaration {
        if d.provider == Provider::Npm && !matches!(d.policy, VersionPolicy::Exact { .. }) {
            let policy = match &d.policy {
                VersionPolicy::ProviderDefault {} => "default",
                VersionPolicy::Latest {} => "latest",
                VersionPolicy::Exact { .. } => unreachable!(),
            };
            add(
                format!(
                    "{} installation needs an exact version; this declaration uses {policy}.",
                    provider_label(d.provider)
                ),
                "Edit this declaration to choose an exact version, then check again.",
            );
        } else if d.provider == Provider::Brew
            && !matches!(
                d.policy,
                VersionPolicy::ProviderDefault {} | VersionPolicy::Latest {}
            )
        {
            add(
                "This package manager installs only its current package; the declaration pins a version."
                    .into(),
                "Edit the declaration to use default/latest, or install the pinned version separately and check again.",
            );
        } else if d.provider == Provider::Brew && !install::brew::formula(&d.package) {
            add(
                "The package name is outside the supported install scope.".into(),
                "Edit the package to a supported Homebrew core formula, then check again.",
            );
        }
    }
    for blocker in &row.blockers {
        let (why, next) = match blocker {
            Blocker::ProviderNotObserved => (
                "The package manager has not been checked.".into(),
                "Open Tool settings, then check installed again.",
            ),
            Blocker::ProviderUnavailable => (
                "The package manager check failed.".into(),
                "Open tool details, fix Tool settings and check again.",
            ),
            Blocker::ProviderScopeMismatch => (
                "Tool settings do not match this declaration.".into(),
                "Review Tool settings, then check again.",
            ),
            Blocker::AmbiguousPackageIdentity => (
                "The package manager reported more than one match.".into(),
                "Review the package and Tool settings, then check again.",
            ),
            Blocker::InvalidProviderMetadata => (
                "The package manager returned invalid data.".into(),
                "Fix Tool settings and check again.",
            ),
            Blocker::VersionMismatch => (
                "Installed version differs from the declaration.".into(),
                "Edit the declaration or install that version separately. Dort does not upgrade automatically.",
            ),
            Blocker::VersionUnverifiable => (
                "The version cannot be verified.".into(),
                "Review the declaration and Tool settings, then check again.",
            ),
            Blocker::BinaryNotObserved | Blocker::BinaryObservationUnavailable => (
                "Command lookup failed.".into(),
                "Check command search folders in Tool settings, then check again.",
            ),
            Blocker::UnattributedBinaryCandidates => (
                "A command exists, but no package owns it.".into(),
                "Review the command candidates in tool details and resolve the conflict.",
            ),
            Blocker::RuntimeBindingUnverified => (
                "The npm runtime is not verified.".into(),
                "Check the runtime's Node and npm settings in Tool settings.",
            ),
            Blocker::DependencyInactive { tool } => (
                format!("Required tool {tool} is outside the active profiles."),
                "Enable the required profile or edit the dependency, then check again.",
            ),
            Blocker::DependencyBlocked { tool } => (
                format!("Required tool {tool} needs attention."),
                "Resolve the required tool first, then check installed again.",
            ),
            Blocker::DependencyNeedsReview { tool } => (
                format!("Required tool {tool} needs its own installation review."),
                "Install the required tool first, then check installed again.",
            ),
        };
        add(why, next);
    }
    if issues.is_empty() {
        issues.push(Issue {
            tool: row.id.clone(),
            reason: "This package cannot be installed here.".into(),
            next_step: "Review the declaration and Tool settings, then check again.".into(),
        });
    }
    issues
}
struct InstallItem {
    tool: String,
    preview: install::Preview,
}
pub struct InstallPreview {
    items: Vec<InstallItem>,
    pub deferred: Vec<Issue>,
    source: Source,
    manifest: PathBuf,
}
pub struct InstallApproval {
    items: Vec<(String, install::Approval)>,
    source: Source,
    manifest: PathBuf,
    profiles: BTreeSet<String>,
}
#[derive(Serialize)]
pub struct InstallReview<'a> {
    pub installations: Vec<(&'a str, install::Review<'a>)>,
    pub deferred: &'a [Issue],
    pub warning: &'static str,
}
impl InstallPreview {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn review(&self) -> InstallReview<'_> {
        InstallReview {
            installations: self
                .items
                .iter()
                .map(|i| (i.tool.as_str(), i.preview.review()))
                .collect(),
            deferred: &self.deferred,
            warning: if self.is_empty() {
                "No installs are ready. Fix the listed issues, then check again."
            } else {
                "Runs package-manager and dependency code. Installs one tool at a time; stops on failure. Partial effects; no rollback."
            },
        }
    }
    pub fn approve(self) -> Result<InstallApproval> {
        if self.items.is_empty() {
            return Err(invalid("No eligible missing tools to install."));
        }
        let scope = self.items[0].preview.review();
        let manifest = self.manifest;
        let profiles = scope.scope.profiles.clone();
        Ok(InstallApproval {
            items: self
                .items
                .into_iter()
                .map(|i| (i.tool, i.preview.approve()))
                .collect(),
            manifest,
            profiles,
            source: self.source,
        })
    }
}
#[derive(Serialize)]
pub struct InstallResult {
    pub tool: String,
    pub report: Option<install::Report>,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct Installed {
    pub results: Vec<InstallResult>,
    pub not_started: Vec<String>,
}
impl Installed {
    pub fn succeeded(&self) -> bool {
        self.not_started.is_empty()
            && self.results.iter().all(|r| {
                r.error.is_none() && r.report.as_ref().is_some_and(installation::succeeded)
            })
    }
}
impl InstallApproval {
    pub fn scope(&self) -> (&Path, &BTreeSet<String>) {
        (&self.manifest, &self.profiles)
    }
    pub fn tools(&self) -> Vec<&str> {
        self.items.iter().map(|(id, _)| id.as_str()).collect()
    }
    pub fn execute(
        self,
        host: &HostContext,
        lock: &WriterLock,
        mut progress: impl FnMut(install::Event),
    ) -> Installed {
        let source_check = read_manifest(&self.manifest).and_then(|(_, entry)| {
            if self.source == Source::capture(host, &entry)? {
                Ok(())
            } else {
                Err(invalid(
                    "Declaration or profiles changed. Check and review the install again.",
                ))
            }
        });
        let mut pending = self.items.into_iter();
        if let Err(error) = source_check {
            let results = pending
                .next()
                .map(|(tool, _)| InstallResult {
                    tool,
                    report: None,
                    error: Some(error.to_string()),
                })
                .into_iter()
                .collect();
            return Installed {
                results,
                not_started: pending.map(|(tool, _)| tool).collect(),
            };
        }
        let mut results = Vec::new();
        for (tool, approval) in pending.by_ref() {
            let result = install::execute(host, lock, approval, &mut progress);
            let success = result.as_ref().is_ok_and(installation::succeeded);
            results.push(match result {
                Ok(report) => InstallResult {
                    tool,
                    report: Some(report),
                    error: None,
                },
                Err(error) => InstallResult {
                    tool,
                    report: None,
                    error: Some(withheld(error).to_string()),
                },
            });
            if !success {
                break;
            }
        }
        Installed {
            results,
            not_started: pending.map(|(tool, _)| tool).collect(),
        }
    }
}

pub fn manifest(root: &Path) -> PathBuf {
    root.join("dort.toml")
}
