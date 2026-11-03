//! Shared application dispatch; presentation must obtain approval before calling.
use dort_core::{
    Result, adoption, bootstrap,
    context::HostContext,
    lifecycle,
    planner::{Plan, Request, UnmanageMode},
    state::WriterLock,
};

use std::path::{Path, PathBuf};

pub fn resolve_root(path: &Path, launch_directory: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        launch_directory.join(path)
    }
}

/// Own the actual preview and its executable binding, never a serialized report.
pub struct BootstrapPreview {
    pub report: bootstrap::Preview,
    git: PathBuf,
}
impl BootstrapPreview {
    pub fn prepare(host: &HostContext, root: &Path, git: &Path) -> Result<Self> {
        Ok(Self {
            report: bootstrap::preview(host, root, git)?,
            git: git.to_owned(),
        })
    }
    pub fn approve(self) -> Result<BootstrapApproval> {
        Ok(BootstrapApproval {
            approval: bootstrap::Approval::from_preview(self.report)?,
            git: self.git,
        })
    }
}
pub struct BootstrapApproval {
    approval: bootstrap::Approval,
    git: PathBuf,
}
impl BootstrapApproval {
    pub fn execute(
        self,
        host: &HostContext,
        observer: impl FnMut(&adoption::Event) -> Result<()>,
    ) -> Result<String> {
        let lock = WriterLock::acquire(host)?;
        bootstrap::execute(host, &lock, self.approval, &self.git, observer)
    }
}

pub enum FileApproval {
    Adopt(adoption::Approval),
    Lifecycle(lifecycle::Approval),
}
impl FileApproval {
    pub fn from_plan(plan: Plan, removal_ack: Option<&str>) -> Result<Self> {
        match &plan.inputs.request {
            Request::Adopt { .. } => Ok(Self::Adopt(adoption::Approval::from_plan(plan)?)),
            Request::Unmanage {
                id,
                mode: UnmanageMode::Remove,
            } => {
                if removal_ack != Some(id.as_str()) {
                    return Err(dort_core::Error::Invalid(
                        "link deletion requires its exact file ID".into(),
                    ));
                }
                let ack = lifecycle::RemovalAcknowledgement::confirm(id.clone());
                Ok(Self::Lifecycle(lifecycle::Approval::from_removal_plan(
                    plan, ack,
                )?))
            }
            _ => Ok(Self::Lifecycle(lifecycle::Approval::from_plan(plan)?)),
        }
    }
    pub fn execute(
        self,
        host: &HostContext,
        observer: impl FnMut(&adoption::Event) -> Result<()>,
    ) -> Result<Option<String>> {
        let lock = WriterLock::acquire(host)?;
        match self {
            Self::Adopt(approval) => adoption::execute(host, &lock, approval, observer).map(Some),
            Self::Lifecycle(approval) => lifecycle::execute(host, &lock, approval, observer),
        }
    }
}
