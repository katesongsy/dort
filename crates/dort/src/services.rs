//! Shared application dispatch; presentation must obtain approval before calling.
use dort_core::{
    Result, adoption,
    context::HostContext,
    lifecycle,
    planner::{Plan, Request, UnmanageMode},
    state::WriterLock,
};

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
