//! UI-independent action preparation and dispatch over the existing core services.
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
    pub files: Vec<(String, String)>,
    pub managed: Vec<ManagedRow>,
    pub files_error: Option<String>,
    pub history: Vec<journal::Diagnosis>,
    pub history_error: Option<String>,
}

// TOML parser Display includes source lines. Ordinary TUI errors are metadata only.
pub fn diagnostic(error: &Error) -> String {
    match error {
        Error::Parse(_) => "Invalid TOML; manifest source text is not displayed.".into(),
        _ => error.to_string(),
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
    let mut selected = None;
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
                        "{error}\nChoose repository or Create/attach in Actions."
                    ));
                }
                Ok(root) => {
                    let registered = known_roots.contains(&root);
                    overview.registered = Some(registered);
                    selected = Some(root.clone());
                    match crate::services::observe(
                        host,
                        &local,
                        Some(&root),
                        requested,
                        Request::Apply,
                    ) {
                        Err(error) => {
                            let error = diagnostic(&error);
                            files_error = Some(format!("File observation failed: {error}"));
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
                            for (file, input) in plan.files.iter().zip(&plan.inputs.files) {
                                managed.push(ManagedRow {
                                    id: file.id.clone(),
                                    source: input.source_path.clone(),
                                    target: input.target_path.clone(),
                                    source_kind: entry_kind(input.source.as_ref()),
                                    target_kind: entry_kind(input.destination.as_ref()),
                                    state: match file.outcome {
                                        planner::Outcome::NoOp => "Linked",
                                        planner::Outcome::Change => "Needs apply",
                                        planner::Outcome::Blocked => "Blocked",
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
                }
            }
        }
    }
    Ok(Data {
        overview: Some(overview),
        repository: selected,
        known_roots,
        profiles,
        files,
        managed,
        files_error,
        history,
        history_error,
    })
}

pub enum Pending {
    Files(Box<Plan>),
    Batch(Box<dort_core::batch::Plan>),
    Init(Box<BootstrapPreview>),
    Rollback(recovery::Preview),
    Bootstrap(bootstrap::RecoveryPreview),
}
impl Pending {
    pub fn report(&self) -> Result<String> {
        Ok(match self {
            Self::Batch(plan) => plan.human(),
            Self::Files(plan) => format!(
                "Repository: {:?}\nEffective profiles: {:?}\nScope: all mappings selected by this request; display filters never narrow it.\n{}",
                plan.inputs.repository,
                plan.inputs.active_profiles,
                plan.human()
            ),
            Self::Init(preview) => format!(
                "{} local repository\nResolved path: {:?}\nRepository ID: {}\n\nEffects:\n- {}\n\nRegistration and active selection are persistent only after approval.\nPrivate artifacts are retained on failure; inspect Recovery.\nNo network, cloning or live dotfile changes.",
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
                "Rollback schema {}\nRepository ID: {}\nTransaction ID: {}\nJournal head: {}\nInverse paths:\n{}\nLater edits can block execution. Schema-1 rollback withdraws its new source into private transaction storage; schema-2 leaves repository sources unchanged. Private backups/artifacts are retained; no garbage collection.",
                preview.journal_schema,
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
                "{} bootstrap\nRepository: {:?}\nRepository ID: {}\nTransaction ID: {}\nJournal head: {}\nRepository published: {}\nRegistration published: {}\nEffects:\n- {}\nAbandon retains published repositories, registration and private artifacts. It does not undo publication.\nResume publishes only the exact retained preparation; later changes can block execution. Preparing transactions must be abandoned before retrying Create/attach with a new transaction. No network or live dotfile changes.",
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
            _ => false,
        }
    }
    pub fn approve(self, removal: Option<&str>) -> Result<Approved> {
        match self {
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
    ) -> Result<Completion> {
        let (operation, transaction_id, repository) = match self {
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
                ("Bootstrap recovery", Some(tid), None)
            }
        };
        Ok(Completion {
            operation,
            transaction_id,
            repository,
        })
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
    History,
    Diagnostics,
    Git(super::git::Read),
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
    Execute(Approved),
}
pub enum Output {
    History {
        rows: Vec<journal::Diagnosis>,
        error: Option<String>,
    },
    Diagnostics(Diagnostics),
    Git(super::git::Report),
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
pub fn perform_scoped(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    requested: &[String],
    observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Output> {
    match task {
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
            Ok(Output::Checked { report, standalone })
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
            let report = Box::new(crate::services::check_manifest(
                host, &local, path, &requested,
            )?);
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
        Task::Execute(approval) => approval.execute(host, observer).map(Output::Completed),
    }
}
