use super::actions::{Output, Pending};
use crate::services::{CheckReport, declared_tools, presentation};
use dort_core::{Result, context::HostContext, environment};
use std::path::PathBuf;

pub enum Destination {
    Manifest(Box<CheckReport>, bool),
    Environment {
        root: PathBuf,
        profiles: Vec<String>,
        resume: bool,
    },
}

pub struct Preview {
    pub tools: declared_tools::CheckPreview,
    pub destination: Destination,
}

pub struct Approval {
    tools: declared_tools::CheckApproval,
    destination: Destination,
}

impl Preview {
    pub fn scope(&self) -> (&std::path::Path, &std::collections::BTreeSet<String>) {
        let review = self.tools.review();
        (&review.scope.manifest, &review.scope.active_profiles)
    }

    pub fn finish_or_review(self, host: &HostContext) -> Result<Output> {
        if self.tools.review().inspections.is_empty()
            && matches!(self.destination, Destination::Manifest(..))
        {
            self.approve().execute(host)
        } else {
            Ok(Output::Preview(Pending::FullCheck(Box::new(self))))
        }
    }

    pub fn cancel(&self) -> Option<super::actions::Task> {
        match self.destination {
            Destination::Environment { resume, .. } => {
                Some(super::actions::Task::Environment { resume })
            }
            Destination::Manifest(..) => None,
        }
    }

    pub fn human(&self) -> String {
        let heading = match self.destination {
            Destination::Manifest(..) => "Check manifest and declared tools",
            Destination::Environment { .. } => "Check environment",
        };
        format!(
            "{heading}\n\n{}\n\nOnly checks run. Applying files or installing packages needs separate approval.",
            presentation::check_scope(&self.tools.review())
        )
    }

    pub fn approve(self) -> Approval {
        Approval {
            tools: self.tools.approve(),
            destination: self.destination,
        }
    }
}

impl Approval {
    pub fn execute(self, host: &HostContext) -> Result<Output> {
        let checked = self.tools.check(host)?;
        match self.destination {
            Destination::Manifest(mut report, standalone) => {
                report
                    .unsupported_capabilities
                    .retain(|limit| limit != crate::services::TOOL_CHECK_LIMIT);
                Ok(Output::ManifestChecked {
                    report,
                    standalone,
                    tools: checked
                        .reconciliation
                        .plan
                        .rows
                        .iter()
                        .map(|row| (row.id.clone(), declared_tools::status(row).label()))
                        .collect(),
                })
            }
            Destination::Environment {
                root,
                profiles,
                resume,
            } => environment::Preview::prepare(host, &root, &profiles, resume)
                .map(|preview| Output::Preview(Pending::Environment(Box::new(preview)))),
        }
    }
}
