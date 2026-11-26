//! Prepares and dispatches actions over core services.
use crate::services::{BootstrapApproval, BootstrapPreview, FileApproval};
use dort_core::{
    Error, Result,
    adoption::Event,
    bootstrap,
    context::HostContext,
    journal,
    local::LocalConfig,
    planner::{self, Plan, Request, UnmanageMode},
    recovery,
    state::WriterLock,
};
use std::path::{Path, PathBuf};

pub struct ManagedRow {
    pub id: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub state: &'static str,
    pub source_kind: &'static str,
    pub target_kind: &'static str,
    pub blockers: Vec<String>,
}

fn entry_kind(entry: Option<&dort_core::observation::Entry>) -> &'static str {
    use dort_core::observation::Kind;
    match entry.map(|e| &e.kind) {
        Some(Kind::Missing) => "Missing",
        Some(Kind::Regular) => "Regular file",
        Some(Kind::Directory) => "Directory",
        Some(Kind::Symlink) => "Symbolic link",
        Some(Kind::Special) => "Special file",
        None => "Unavailable",
    }
}
/// Read-only summary facts, never reconstructed from human report strings.
pub struct Overview {
    pub host: HostContext,
    pub registered: Option<bool>,
    pub registry_count: Option<usize>,
    pub configuration_error: Option<String>,
    pub files: Option<FileCounts>,
}
#[derive(Default)]
pub struct FileCounts {
    pub linked: usize,
    pub changes: usize,
    pub blocked: usize,
    pub plan_blocked: bool,
}
#[derive(Default)]
pub struct Data {
    pub overview: Option<Overview>,
    pub repository: Option<PathBuf>,
    pub known_roots: Vec<PathBuf>,
    pub profiles: Vec<String>,
    pub profile_rows: Vec<dort_core::profiles::Row>,
    pub profile_declarations: Option<dort_core::profiles::Profiles>,
    pub files: Vec<(String, String)>,
    pub managed: Vec<ManagedRow>,
    pub file_declarations: Vec<dort_core::model::ManagedFile>,
    pub tools: Option<std::result::Result<Vec<dort_core::model::Tool>, String>>,
    pub tools_source: Option<crate::services::declared_tools::Source>,
    pub files_error: Option<String>,
    pub history: Vec<journal::Diagnosis>,
    pub history_error: Option<String>,
}

// TOML parser Display includes source lines. Ordinary TUI errors are metadata only.
pub fn diagnostic(error: &Error) -> String {
    match error {
        Error::Parse(_) => {
            "Invalid TOML. Fix the manifest syntax or choose another manifest.".into()
        }
        Error::Validation(issues) if !issues.is_empty() => issues.join("\n"),
        _ => error.to_string(),
    }
}

pub fn input_problem(error: &Error) -> Option<String> {
    match error {
        Error::Invalid(_) | Error::Validation(_) | Error::Parse(_) => Some(diagnostic(error)),
        Error::Io { path, source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            Some(format!(
                "Path not found. Check the path and try again.\n{}",
                path.display()
            ))
        }
        Error::Io { path, source, .. } if source.kind() == std::io::ErrorKind::NotADirectory => {
            Some(format!(
                "A folder in this path is a file. Correct the path and try again.\n{}",
                path.display()
            ))
        }
        _ => None,
    }
}

/// Preserve typed write blockers instead of reconstructing them from message text.
pub fn present(result: Result<Output>) -> std::result::Result<Output, String> {
    match result {
        Err(Error::RecoveryRequired(blocker)) => Ok(Output::RecoveryRequired(blocker)),
        other => other.map_err(|error| diagnostic(&error)),
    }
}

/// Deferred quit returns through CLI error presentation after terminal restoration.
pub fn exit_error(error: Error) -> Error {
    if matches!(error, Error::Parse(_)) {
        Error::Invalid(diagnostic(&error))
    } else {
        error
    }
}

#[cfg(test)]
pub fn load(host: &HostContext, repository: Option<&Path>) -> Result<Data> {
    load_scoped(host, repository, &[])
}
pub fn load_scoped(
    host: &HostContext,
    repository: Option<&Path>,
    requested: &[String],
) -> Result<Data> {
    let (history, history_error) = match journal::diagnose(host) {
        Ok(rows) => (rows, None),
        Err(error) => (vec![], Some(diagnostic(&error))),
    };
    let mut overview = Overview {
        host: host.clone(),
        registered: None,
        registry_count: None,
        configuration_error: None,
        files: None,
    };
    let local = LocalConfig::load(host);
    let mut files = vec![];
    let mut managed = vec![];
    let mut files_error = None;
    let mut known_roots = vec![];
    let mut profiles = vec![];
    let mut profile_rows = vec![];
    let mut profile_declarations = None;
    let mut file_declarations = vec![];
    let mut selected = None;
    let mut tools = None;
    let mut tools_source = None;
    match local {
        Err(error) => {
            let error = diagnostic(&error);
            files_error = Some(format!("Configuration unavailable: {error}"));
            overview.configuration_error = Some(error);
        }
        Ok(local) => {
            overview.registry_count = Some(local.repositories.len());
            known_roots = local
                .repositories
                .values()
                .map(|record| record.root.clone())
                .collect();
            match local.select_repository(repository) {
                Err(error) => {
                    let error = diagnostic(&error);
                    files_error = Some(format!(
                        "{error}\nOpen Overview to choose, clone, create or attach a repository."
                    ));
                }
                Ok(root) => {
                    let registered = known_roots.contains(&root);
                    overview.registered = Some(registered);
                    selected = Some(root.clone());
                    tools = Some(
                        dort_core::observation::read_manifest(&root.join("dort.toml"))
                            .map(|(manifest, entry)| {
                                tools_source =
                                    crate::services::declared_tools::Source::capture(host, &entry)
                                        .ok();
                                profile_rows = local
                                    .profile_rows(
                                        &manifest.profiles,
                                        host.platform,
                                        Some(&root),
                                        requested,
                                    )
                                    .unwrap_or_default();
                                profile_declarations = Some(manifest.profiles);
                                file_declarations = manifest.files;
                                manifest.tools
                            })
                            .map_err(|error| diagnostic(&error)),
                    );
                    match crate::services::observe(
                        host,
                        &local,
                        Some(&root),
                        requested,
                        Request::Apply,
                    ) {
                        Err(error) => {
                            let error = diagnostic(&error);
                            files_error = Some(format!("Could not read files: {error}"));
                        }
                        Ok(snapshot) => {
                            profiles = snapshot.active_profiles.iter().cloned().collect();
                            let plan = planner::plan(snapshot);
                            let mut counts = FileCounts {
                                plan_blocked: plan.has_blockers(),
                                ..Default::default()
                            };
                            for file in &plan.files {
                                match file.outcome {
                                    planner::Outcome::NoOp => counts.linked += 1,
                                    planner::Outcome::Change => counts.changes += 1,
                                    planner::Outcome::Blocked => counts.blocked += 1,
                                }
                            }
                            overview.files = Some(counts);
                            for link in &plan.inputs.inactive_links {
                                managed.push(ManagedRow {
                                    id: link.id.clone(),
                                    source: link.source.clone(),
                                    target: link.target.clone(),
                                    state: "No longer selected",
                                    source_kind: "Repository source",
                                    target_kind: "Symbolic link",
                                    blockers: vec![],
                                });
                                files.push((
                                    link.id.clone(),
                                    format!(
                                        "No longer selected. Profiles: {}",
                                        link.profiles.join(", ")
                                    ),
                                ));
                            }
                            for (file, input) in plan.files.iter().zip(&plan.inputs.files) {
                                managed.push(ManagedRow {
                                    id: file.id.clone(),
                                    source: input.source_path.clone(),
                                    target: input.target_path.clone(),
                                    source_kind: entry_kind(input.source.as_ref()),
                                    target_kind: entry_kind(input.destination.as_ref()),
                                    state: if input.mapping.mode
                                        == dort_core::model::FileMode::Template
                                    {
                                        "Preview only"
                                    } else {
                                        match file.outcome {
                                            planner::Outcome::NoOp => "Linked",
                                            planner::Outcome::Change => "Needs apply",
                                            planner::Outcome::Blocked => "Blocked",
                                        }
                                    },
                                    blockers: file.blockers.clone(),
                                });
                                files.push((
                                    file.id.clone(),
                                    format!(
                                        "{} [{:?}]\nSource: {:?} [{:?}]\nTarget: {:?} [{:?}]\n{}",
                                        file.id,
                                        file.outcome,
                                        input.source_path,
                                        input.source.as_ref().map(|e| &e.kind),
                                        input.target_path,
                                        input.destination.as_ref().map(|e| &e.kind),
                                        file.blockers.join("\n")
                                    ),
                                ));
                            }
                        }
                    }
                    for mapping in &file_declarations {
                        if files.iter().any(|(id, _)| id == &mapping.id) {
                            continue;
                        }
                        let source = root.join(
                            mapping
                                .overrides
                                .get(&host.platform)
                                .unwrap_or(&mapping.source),
                        );
                        let Ok(target) = host.target_path(&mapping.target, Some(&root)) else {
                            continue;
                        };
                        let state = if files_error.is_some() {
                            "Blocked"
                        } else {
                            "Not selected"
                        };
                        files.push((
                            mapping.id.clone(),
                            format!("{state}. Profiles: {}", mapping.profiles.join(", ")),
                        ));
                        managed.push(ManagedRow {
                            id: mapping.id.clone(),
                            source,
                            target,
                            state,
                            source_kind: "Not checked",
                            target_kind: "Not checked",
                            blockers: vec![],
                        });
                    }
                }
            }
        }
    }
    Ok(Data {
        overview: Some(overview),
        repository: selected,
        known_roots,
        profiles,
        profile_rows,
        profile_declarations,
        file_declarations,
        files,
        managed,
        tools,
        tools_source,
        files_error,
        history,
        history_error,
    })
}

pub enum Pending {
    FullCheck(Box<super::full_check::Preview>),
    Configuration(super::configuration::Preview),
    Environment(Box<dort_core::environment::Preview>),
    EnvironmentAbandon(Box<dort_core::environment::AbandonPreview>),
    Clone(Box<bootstrap::ClonePreview>),
    Profile(Box<dort_core::local::ProfilePreview>),
    ToolDeclaration(Box<dort_core::tools::declaration::Preview>),
    Ignore(dort_core::discovery_ignore::Preview),
    Files(Box<Plan>),
    Batch(Box<dort_core::batch::Plan>),
    Init(Box<BootstrapPreview>),
    Rollback(recovery::Preview),
    Bootstrap(bootstrap::RecoveryPreview),
}
impl Pending {
    pub fn report(&self) -> Result<String> {
        Ok(match self {
            Self::ToolDeclaration(p) => crate::services::declaration::report(p),
            Self::Ignore(preview) => preview.human(),
            Self::Profile(p) => p.human(),
            Self::Configuration(p) => p.human(),
            Self::Environment(p) => p.human(),
            Self::FullCheck(p) => p.human(),
            Self::EnvironmentAbandon(p) => p.human(),
            Self::Clone(p) => p.human(),
            Self::Batch(plan) => plan.human(),
            Self::Files(plan) => format!(
                "Repository: {:?}\nProfiles: {:?}\nScope: every mapping selected by this request. The preview shows what runs.\n{}",
                plan.inputs.repository,
                plan.inputs.active_profiles,
                plan.human()
            ),
            Self::Init(preview) => format!(
                "{} local repository\nPath: {:?}\nRepository ID: {}\n\nEffects:\n- {}\n\nThe change is saved only after approval.\nA failure keeps private records; open Recovery.\nExisting dotfiles stay in place.",
                if preview.report.create {
                    "Create"
                } else {
                    "Attach"
                },
                preview.report.repository,
                preview.report.repository_id,
                preview.report.effects.join("\n- ")
            ),
            Self::Rollback(preview) => format!(
                "Roll back operation\nRepository ID: {}\nOperation ID: {}\nHistory head: {}\nPaths:\n{}\n\nThis reverses the recorded changes if they still match. Later edits can block it. Private backups stay.",
                preview.repository_id,
                preview.transaction_id,
                preview.journal_head_sha256,
                preview
                    .effects
                    .iter()
                    .map(|p| format!("- {p:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            Self::Bootstrap(preview) => format!(
                "{} repository setup\nRepository: {:?}\nRepository ID: {}\nOperation ID: {}\nHistory head: {}\nRepository published: {}\nRegistration published: {}\nEffects:\n- {}\n\nAbandon keeps the published repository, registration and private files.\nResume continues the saved setup; later changes can block it. Finish or abandon it before another create or attach. Existing dotfiles stay in place.",
                match preview.action {
                    bootstrap::RecoveryAction::Resume => "Resume",
                    bootstrap::RecoveryAction::Abandon => "Abandon",
                },
                preview.repository,
                preview.repository_id,
                preview.transaction_id,
                preview.journal_head_sha256,
                preview.repository_published,
                preview.registration_published,
                preview.effects.join("\n- ")
            ),
        })
    }
    pub fn confirmation_label(&self) -> &'static str {
        match self {
            Self::Clone(_) => "Clone and register",
            Self::Environment(_) => "Run environment",
            Self::FullCheck(_) => "Run checks",
            Self::EnvironmentAbandon(_) => "Abandon environment run",
            Self::Profile(_) => "Save profile choice",
            Self::Configuration(p) => p.label(),
            _ if self.adoption() => "Adopt all",
            _ => "Confirm changes",
        }
    }
    pub fn adoption(&self) -> bool {
        match self {
            Self::Files(p) => matches!(p.inputs.request, Request::Adopt { .. }),
            Self::Batch(p) => p.inputs.action == dort_core::batch::Action::Adopt,
            _ => false,
        }
    }
    pub fn review_len(&self) -> Option<usize> {
        match self {
            Self::Files(p) => Some(p.files.len()),
            Self::Batch(p) => Some(p.items.iter().map(|i| i.files.len()).sum()),
            _ => None,
        }
    }
    pub fn removal_id(&self) -> Option<&str> {
        match self {
            Self::Files(plan) => match &plan.inputs.request {
                Request::Unmanage {
                    id,
                    mode: UnmanageMode::Remove,
                } => Some(id),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn blocked(&self) -> bool {
        match self {
            Self::Files(plan) => plan.has_blockers(),
            Self::Batch(plan) => plan.has_blockers(),
            Self::Environment(p) => p.blocked(),
            _ => false,
        }
    }
    pub fn approve(self, removal: Option<&str>) -> Result<Approved> {
        match self {
            Self::ToolDeclaration(p) => dort_core::tools::declaration::Approval::from_preview(*p)
                .map(|a| Approved::ToolDeclaration(Box::new(a))),
            Self::Ignore(preview) => Ok(Approved::Ignore(preview.approve())),
            Self::Profile(p) => Ok(Approved::Profile(Box::new(p.approve()))),
            Self::Configuration(p) => Ok(Approved::Configuration(p.approve())),
            Self::Environment(p) => Ok(Approved::Environment(Box::new(p.approve()?))),
            Self::FullCheck(p) => Ok(Approved::FullCheck(Box::new(p.approve()))),
            Self::EnvironmentAbandon(p) => Ok(Approved::EnvironmentAbandon(Box::new(p.approve()))),
            Self::Clone(p) => {
                let root = p.destination().to_owned();
                Ok(Approved::Clone(Box::new(p.approve()), root))
            }
            Self::Batch(plan) => {
                dort_core::batch::Approval::from_plan(*plan).map(|a| Approved::Batch(Box::new(a)))
            }
            Self::Files(plan) => FileApproval::from_plan(*plan, removal)
                .map(|approval| Approved::Files(Box::new(approval))),
            Self::Init(preview) => {
                let root = preview.report.repository.clone();
                Ok(Approved::Init(Box::new((*preview).approve()?), root))
            }
            Self::Rollback(p) => {
                let tid = p.transaction_id.clone();
                recovery::Approval::from_preview(p)
                    .map(|approval| Approved::Rollback(approval, tid))
            }
            Self::Bootstrap(p) => {
                let tid = p.transaction_id.clone();
                bootstrap::RecoveryApproval::from_preview(p)
                    .map(|approval| Approved::Bootstrap(approval, tid))
            }
        }
    }
}

pub enum Approved {
    FullCheck(Box<super::full_check::Approval>),
    Configuration(super::configuration::Approval),
    Environment(Box<dort_core::environment::Approval>),
    EnvironmentAbandon(Box<dort_core::environment::AbandonApproval>),
    Clone(Box<bootstrap::CloneApproval>, PathBuf),
    Profile(Box<dort_core::local::ProfileApproval>),
    ToolDeclaration(Box<dort_core::tools::declaration::Approval>),
    Ignore(dort_core::discovery_ignore::Approval),
    Files(Box<FileApproval>),
    Batch(Box<dort_core::batch::Approval>),
    Init(Box<BootstrapApproval>, PathBuf),
    Rollback(recovery::Approval, String),
    Bootstrap(bootstrap::RecoveryApproval, String),
}
pub struct Completion {
    pub operation: &'static str,
    pub transaction_id: Option<String>,
    pub repository: Option<PathBuf>,
}
impl Approved {
    pub fn execute(
        self,
        host: &HostContext,
        observer: impl FnMut(&Event) -> Result<()>,
    ) -> Result<Output> {
        let (operation, transaction_id, repository) = match self {
            Self::Configuration(super::configuration::Approval::Registry(p)) => {
                ("Repository registration", None, p.execute(host)?)
            }
            Self::Configuration(super::configuration::Approval::Repair(p)) => {
                let backup = p.execute(host)?.expect("a repair changes the manifest");
                return Ok(Output::ManifestRepaired(backup));
            }
            Self::Configuration(super::configuration::Approval::Manifest(p)) => {
                let backup = p.execute(host)?;
                let id = backup
                    .as_deref()
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .and_then(|id| id.to_str())
                    .map(str::to_owned);
                ("Manifest change", id, None)
            }
            Self::Environment(p) => return p.execute(host, observer).map(Output::EnvironmentRun),
            Self::FullCheck(p) => return p.execute(host),
            Self::EnvironmentAbandon(p) => return p.execute(host).map(Output::EnvironmentRun),
            Self::ToolDeclaration(a) => {
                let lock = WriterLock::acquire(host)?;
                (
                    "Tool declaration",
                    dort_core::tools::declaration::execute(host, &lock, *a, observer)?,
                    None,
                )
            }
            Self::Clone(p, root) => match p.execute_with_result(host)? {
                bootstrap::CloneResult::Registered(_) => ("Clone and register", None, Some(root)),
                bootstrap::CloneResult::InvalidManifest { repository, reason } => {
                    return Ok(Output::CloneNeedsRepair { repository, reason });
                }
            },
            Self::Profile(p) => {
                p.execute(host)?;
                ("Save profile choice", None, None)
            }
            Self::Ignore(approval) => {
                approval.execute(host, false)?;
                ("Discovery preferences", None, None)
            }
            Self::Batch(approval) => {
                let lock = WriterLock::acquire(host)?;
                (
                    "Batch file operation",
                    dort_core::batch::execute(host, &lock, *approval, observer)?,
                    None,
                )
            }
            Self::Files(approval) => ("File operation", (*approval).execute(host, observer)?, None),
            Self::Init(approval, root) => (
                "Create/attach",
                Some((*approval).execute(host, observer)?),
                Some(root),
            ),
            Self::Rollback(approval, tid) => {
                let lock = WriterLock::acquire(host)?;
                recovery::execute_with_observer(host, &lock, approval, observer)?;
                ("Rollback", Some(tid), None)
            }
            Self::Bootstrap(approval, tid) => {
                let lock = WriterLock::acquire(host)?;
                bootstrap::recover(host, &lock, approval, observer)?;
                ("Repository setup recovery", Some(tid), None)
            }
        };
        Ok(Output::Completed(Completion {
            operation,
            transaction_id,
            repository,
        }))
    }
}

pub struct Diagnostics {
    pub host: HostContext,
    pub registry: std::result::Result<crate::services::DoctorReport, String>,
    pub history: std::result::Result<(usize, usize), String>,
}

/// Retain intent only, never an approval or an executable preview.
#[derive(Clone)]
pub enum Retry {
    ToolsCheckPrepare(crate::services::inspection_defaults::Inputs),
    ToolsInstalledPrepare(crate::services::inspection_defaults::Inputs),
    ToolsInstallMissingPrepare(std::sync::Arc<crate::services::declared_tools::Checked>),
    ToolDeclaration(crate::services::declaration::Request),
    Check(Option<PathBuf>, Vec<String>),
    Profiles(Option<PathBuf>, Vec<String>, bool),
    Discover,
    Select(Option<PathBuf>),
    History,
    Diagnostics,
    Refresh,
    Files(Request),
    Batch(dort_core::batch::Request),
    Init(PathBuf, PathBuf),
    Rollback(String, String),
    Bootstrap(String, String, bootstrap::RecoveryAction),
}
impl Retry {
    pub fn task(self) -> Task {
        match self {
            Self::ToolsCheckPrepare(inputs) => Task::ToolsCheckPrepare(inputs),
            Self::ToolsInstalledPrepare(inputs) => Task::ToolsInstalledPrepare(inputs),
            Self::ToolsInstallMissingPrepare(checked) => Task::ToolsInstallMissingPrepare(checked),
            Self::ToolDeclaration(r) => Task::ToolDeclaration(r),
            Self::Check(manifest, requested) => Task::Check {
                manifest,
                requested,
            },
            Self::Profiles(manifest, requested, apply) => Task::Profiles {
                manifest,
                requested,
                apply,
            },
            Self::Discover => Task::Discover,
            Self::Select(root) => Task::Select(root),
            Self::History => Task::History,
            Self::Diagnostics => Task::Diagnostics,
            Self::Refresh => Task::Refresh,
            Self::Files(request) => Task::Files(request),
            Self::Batch(request) => Task::Batch(request),
            Self::Init(root, git) => Task::Init { root, git },
            Self::Rollback(r, t) => Task::Rollback(r, t),
            Self::Bootstrap(r, t, action) => Task::Bootstrap(r, t, action),
        }
    }
}
impl Task {
    pub fn retry_intent(&self) -> Option<Retry> {
        match self {
            Self::ToolsCheckPrepare(inputs) => Some(Retry::ToolsCheckPrepare(inputs.clone())),
            Self::ToolsInstalledPrepare(inputs) => {
                Some(Retry::ToolsInstalledPrepare(inputs.clone()))
            }
            Self::ToolsInstallMissingPrepare(checked) => {
                Some(Retry::ToolsInstallMissingPrepare(checked.clone()))
            }
            Self::ToolDeclaration(r) => Some(Retry::ToolDeclaration(r.clone())),
            Self::Check {
                manifest,
                requested,
            } => Some(Retry::Check(manifest.clone(), requested.clone())),
            Self::Profiles {
                manifest,
                requested,
                apply,
            } => Some(Retry::Profiles(manifest.clone(), requested.clone(), *apply)),
            Self::Discover => Some(Retry::Discover),
            Self::Select(root) => Some(Retry::Select(root.clone())),
            Self::History => Some(Retry::History),
            Self::Diagnostics => Some(Retry::Diagnostics),
            Self::Refresh => Some(Retry::Refresh),
            Self::Files(request) => Some(Retry::Files(request.clone())),
            Self::Batch(request) => Some(Retry::Batch(request.clone())),
            Self::Init { root, git } => Some(Retry::Init(root.clone(), git.clone())),
            Self::Rollback(r, t) => Some(Retry::Rollback(r.clone(), t.clone())),
            Self::Bootstrap(r, t, action) => Some(Retry::Bootstrap(r.clone(), t.clone(), *action)),
            _ => None,
        }
    }
}
pub enum Task {
    Configuration(Box<super::configuration::Request>),
    Environment {
        resume: bool,
    },
    EnvironmentCheck {
        detection: crate::services::inspection_defaults::Inputs,
        resume: bool,
    },
    EnvironmentAbandon,
    EnvironmentRecord,
    ToolsInstalledPrepare(crate::services::inspection_defaults::Inputs),
    ToolsInstalledInspect(crate::services::installed_tools::Approval),
    ToolsCheckPrepare(crate::services::inspection_defaults::Inputs),
    ToolsStandaloneCheckPrepare(crate::services::declared_tools::Input),
    ToolsCheck(Box<crate::services::declared_tools::CheckApproval>),
    ToolsInstallMissingPrepare(std::sync::Arc<crate::services::declared_tools::Checked>),
    ToolsInstallMissing(crate::services::declared_tools::InstallApproval),

    ToolAttempts,
    ToolClosePrepare(dort_core::tools::install::CloseRequest),
    ToolClose(Box<dort_core::tools::install::CloseApproval>),
    ToolDeclaration(crate::services::declaration::Request),
    IgnoredPaths,
    Ignore(PathBuf, bool),
    PreviewGenerated(String),
    DiscoveryTree(super::discovery_tree::Read),
    DiscoverContent(super::discover_content::Read),
    History,
    Diagnostics,
    Git(super::git::Read),
    GitPreview(super::git::PreviewRead),
    ToolsSettingsDetect {
        tool: dort_core::model::Tool,
        inputs: crate::services::inspection_defaults::Inputs,
    },
    ToolsSettingsSave {
        editor: crate::services::inspection_defaults::SettingsEditor,
        preview_only: bool,
    },
    Refresh,
    Discover,
    Check {
        manifest: Option<PathBuf>,
        requested: Vec<String>,
    },
    Profiles {
        manifest: Option<PathBuf>,
        requested: Vec<String>,
        apply: bool,
    },
    Select(Option<PathBuf>),
    Init {
        root: PathBuf,
        git: PathBuf,
    },
    Files(Request),
    Batch(dort_core::batch::Request),
    Rollback(String, String),
    Bootstrap(String, String, bootstrap::RecoveryAction),
    Clone {
        source: String,
        root: PathBuf,
        git: PathBuf,
    },
    Profile(PathBuf, String, dort_core::profiles::Choice),
    Execute(Approved),
}
pub enum Output {
    ManifestChecked {
        report: Box<crate::services::CheckReport>,
        standalone: bool,
        tools: Vec<(String, &'static str)>,
    },
    MappingUnchanged,
    InputRejected,
    EnvironmentRun(dort_core::environment::Run),
    EnvironmentRecord(Option<dort_core::environment::Run>),
    RecoveryRequired(Box<journal::WriteBlocker>),
    ToolsInstalledPreview(crate::services::installed_tools::Preview),
    ToolsInstalled(crate::services::installed_tools::Current),
    ToolsCheckPreview(crate::services::declared_tools::CheckPreview),
    ToolsChecked(crate::services::declared_tools::Checked),
    ToolsMissingPreview(crate::services::declared_tools::InstallPreview),
    ToolsMissingInstalled(crate::services::declared_tools::Installed),

    ToolAttempts(std::result::Result<Vec<dort_core::tools::install::Attempt>, String>),
    ToolClosePreview(Box<dort_core::tools::install::ClosePreview>),
    ToolClosed(std::result::Result<dort_core::tools::install::Attempt, String>),
    IgnoredPaths(Vec<PathBuf>),
    Generated(Box<dort_core::generated::Preview>),
    DiscoveryTree(super::discovery_tree::Report),
    DiscoverContent(super::discover_content::Report),
    History {
        rows: Vec<journal::Diagnosis>,
        error: Option<String>,
    },
    Diagnostics(Diagnostics),
    Git(super::git::Metadata),
    GitPreview(super::git::PreviewReport),
    ToolsSettingsDetected(crate::services::inspection_defaults::SettingsEditor),
    ToolsSettingsSaved {
        preview_only: bool,
    },
    ToolSettingsRejected(String),
    Data(Data),
    Discovery(crate::services::Discovery),
    Checked {
        report: Box<crate::services::CheckReport>,
        standalone: bool,
    },
    Profiles {
        report: Box<crate::services::CheckReport>,
        requested: Vec<String>,
        standalone: bool,
        apply: bool,
    },
    Selected(Option<PathBuf>, Data),
    Preview(Pending),
    RepairDraft(Box<dort_core::manifest_edit::RepairDraft>),
    ManifestRepaired(PathBuf),
    CloneNeedsRepair {
        repository: PathBuf,
        reason: String,
    },
    Completed(Completion),
}
#[cfg(test)]
pub fn perform(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Output> {
    perform_scoped(task, host, repository, &[], observer)
}
#[cfg(test)]
pub fn perform_scoped(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    requested: &[String],
    observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Output> {
    perform_scoped_with_install_progress(task, host, repository, requested, observer, |_| {})
}
pub fn perform_scoped_with_install_progress(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    requested: &[String],
    observer: impl FnMut(&Event) -> Result<()>,
    installation_observer: impl FnMut(dort_core::tools::install::Event),
) -> Result<Output> {
    match task {
        Task::ToolsInstalledPrepare(input) => {
            let manifest = LocalConfig::load(host)?
                .select_repository(repository.as_deref())
                .ok()
                .map(|r| r.join("dort.toml"));
            crate::services::installed_tools::prepare(host, manifest, input)
                .map(Output::ToolsInstalledPreview)
        }
        Task::ToolsInstalledInspect(approval) => approval.inspect().map(Output::ToolsInstalled),
        Task::EnvironmentRecord => {
            let root = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
            dort_core::environment::Run::latest(host, &root).map(Output::EnvironmentRecord)
        }
        Task::EnvironmentAbandon => {
            let root = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
            dort_core::environment::AbandonPreview::prepare(host, &root)
                .map(|p| Output::Preview(Pending::EnvironmentAbandon(Box::new(p))))
        }
        Task::Environment { resume } => {
            let root = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
            dort_core::environment::Preview::prepare(host, &root, requested, resume)
                .map(|p| Output::Preview(Pending::Environment(Box::new(p))))
        }
        Task::EnvironmentCheck { detection, resume } => {
            let local = LocalConfig::load(host)?;
            let root = local.select_repository(repository.as_deref())?;
            if !local.repositories.values().any(|r| r.root == root) {
                return Err(Error::Invalid(
                    "Choose a registered repository to check the environment.".into(),
                ));
            }
            let profiles = requested.to_vec();
            let tools = crate::services::declared_tools::Input {
                manifest: root.join("dort.toml"),
                profiles: profiles.clone(),
                detection,
            }
            .prepare_for_tools(host, false)?
            .for_environment();
            super::full_check::Preview {
                tools,
                destination: super::full_check::Destination::Environment {
                    root,
                    profiles,
                    resume,
                },
            }
            .finish_or_review(host)
        }
        Task::ToolsCheckPrepare(detection) => {
            let local = LocalConfig::load(host)?;
            let root = local.select_repository(repository.as_deref())?;
            let input = crate::services::declared_tools::Input {
                manifest: root.join("dort.toml"),
                profiles: requested.to_vec(),
                detection,
            };
            input
                .prepare_for_tools(host, false)
                .map(Output::ToolsCheckPreview)
        }
        Task::ToolsStandaloneCheckPrepare(input) => input
            .prepare_for_tools(host, true)
            .map(Output::ToolsCheckPreview),
        Task::ToolsCheck(approval) => approval.check(host).map(Output::ToolsChecked),
        Task::ToolsInstallMissingPrepare(checked) => checked
            .prepare_missing(host)
            .map(Output::ToolsMissingPreview),
        Task::ToolsInstallMissing(approval) => {
            let lock = dort_core::state::WriterLock::acquire(host)?;
            Ok(Output::ToolsMissingInstalled(approval.execute(
                host,
                &lock,
                installation_observer,
            )))
        }

        Task::ToolAttempts => Ok(Output::ToolAttempts(
            dort_core::tools::install::diagnose(host).map_err(|e| diagnostic(&e)),
        )),
        Task::ToolClosePrepare(request) => dort_core::tools::install::prepare_close(host, request)
            .map(|p| Output::ToolClosePreview(Box::new(p))),
        Task::ToolClose(approval) => Ok(Output::ToolClosed(
            (|| {
                let lock = dort_core::state::WriterLock::acquire(host)?;
                dort_core::tools::install::close(host, &lock, *approval)
            })()
            .map_err(|e| diagnostic(&e)),
        )),
        Task::ToolDeclaration(request) => {
            let repository = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
            request
                .prepare(host, &repository, requested)
                .map(|p| Output::Preview(Pending::ToolDeclaration(Box::new(p))))
        }
        Task::ToolsSettingsDetect { tool, inputs } => {
            let root = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
            crate::services::inspection_defaults::edit(
                host,
                root.join("dort.toml"),
                tool.id,
                inputs,
            )
            .map(Output::ToolsSettingsDetected)
        }
        Task::ToolsSettingsSave {
            editor,
            preview_only,
        } => {
            use crate::services::inspection_defaults::SettingsSave;
            Ok(
                match editor.save(host, repository.as_deref(), preview_only)? {
                    SettingsSave::Saved => Output::ToolsSettingsSaved { preview_only },
                    SettingsSave::InvalidInput(error) => {
                        Output::ToolSettingsRejected(diagnostic(&error))
                    }
                },
            )
        }
        Task::IgnoredPaths => Ok(Output::IgnoredPaths(
            dort_core::discovery_ignore::Rules::load(host)?
                .paths()
                .cloned()
                .collect(),
        )),
        Task::Ignore(path, ignored) => {
            let relative = if ignored {
                path.strip_prefix(&host.home)
                    .map_err(|_| Error::Invalid("ignore selection must be under HOME".into()))?
            } else {
                &path
            };
            dort_core::discovery_ignore::Preview::prepare(host, relative, ignored)
                .map(|p| Output::Preview(Pending::Ignore(p)))
        }
        Task::PreviewGenerated(id) => crate::services::preview_generated(
            host,
            &LocalConfig::load(host)?,
            repository.as_deref(),
            &id,
        )
        .map(|preview| Output::Generated(Box::new(preview))),
        Task::DiscoveryTree(read) => Ok(Output::DiscoveryTree(read.perform())),
        Task::DiscoverContent(read) => Ok(Output::DiscoverContent(read.perform(host))),
        Task::History => Ok(match journal::diagnose(host) {
            Ok(rows) => Output::History { rows, error: None },
            Err(error) => Output::History {
                rows: vec![],
                error: Some(diagnostic(&error)),
            },
        }),
        Task::Diagnostics => Ok(Output::Diagnostics(Diagnostics {
            host: host.clone(),
            registry: LocalConfig::load(host)
                .map(|local| crate::services::doctor(host, &local))
                .map_err(|e| diagnostic(&e)),
            history: journal::diagnose(host)
                .map(|rows| {
                    (
                        rows.len(),
                        rows.iter().filter(|r| r.blocks_writes()).count(),
                    )
                })
                .map_err(|e| diagnostic(&e)),
        })),
        Task::Git(read) => read.perform().map(Output::Git),
        Task::GitPreview(read) => Ok(Output::GitPreview(read.perform())),
        Task::Refresh => load_scoped(host, repository.as_deref(), requested).map(Output::Data),
        Task::Check {
            manifest,
            requested,
        } => {
            let local = LocalConfig::load(host)?;
            let standalone = manifest.is_some();
            let path = match manifest {
                Some(path) => path,
                None => local
                    .select_repository(repository.as_deref())?
                    .join("dort.toml"),
            };
            let report = Box::new(crate::services::check_manifest(
                host, &local, path, &requested,
            )?);
            if report.selected_tools == 0 {
                return Ok(Output::Checked { report, standalone });
            }
            let tools = crate::services::declared_tools::Input {
                manifest: report.manifest.clone(),
                profiles: requested,
                detection: crate::environment::inspection_inputs(),
            }
            .prepare_for_tools(host, true)?;
            super::full_check::Preview {
                tools,
                destination: super::full_check::Destination::Manifest(report, standalone),
            }
            .finish_or_review(host)
        }
        Task::Profiles {
            manifest,
            requested,
            apply,
        } => {
            let local = LocalConfig::load(host)?;
            let standalone = manifest.is_some();
            let path = match manifest {
                Some(path) => path,
                None => local
                    .select_repository(repository.as_deref())?
                    .join("dort.toml"),
            };
            let report = Box::new(if apply {
                crate::services::check_manifest(host, &local, path, &requested)?
            } else {
                crate::services::profile_picker(host, &local, path, &requested)?
            });
            Ok(Output::Profiles {
                report,
                requested,
                standalone,
                apply,
            })
        }
        Task::Discover => {
            let local = LocalConfig::load(host)?;
            crate::services::discover(host, &local, repository.as_deref()).map(Output::Discovery)
        }
        Task::Select(explicit) => {
            let local = LocalConfig::load(host)?;
            let root = local.select_repository(explicit.as_deref())?;
            dort_core::observation::validate_output_path(&root)?;
            if !root.is_dir() {
                return Err(Error::Invalid(
                    "repository selection must be an existing directory".into(),
                ));
            }
            let data = load_scoped(host, Some(&root), &[])?;
            Ok(Output::Selected(explicit.map(|_| root), data))
        }
        Task::Configuration(request) => {
            use super::configuration::{Preview, Request};
            let preview = match *request {
                Request::Manifest(change) => {
                    let root = LocalConfig::load(host)?.select_repository(repository.as_deref())?;
                    let preview = dort_core::manifest_edit::Preview::prepare(host, &root, &change)?;
                    if matches!(change, dort_core::manifest_edit::Change::File(_))
                        && preview.unchanged()
                    {
                        return Ok(Output::MappingUnchanged);
                    }
                    Preview::Manifest(Box::new(preview))
                }
                Request::OpenRepair(root, git) => {
                    return dort_core::manifest_edit::RepairDraft::open(host, &root, &git)
                        .map(|draft| Output::RepairDraft(Box::new(draft)));
                }
                Request::Repair(draft, replacement) => {
                    Preview::Repair(Box::new(draft.review(host, replacement)?))
                }
                Request::Registry(change) => Preview::Registry(Box::new(
                    dort_core::local::registry_edit::Preview::prepare(host, change)?,
                )),
            };
            Ok(Output::Preview(Pending::Configuration(preview)))
        }
        Task::Profile(root, name, choice) => {
            dort_core::local::ProfilePreview::prepare(host, &root, &name, choice)
                .map(|p| Output::Preview(Pending::Profile(Box::new(p))))
        }
        Task::Clone { source, root, git } => {
            bootstrap::ClonePreview::prepare(host, &source, &root, &git)
                .map(|p| Output::Preview(Pending::Clone(Box::new(p))))
        }
        Task::Init { root, git } => BootstrapPreview::prepare(host, &root, &git)
            .map(|p| Output::Preview(Pending::Init(Box::new(p)))),
        Task::Batch(request) => {
            let local = LocalConfig::load(host)?;
            crate::services::batch_plan(host, &local, repository.as_deref(), requested, request)
                .map(|p| Output::Preview(Pending::Batch(Box::new(p))))
        }
        Task::Files(request) => {
            let local = LocalConfig::load(host)?;
            let snapshot =
                crate::services::observe(host, &local, repository.as_deref(), requested, request)?;
            Ok(Output::Preview(Pending::Files(Box::new(planner::plan(
                snapshot,
            )))))
        }
        Task::Rollback(r, t) => {
            recovery::preview(host, &r, &t).map(|p| Output::Preview(Pending::Rollback(p)))
        }
        Task::Bootstrap(r, t, action) => bootstrap::recovery_preview(host, &r, &t, action)
            .map(|p| Output::Preview(Pending::Bootstrap(p))),
        Task::Execute(approval) => approval.execute(host, observer),
    }
}
