//! UI-independent action preparation and dispatch over the existing core services.
use crate::services::FileApproval;
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
    pub files: Vec<(String, String)>,
    pub history: Vec<journal::Diagnosis>,
    pub history_error: Option<String>,
}

pub fn load(host: &HostContext, repository: Option<&Path>) -> Result<Data> {
    let local = LocalConfig::load(host)?;
    let (history, history_error) = match journal::diagnose(host) {
        Ok(rows) => (rows, None),
        Err(error) => (vec![], Some(error.to_string())),
    };
    let mut overview = format!(
        "Experimental local-file manager\nHost: {:?} / {:?}\n",
        host.platform, host.architecture
    );
    overview.push_str(&format!(
        "Write blockers: {}\n",
        history.iter().filter(|d| d.blocks_writes()).count()
    ));
    if let Some(error) = &history_error {
        overview.push_str(&format!("History diagnosis failed: {error}\n"));
    }
    let mut files = vec![];
    match local.select_repository(repository) {
        Err(error) => overview.push_str(&format!(
            "{error}\nUse dort init PATH outside the TUI to create/attach a repository.\n"
        )),
        Ok(root) => {
            overview.push_str(&format!("Repository: {root:?}\n"));
            match planner::observe(host, &root, &local.active_profiles, Request::Apply) {
                Err(error) => overview.push_str(&format!("File observation failed: {error}\n")),
                Ok(snapshot) => {
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
    overview.push_str("\nNo tools, templates, secrets, network or Git writes.\nPreviews never authorize execution. Failed writes may retain partial effects.\n");
    Ok(Data {
        overview,
        files,
        history,
        history_error,
    })
}

pub enum Pending {
    Files(Box<Plan>),
    Rollback(recovery::Preview),
    Bootstrap(bootstrap::RecoveryPreview),
}
impl Pending {
    pub fn report(&self) -> Result<String> {
        match self {
            Self::Files(plan) => Ok(plan.human()),
            Self::Rollback(preview) => serde_json::to_string_pretty(preview),
            Self::Bootstrap(preview) => serde_json::to_string_pretty(preview),
        }
        .map_err(|_| Error::Invalid("cannot render preview".into()))
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
            Self::Rollback(p) => recovery::Approval::from_preview(p).map(Approved::Rollback),
            Self::Bootstrap(p) => {
                bootstrap::RecoveryApproval::from_preview(p).map(Approved::Bootstrap)
            }
        }
    }
}

pub enum Approved {
    Files(Box<FileApproval>),
    Rollback(recovery::Approval),
    Bootstrap(bootstrap::RecoveryApproval),
}
impl Approved {
    pub fn execute(
        self,
        host: &HostContext,
        observer: impl FnMut(&Event) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::Files(approval) => (*approval).execute(host, observer).map(|_| ()),
            Self::Rollback(approval) => {
                let lock = WriterLock::acquire(host)?;
                recovery::execute_with_observer(host, &lock, approval, observer)
            }
            Self::Bootstrap(approval) => {
                let lock = WriterLock::acquire(host)?;
                bootstrap::recover(host, &lock, approval, observer)
            }
        }
    }
}

pub enum Task {
    Refresh,
    Files(Request),
    Rollback(String, String),
    Bootstrap(String, String, bootstrap::RecoveryAction),
    Execute(Approved),
}
pub enum Output {
    Data(Data),
    Preview(Pending),
    Completed,
}
pub fn perform(
    task: Task,
    host: &HostContext,
    repository: Option<PathBuf>,
    observer: impl FnMut(&Event) -> Result<()>,
) -> Result<Output> {
    match task {
        Task::Refresh => load(host, repository.as_deref()).map(Output::Data),
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
        Task::Execute(approval) => approval.execute(host, observer).map(|_| Output::Completed),
    }
}
