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

pub struct Data {
    pub overview: String,
    pub repository: Option<PathBuf>,
    pub known_roots: Vec<PathBuf>,
    pub profiles: Vec<String>,
    pub files: Vec<(String, String)>,
    pub history: Vec<journal::Diagnosis>,
    pub history_error: Option<String>,
}

pub fn load(host: &HostContext, repository: Option<&Path>) -> Result<Data> {
    let (history, history_error) = match journal::diagnose(host) {
        Ok(rows) => (rows, None),
        Err(error) => (vec![], Some(error.to_string())),
    };
    let mut overview = format!(
        "Experimental local-file manager\nHost: {:?} / {:?}\nWrite blockers: {}\n",
        host.platform,
        host.architecture,
        history.iter().filter(|d| d.blocks_writes()).count()
    );
    if let Some(error) = &history_error {
        overview.push_str(&format!("History diagnosis failed: {error}\n"));
    }
    let local = LocalConfig::load(host);
    let mut files = vec![];
    let mut known_roots = vec![];
    let mut profiles = vec![];
    let mut selected = None;
    match local {
        Err(error) => overview.push_str(&format!(
            "Configuration unavailable: {error}\nRecovery remains available.\n"
        )),
        Ok(local) => {
            known_roots = local
                .repositories
                .values()
                .map(|record| record.root.clone())
                .collect();
            match local.select_repository(repository) {
                Err(error) => overview.push_str(&format!(
                    "{error}\nWelcome: open Actions to Choose repository or Create/attach.\n"
                )),
                Ok(root) => {
                    let registered = known_roots.contains(&root);
                    overview.push_str(&format!(
                        "Repository: {root:?}\nRegistration: {}\n",
                        if registered {
                            "registered"
                        } else {
                            "unregistered (attach before writing)"
                        }
                    ));
                    selected = Some(root.clone());
                    match planner::observe(host, &root, &local.active_profiles, Request::Apply) {
                        Err(error) => {
                            overview.push_str(&format!("File observation failed: {error}\n"))
                        }
                        Ok(snapshot) => {
                            profiles = snapshot.active_profiles.iter().cloned().collect();
                            overview.push_str(&format!(
                                "Effective profiles: {:?}\n",
                                snapshot.active_profiles
                            ));
                            let plan = planner::plan(snapshot);
                            overview.push_str(&format!(
                                "Active files: {}\nPlan blockers: {}\n",
                                plan.files.len(),
                                plan.has_blockers()
                            ));
                            for (file, input) in plan.files.iter().zip(&plan.inputs.files) {
                                files.push((
                                    file.id.clone(),
                                    format!(
                                        "{} [{:?}]\nSource: {:?}\nTarget: {:?}\n{}",
                                        file.id,
                                        file.outcome,
                                        input.source_path,
                                        input.target_path,
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
    overview.push_str("\nActions: Choose repository | Create/attach | Preview apply\nNo tool execution, templates, secrets, network, staging or commits.\nPreviews never authorize execution. Failed writes may retain partial effects.\n");
    Ok(Data {
        overview,
        repository: selected,
        known_roots,
        profiles,
        files,
        history,
        history_error,
    })
}

pub enum Pending {
    Files(Box<Plan>),
    Init(Box<BootstrapPreview>),
    Rollback(recovery::Preview),
    Bootstrap(bootstrap::RecoveryPreview),
}
impl Pending {
    pub fn report(&self) -> Result<String> {
        Ok(match self {
            Self::Files(plan) => plan.human(),
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
                "Rollback schema {}\nRepository ID: {}\nTransaction ID: {}\nJournal head: {}\nInverse paths:\n{}\nLater edits can block execution; retained sources/backups are not deleted.",
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
                "{} bootstrap\nRepository: {:?}\nRepository ID: {}\nTransaction ID: {}\nRepository published: {}\nRegistration published: {}\nEffects:\n- {}\nAbandon retains published repositories, registration and private artifacts.",
                match preview.action {
                    bootstrap::RecoveryAction::Resume => "Resume",
                    bootstrap::RecoveryAction::Abandon => "Abandon",
                },
                preview.repository,
                preview.repository_id,
                preview.transaction_id,
                preview.repository_published,
                preview.registration_published,
                preview.effects.join("\n- ")
            ),
        })
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
        matches!(self, Self::Files(plan) if plan.has_blockers())
    }
    pub fn approve(self, removal: Option<&str>) -> Result<Approved> {
        match self {
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

pub enum Task {
    Refresh,
    Select(Option<PathBuf>),
    Init { root: PathBuf, git: PathBuf },
    Files(Request),
    Rollback(String, String),
    Bootstrap(String, String, bootstrap::RecoveryAction),
    Execute(Approved),
}
pub enum Output {
    Data(Data),
    Selected(Option<PathBuf>, Data),
    Preview(Pending),
    Completed(Completion),
}
pub fn perform(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Output> {
    match task {
        Task::Refresh => load(host, repository.as_deref()).map(Output::Data),
        Task::Select(explicit) => {
            let local = LocalConfig::load(host)?;
            let root = local.select_repository(explicit.as_deref())?;
            dort_core::observation::validate_output_path(&root)?;
            if !root.is_dir() {
                return Err(Error::Invalid(
                    "repository selection must be an existing directory".into(),
                ));
            }
            let data = load(host, Some(&root))?;
            Ok(Output::Selected(explicit.map(|_| root), data))
        }
        Task::Init { root, git } => BootstrapPreview::prepare(host, &root, &git)
            .map(|p| Output::Preview(Pending::Init(Box::new(p)))),
        Task::Files(request) => {
            let local = LocalConfig::load(host)?;
            let root = local.select_repository(repository.as_deref())?;
            let snapshot = planner::observe(host, &root, &local.active_profiles, request)?;
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
