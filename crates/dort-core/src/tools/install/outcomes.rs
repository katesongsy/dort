//! Pure interpretation of historical metadata, never execution/receipt authority.
use super::{Attempt, BinaryState, Outcome, PackageState, ProcessOutcome, Report, Scope};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unresolved,
    ExplicitlyClosed,
    NoOp,
    ExactReported,
    CurrentReported,
    ProcessFailed,
    ProcessUnavailable,
    ProcessUnverified,
    DeadlineElapsed,
    PackageAbsent,
    PackageConflict,
    PackageUnavailable,
    OutcomeUnavailable,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unresolved => "UNRESOLVED",
            Self::ExplicitlyClosed => "Explicitly closed",
            Self::NoOp => "No installer ran",
            Self::ExactReported => "Exact package reported",
            Self::CurrentReported => "Current package reported",
            Self::ProcessFailed => "Process failed",
            Self::ProcessUnavailable => "Installer unavailable",
            Self::ProcessUnverified => "Process status unknown",
            Self::DeadlineElapsed => "Deadline elapsed",
            Self::PackageAbsent => "Package absent",
            Self::PackageConflict => "Package conflict",
            Self::PackageUnavailable => "Package unknown",
            Self::OutcomeUnavailable => "Outcome unknown",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    ReviewClosure,
    FreshReview,
}
impl NextStep {
    pub fn hint(self) -> &'static str {
        match self {
            Self::ReviewClosure => {
                "Inspect attempt metadata; unresolved records need fresh-query closure before another install. Closure is not success, repair or proof of root termination."
            }
            Self::FreshReview => {
                "For another install, separately approve fresh inventories, explicitly bind every necessary snapshot, then approve a new install review. Nothing is replayed."
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub status: Status,
    pub process: Option<ProcessOutcome>,
    pub package: Option<PackageState>,
    pub binary: Option<BinaryState>,
    pub explanation: &'static str,
    pub binary_note: &'static str,
    pub next_step: NextStep,
    pub next_step_hint: &'static str,
    pub readiness_verified: bool,
}
impl Summary {
    pub fn package_note(self) -> &'static str {
        match self.package {
            Some(PackageState::ExactPresent) => "Package: exact version reported present",
            Some(PackageState::ProviderCurrentPresent) => {
                "Package: provider-current reported present"
            }
            Some(PackageState::Absent) => "Package: reported absent",
            Some(PackageState::Conflict) => "Package: conflicting observation",
            Some(PackageState::Unavailable) => "Package: observation unavailable",
            None if self.status == Status::NoOp => "Matching package; no installer ran",
            None => "Package: no usable outcome",
        }
    }
    pub fn binary_label(self) -> &'static str {
        match self.binary {
            Some(BinaryState::Missing) => "Binary missing; readiness unknown",
            Some(BinaryState::Candidates) => "Binary candidates; readiness/ownership unknown",
            Some(BinaryState::Unavailable) => "Binary observation unavailable",
            None => "Binary readiness and ownership unverified",
        }
    }
    pub fn process_label(self) -> &'static str {
        match self.process {
            Some(ProcessOutcome::Success) => "Process completed",
            Some(ProcessOutcome::Failed) => "Unsuccessful exit",
            Some(ProcessOutcome::Unavailable) => "Installer unavailable",
            Some(ProcessOutcome::Pipe) => "Process I/O unavailable",
            Some(ProcessOutcome::Wait) => "Process status unavailable",
            Some(ProcessOutcome::Timeout) => "Requested deadline elapsed",
            None if self.status == Status::NoOp => "No installer requested",
            None => "No usable process outcome",
        }
    }
    fn new(status: Status, outcome: Option<Outcome>) -> Self {
        let next_step = if matches!(status, Status::Unresolved | Status::OutcomeUnavailable) {
            NextStep::ReviewClosure
        } else {
            NextStep::FreshReview
        };
        let binary = outcome.map(|o| o.binary);
        Self {
            status,
            process: outcome.map(|o| o.process),
            package: outcome.map(|o| o.package),
            binary,
            explanation: match status {
                Status::Unresolved => {
                    "Attempt metadata is incomplete, malformed or changed; later Tools installs remain blocked."
                }
                Status::ExplicitlyClosed => {
                    "Only unchanged historical metadata was acknowledged after fresh inspection; no install success, repair or root termination is implied."
                }
                Status::NoOp => {
                    "A matching package was reported by fresh inspection; no installer or attempt was created."
                }
                Status::ExactReported => {
                    "The process completed and the exact requested package version was reported; this is not verified binary readiness or ownership."
                }
                Status::CurrentReported => {
                    "The process completed and a provider-current package was reported, not an exact pin; this is not verified binary readiness or ownership."
                }
                Status::ProcessFailed => match outcome.map(|o| o.package) {
                    Some(PackageState::ExactPresent | PackageState::ProviderCurrentPresent) => {
                        "The process did not complete successfully even though a package was reported present. Native effects may be partial or ongoing; presence does not turn failure into success."
                    }
                    Some(PackageState::Absent) => {
                        "The process did not complete successfully and the package was reported absent. Native effects may be partial or ongoing; no rollback is implied."
                    }
                    _ => {
                        "The process did not complete successfully; the package is conflicting or unavailable. Native effects may be partial or ongoing; no rollback is implied."
                    }
                },
                Status::ProcessUnavailable => {
                    "The installer could not be started. Earlier provider queries may have native cache effects; package presence does not authorize replay."
                }
                Status::ProcessUnverified => {
                    "The process outcome could not be established; package presence alone is not process success. Native effects may be partial or ongoing; no rollback is implied."
                }
                Status::DeadlineElapsed => {
                    "The requested deadline elapsed without verified success. Effects may be partial or ongoing; no privileged-termination or rollback proof is implied."
                }
                Status::PackageAbsent => {
                    "The process completed but the requested package was reported absent; installation is not established and partial effects may remain."
                }
                Status::PackageConflict => {
                    "The process completed but the observed package conflicts with the request; installation is not established and partial effects may remain."
                }
                Status::PackageUnavailable => {
                    "The process completed but the package observation is unavailable, not healthy empty or verified success; partial effects may remain."
                }
                Status::OutcomeUnavailable => {
                    "No usable outcome was reported; inspect attempt metadata before considering another install."
                }
            },
            binary_note: match binary {
                Some(BinaryState::Missing) => {
                    "Declared binary missing; package presence does not establish readiness."
                }
                Some(BinaryState::Candidates) => {
                    "Binary candidates observed as metadata only; none was executed or verified ready/owned."
                }
                Some(BinaryState::Unavailable) => {
                    "Binary observation unavailable, not a healthy empty result or verified readiness."
                }
                None => "Binary state not reported here; readiness and ownership are not verified.",
            },
            next_step,
            next_step_hint: next_step.hint(),
            readiness_verified: false,
        }
    }
}
impl Outcome {
    pub fn succeeded(self) -> bool {
        self.process == ProcessOutcome::Success
            && matches!(
                self.package,
                PackageState::ExactPresent | PackageState::ProviderCurrentPresent
            )
    }
    pub fn summary(self) -> Summary {
        let status = match self.process {
            ProcessOutcome::Failed => Status::ProcessFailed,
            ProcessOutcome::Unavailable => Status::ProcessUnavailable,
            ProcessOutcome::Pipe | ProcessOutcome::Wait => Status::ProcessUnverified,
            ProcessOutcome::Timeout => Status::DeadlineElapsed,
            ProcessOutcome::Success => match self.package {
                PackageState::ExactPresent => Status::ExactReported,
                PackageState::ProviderCurrentPresent => Status::CurrentReported,
                PackageState::Absent => Status::PackageAbsent,
                PackageState::Conflict => Status::PackageConflict,
                PackageState::Unavailable => Status::PackageUnavailable,
            },
        };
        Summary::new(status, Some(self))
    }
}
impl Attempt {
    pub fn summary(&self) -> Summary {
        if self.unresolved {
            Summary::new(Status::Unresolved, self.outcome)
        } else if self.explicitly_closed {
            Summary::new(Status::ExplicitlyClosed, self.outcome)
        } else {
            self.outcome
                .map(Outcome::summary)
                .unwrap_or_else(|| Summary::new(Status::OutcomeUnavailable, None))
        }
    }
}
impl Report {
    pub fn summary(&self) -> Summary {
        if self.no_op {
            Summary::new(Status::NoOp, self.outcome)
        } else {
            self.outcome
                .map(Outcome::summary)
                .unwrap_or_else(|| Summary::new(Status::OutcomeUnavailable, None))
        }
    }
}
// Only public reports gain derived fields. Persisted Intent/Completed/Closure,
// their payload bytes, checksums and heads never pass through these views.
impl Serialize for Attempt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            id: &'a str,
            scope: &'a Option<Scope>,
            outcome: Option<Outcome>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            observed_versions: &'a Vec<String>,
            problem: &'a Option<super::Problem>,
            unresolved: bool,
            explicitly_closed: bool,
            summary: Summary,
        }
        View {
            id: &self.id,
            scope: &self.scope,
            outcome: self.outcome,
            observed_versions: &self.observed_versions,
            problem: &self.problem,
            unresolved: self.unresolved,
            explicitly_closed: self.explicitly_closed,
            summary: self.summary(),
        }
        .serialize(serializer)
    }
}
impl Serialize for Report {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            scope: &'a Scope,
            attempt_id: &'a Option<String>,
            no_op: bool,
            outcome: Option<Outcome>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            observed_versions: &'a Vec<String>,
            possible_partial_effects: bool,
            readiness_verified: bool,
            summary: Summary,
        }
        View {
            scope: &self.scope,
            attempt_id: &self.attempt_id,
            no_op: self.no_op,
            outcome: self.outcome,
            observed_versions: &self.observed_versions,
            possible_partial_effects: self.possible_partial_effects,
            readiness_verified: self.readiness_verified,
            summary: self.summary(),
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_process_package_binary_combinations_preserve_independent_facts() {
        for process in [
            ProcessOutcome::Success,
            ProcessOutcome::Failed,
            ProcessOutcome::Unavailable,
            ProcessOutcome::Pipe,
            ProcessOutcome::Wait,
            ProcessOutcome::Timeout,
        ] {
            for package in [
                PackageState::ExactPresent,
                PackageState::ProviderCurrentPresent,
                PackageState::Absent,
                PackageState::Conflict,
                PackageState::Unavailable,
            ] {
                for binary in [
                    BinaryState::Missing,
                    BinaryState::Candidates,
                    BinaryState::Unavailable,
                ] {
                    let o = Outcome {
                        process,
                        package,
                        binary,
                    };
                    let s = o.summary();
                    assert_eq!(
                        (s.process, s.package, s.binary),
                        (Some(process), Some(package), Some(binary))
                    );
                    assert!(!s.readiness_verified);
                    assert_eq!(
                        o.succeeded(),
                        matches!(s.status, Status::ExactReported | Status::CurrentReported)
                    );
                    let expected = match (process, package) {
                        (ProcessOutcome::Success, PackageState::ExactPresent) => {
                            Status::ExactReported
                        }
                        (ProcessOutcome::Success, PackageState::ProviderCurrentPresent) => {
                            Status::CurrentReported
                        }
                        (ProcessOutcome::Success, PackageState::Absent) => Status::PackageAbsent,
                        (ProcessOutcome::Success, PackageState::Conflict) => {
                            Status::PackageConflict
                        }
                        (ProcessOutcome::Success, PackageState::Unavailable) => {
                            Status::PackageUnavailable
                        }
                        (ProcessOutcome::Failed, _) => Status::ProcessFailed,
                        (ProcessOutcome::Unavailable, _) => Status::ProcessUnavailable,
                        (ProcessOutcome::Pipe | ProcessOutcome::Wait, _) => {
                            Status::ProcessUnverified
                        }
                        (ProcessOutcome::Timeout, _) => Status::DeadlineElapsed,
                    };
                    assert_eq!(s.status, expected);
                    if binary == BinaryState::Candidates {
                        assert!(s.binary_note.contains("none was executed"));
                    }
                    if package == PackageState::Unavailable && process == ProcessOutcome::Success {
                        assert_eq!(s.status, Status::PackageUnavailable);
                    }
                    assert!(s.next_step_hint.contains("explicitly bind"));
                }
            }
        }
    }
    #[test]
    fn damaged_and_explicitly_closed_history_never_becomes_install_success() {
        let mut a = Attempt {
            id: "a".repeat(64),
            scope: None,
            outcome: Some(Outcome {
                process: ProcessOutcome::Success,
                package: PackageState::ExactPresent,
                binary: BinaryState::Candidates,
            }),
            observed_versions: vec![],
            problem: Some(super::super::Problem::Malformed),
            unresolved: true,
            explicitly_closed: false,
            head: "hidden".into(),
        };
        assert_eq!(a.summary().status, Status::Unresolved);
        assert_eq!(a.summary().next_step, NextStep::ReviewClosure);
        a.unresolved = false;
        a.explicitly_closed = true;
        assert_eq!(a.summary().status, Status::ExplicitlyClosed);
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("hidden"));
        assert!(json.contains("explicitly_closed") && json.contains("root termination"));
        a.unresolved = true;
        assert_eq!(a.summary().status, Status::Unresolved);
    }
}
