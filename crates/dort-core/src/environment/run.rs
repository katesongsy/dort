use super::*;
use crate::lifecycle;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum StepState {
    Pending,
    InProgress {
        check: String,
    },
    Confirmed {
        proof: String,
        operation: Option<String>,
    },
    Uncertain {
        check: String,
    },
}
impl StepState {
    fn label(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Confirmed { .. } => "Confirmed",
            Self::Uncertain { .. } | Self::InProgress { .. } => "Uncertain",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Step {
    item: Item,
    approved: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Entry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination: Option<Entry>,
    progress: StepState,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Run {
    id: String,
    input: String,
    steps: Vec<Step>,
    problem: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) abandoned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) stale_check: Option<String>,
}
impl Run {
    pub fn latest(host: &HostContext, root: &Path) -> Result<Option<Self>> {
        let local = LocalConfig::load(host)?;
        let id = local
            .repositories
            .iter()
            .find(|(_, record)| record.root == root)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("Choose a registered repository."))?;
        directory(host, id, false)?
            .map(|dir| read(&dir, "run.json"))
            .transpose()
            .map(Option::flatten)
    }

    fn uncertain_tool(&self) -> bool {
        self.steps.iter().any(|step| {
            matches!(step.item, Item::Tool(_))
                && matches!(
                    step.progress,
                    StepState::InProgress { .. } | StepState::Uncertain { .. }
                )
        })
    }

    pub(super) fn review_resume(&self, plan: &Plan) -> Vec<String> {
        if self.abandoned {
            return vec!["This run was abandoned. Review a new environment plan.".into()];
        }
        if self.complete() {
            return vec!["The last run is complete. Review a new plan.".into()];
        }
        if self.input != plan.input.stamp || self.steps.len() != plan.rows.len() {
            return vec!["Manifest or profile choices changed. Review a new plan.".into()];
        }
        self.steps
            .iter()
            .zip(&plan.rows)
            .filter_map(|(step, row)| {
                verify(step, row, plan.check.as_ref())
                    .err()
                    .map(|error| error.to_string())
            })
            .collect()
    }

    pub fn abandoned(&self) -> bool {
        self.abandoned
    }

    pub fn complete(&self) -> bool {
        self.problem.is_none()
            && self
                .steps
                .iter()
                .all(|step| matches!(step.progress, StepState::Confirmed { .. }))
    }
    pub fn human(&self) -> String {
        let mut text = format!(
            "Environment run {}{}\n",
            self.id,
            if self.abandoned { " (abandoned)" } else { "" }
        );
        for step in &self.steps {
            text.push_str(&format!("{}: {}\n", step.item.id(), step.progress.label()));
        }
        if let Some(problem) = &self.problem {
            text.push_str(problem);
        }
        text
    }
}

#[derive(Serialize)]
pub struct AbandonPreview {
    repository: PathBuf,
    archive: PathBuf,
    run: Run,
    new_stale_check: Option<String>,
    #[serde(skip)]
    original: String,
}

impl AbandonPreview {
    pub fn prepare(host: &HostContext, root: &Path) -> Result<Self> {
        let local = LocalConfig::load(host)?;
        let id = local
            .repositories
            .iter()
            .find(|(_, record)| record.root == root)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("Choose a registered repository."))?;
        let dir =
            directory(host, id, false)?.ok_or_else(|| invalid("No unfinished environment run."))?;
        if observation::inspect(&dir.path.join("run.json"), false)?.kind == Kind::Missing {
            return Err(invalid("No unfinished environment run."));
        }
        let original = String::from_utf8(journal::read_bytes(&dir, "run.json")?)
            .map_err(|_| invalid("Cannot read the saved environment record."))?;
        let run: Run = serde_json::from_str(&original)
            .map_err(|_| invalid("Cannot read the saved environment record."))?;
        if run.complete() || run.abandoned {
            return Err(invalid("No unfinished environment run."));
        }
        let new_stale_check = if run.uncertain_tool() {
            read::<Check>(&dir, "check.json")?.map(|c| c.generation)
        } else {
            run.stale_check.clone()
        };
        Ok(Self {
            repository: root.to_owned(),
            archive: dir.path.join(format!(
                "abandoned-{}.json",
                journal::digest(original.as_bytes())
            )),
            run,
            new_stale_check,
            original,
        })
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn human(&self) -> String {
        format!(
            "Repository: {:?}\n\n{}\n\nKeep the old record at {:?} and mark this run abandoned.\nKeep completed effects in place. No file or package is changed.\nBefore retrying an uncertain install, close any unresolved record in Tools History, run Environment check, then review a new plan.",
            self.repository,
            self.run.human(),
            self.archive
        )
    }

    pub fn approve(self) -> AbandonApproval {
        AbandonApproval(self)
    }
}

pub struct AbandonApproval(AbandonPreview);

impl AbandonApproval {
    pub fn execute(self, host: &HostContext) -> Result<Run> {
        let _lock = WriterLock::acquire(host)?;
        let p = self.0;
        let fresh = AbandonPreview::prepare(host, &p.repository)?;
        if digest(&p)? != digest(&fresh)? {
            return Err(invalid(
                "Environment run or check changed. Review abandonment again.",
            ));
        }
        let dir = PrivateDir::open(p.archive.parent().expect("archive has parent"), false)?
            .ok_or_else(|| invalid("Environment record directory is missing."))?;
        let name = p
            .archive
            .file_name()
            .and_then(|s| s.to_str())
            .expect("generated archive name");
        dir.replace(name, &p.original)?;
        let mut run = p.run;
        run.abandoned = true;
        run.stale_check = p.new_stale_check;
        save(&dir, "run.json", &run)?;
        Ok(run)
    }
}

impl PreparedRow {
    pub(super) fn step(&self) -> Result<Step> {
        let approved = if let Some(p) = &self.install {
            install::preview_binding(p)?
        } else if self.file.is_some() {
            digest(&self.row)?
        } else {
            self.proof.clone().unwrap_or_default()
        };
        Ok(Step {
            item: self.row.item.clone(),
            approved,
            source: self
                .file
                .as_ref()
                .and_then(|f| f.source.clone())
                .map(Entry::stable_parents),
            destination: self
                .file
                .as_ref()
                .and_then(|f| f.destination.clone())
                .map(Entry::stable_parents),
            progress: StepState::Pending,
        })
    }
}

pub struct Approval(Preview);

impl Approval {
    pub(super) fn new(preview: Preview) -> Self {
        Self(preview)
    }
}

fn same_destination(old: &Entry, fresh: &Entry) -> bool {
    if old.kind != Kind::Missing {
        return same(old, fresh);
    }
    old.path == fresh.path
        && fresh.kind == Kind::Missing
        && old.parents.len() <= fresh.parents.len()
        && old.parents.iter().zip(&fresh.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}

fn verify(step: &Step, row: &PreparedRow, check: Option<&Check>) -> Result<()> {
    let changed = || invalid(format!("{} changed. Review a new plan.", step.item.id()));
    if step.item != row.row.item {
        return Err(changed());
    }
    if !row.row.blockers.is_empty()
        && !matches!(
            step.progress,
            StepState::InProgress { .. } | StepState::Uncertain { .. }
        )
    {
        return Err(changed());
    }
    if let Some(file) = &row.file
        && !step
            .source
            .as_ref()
            .zip(file.source.as_ref())
            .is_some_and(|(a, b)| same(a, b))
    {
        return Err(changed());
    }
    match &step.progress {
        StepState::Pending => {
            if let Some(file) = &row.file {
                if step.approved == row.step()?.approved
                    && step
                        .destination
                        .as_ref()
                        .zip(file.destination.as_ref())
                        .is_some_and(|(a, b)| same_destination(a, b))
                {
                    return Ok(());
                }
            } else if step.approved == row.step()?.approved {
                return Ok(());
            }
            Err(changed())
        }
        StepState::Confirmed { proof, .. } if row.proof.as_ref() == Some(proof) => Ok(()),
        StepState::Confirmed { .. } => Err(changed()),
        StepState::InProgress { check: before } | StepState::Uncertain { check: before } => {
            let fresh_check = check.is_some_and(|check| check.generation != *before);
            if row.proof.is_some() && (matches!(step.item, Item::File(_)) || fresh_check) {
                Ok(())
            } else {
                Err(invalid(format!(
                    "{} is uncertain. Check or repair it before resuming.",
                    step.item.id()
                )))
            }
        }
    }
}

fn refresh(host: &HostContext, input: &Inputs, check: &mut Check) -> Result<()> {
    for catalog in &mut check.catalogs {
        if !catalog.current() {
            return Err(invalid(
                "Package manager changed. Run Environment check and review again.",
            ));
        }
        let report = catalog.scope.preview(&host.home)?.approve().inspect();
        if !catalog.current() {
            return Err(invalid("Package manager changed during the check."));
        }
        catalog.observation = report.observation;
    }
    let current = Inputs::read(
        host,
        &input.files.repository,
        &input.files.temporary_profiles,
    )?;
    if current.stamp != input.stamp {
        return Err(invalid(
            "Manifest or profile choices changed. Review a new plan.",
        ));
    }
    check.when = now();
    Ok(())
}

impl Approval {
    pub fn execute(
        self,
        host: &HostContext,
        mut observer: impl FnMut(&adoption::Event) -> Result<()>,
    ) -> Result<Run> {
        let lock = WriterLock::acquire(host)?;
        journal::ensure_writable(host)?;
        let p = self.0;
        let root = &p.plan.input.files.repository;
        let temporary = &p.plan.input.files.temporary_profiles;
        let fresh = Preview::prepare(host, root, temporary, p.resume)?;
        if fresh.blocked() || fresh.binding()? != p.binding()? {
            return Err(invalid("Environment review changed. Review again."));
        }
        let dir = directory(host, &fresh.plan.input.repository_id, true)?
            .expect("created environment directory");
        let input = fresh.plan.input.stamp.clone();
        let mut run = if p.resume {
            fresh.previous.ok_or_else(|| invalid("No run to resume."))?
        } else {
            Run {
                id: adoption::transaction_id(),
                input: input.clone(),
                steps: fresh
                    .plan
                    .rows
                    .iter()
                    .map(PreparedRow::step)
                    .collect::<Result<_>>()?,
                problem: None,
                abandoned: false,
                stale_check: fresh.previous.and_then(|run| run.stale_check),
            }
        };
        run.problem = None;
        save(&dir, "run.json", &run)?;
        let mut check = fresh.plan.check;
        for idx in 0..run.steps.len() {
            let result = (|| {
                let current = Inputs::read(host, root, temporary)?;
                if current.stamp != input {
                    return Err(invalid(
                        "Manifest or profile choices changed. Review a new plan.",
                    ));
                }
                if let Some(check) = &mut check {
                    refresh(host, &current, check)?;
                }
                let current =
                    Plan::from_check(host, Inputs::read(host, root, temporary)?, check.clone())?;
                if current.input.stamp != input || current.rows.len() != run.steps.len() {
                    return Err(invalid("Environment inputs changed. Review a new plan."));
                }
                for (step, row) in run.steps.iter().zip(&current.rows) {
                    verify(step, row, check.as_ref())?;
                }
                let step = &run.steps[idx];
                let row = &current.rows[idx];
                if matches!(step.progress, StepState::Confirmed { .. }) {
                    return Ok(());
                }
                if let Some(proof) = &row.proof {
                    if matches!(
                        step.progress,
                        StepState::InProgress { .. } | StepState::Uncertain { .. }
                    ) && p.plan.rows[idx].proof.as_ref() != Some(proof)
                    {
                        return Err(invalid(
                            "The checked version or file changed. Review again.",
                        ));
                    }
                    run.steps[idx].progress = StepState::Confirmed {
                        proof: proof.clone(),
                        operation: None,
                    };
                    return save(&dir, "run.json", &run);
                }
                let check_id = check
                    .as_ref()
                    .map(|c| c.generation.clone())
                    .unwrap_or_default();
                run.steps[idx].progress = StepState::InProgress {
                    check: check_id.clone(),
                };
                save(&dir, "run.json", &run)?;
                let mut rows = current.rows;
                let row = rows.swap_remove(idx);
                let operation = match (row.row.item, row.install) {
                    (Item::Tool(_), Some(install)) => {
                        let outcome = install::execute(host, &lock, install.approve(), |_| ())?;
                        if !outcome.no_op
                            && !outcome.outcome.is_some_and(install::Outcome::succeeded)
                        {
                            return Err(invalid(
                                "Install result is uncertain. Check or repair the tool before resuming.",
                            ));
                        }
                        outcome.attempt_id
                    }
                    (Item::File(id), None) => {
                        let mut snapshot = current.input.files;
                        snapshot.files.retain(|file| file.mapping.id == id);
                        lifecycle::execute_environment(
                            host,
                            &lock,
                            planner::plan_environment(snapshot),
                            &id,
                            &mut observer,
                        )?
                    }
                    _ => return Err(invalid("No approved step is available. Review again.")),
                };
                let after = Inputs::read(host, root, temporary)?;
                if after.stamp != input {
                    return Err(invalid("Environment inputs changed while the step ran."));
                }
                if matches!(run.steps[idx].item, Item::Tool(_))
                    && let Some(check) = &mut check
                {
                    refresh(host, &after, check)?;
                }
                let confirmed = Plan::from_check(host, after, check.clone())?;
                let proof = confirmed
                    .rows
                    .get(idx)
                    .and_then(|row| row.proof.clone())
                    .ok_or_else(|| {
                        invalid("Result is uncertain. Check or repair the step before resuming.")
                    })?;
                if let Some(check) = &check {
                    save(&dir, "check.json", check)?;
                }
                run.steps[idx].progress = StepState::Confirmed { proof, operation };
                save(&dir, "run.json", &run)
            })();
            if let Err(error) = result {
                if let StepState::InProgress { check } = &run.steps[idx].progress {
                    run.steps[idx].progress = StepState::Uncertain {
                        check: check.clone(),
                    };
                }
                let diagnostic = match error {
                    Error::Parse(_) => {
                        "Manifest became invalid. Fix dort.toml and review again.".into()
                    }
                    error => error.to_string(),
                };
                run.problem = Some(diagnostic);
                save(&dir, "run.json", &run)?;
                break;
            }
        }
        Ok(run)
    }
}
