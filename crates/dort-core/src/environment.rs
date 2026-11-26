//! Ordered environment reviews, execution, and local run records.
use crate::{
    Error, Manifest, Result, adoption,
    context::HostContext,
    journal,
    local::LocalConfig,
    model::Provider,
    observation::{self, Entry, Kind},
    planner, profiles,
    state::{PrivateDir, WriterLock},
    tools::{VersionMatch, VersionPolicy, binaries, inspection, install, planning},
};
mod run;
pub use run::{AbandonApproval, AbandonPreview, Approval, Run};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const CHECK_MAX_AGE: u64 = 600;
const MAX_STEPS: usize = 128;

fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid(reason.into())
}
fn digest(data: &impl Serialize) -> Result<String> {
    Ok(journal::digest(adoption::canonical(data)?.as_bytes()))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    scope: install::npm::Catalog,
    programs: Vec<Entry>,
    observation: inspection::Observation,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    input: String,
    generation: String,
    when: u64,
    dirs: Vec<PathBuf>,
    catalogs: Vec<Catalog>,
}

struct Inputs {
    manifest: Manifest,
    files: planner::Snapshot,
    stamp: String,
    repository_id: String,
}
impl Inputs {
    fn read(host: &HostContext, root: &Path, temporary: &[String]) -> Result<Self> {
        let (config, local) = LocalConfig::load_bound(host)?;
        let repository_id = config
            .repositories
            .iter()
            .find(|(_, record)| record.root == root)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| invalid("Choose a registered repository."))?;
        let files = planner::observe_environment(host, root, temporary)?;
        let (manifest, entry) = observation::read_manifest(&root.join("dort.toml"))?;
        if !same(&entry, &files.manifest)
            || !adoption::same(&local, &LocalConfig::load_bound(host)?.1, true)
        {
            return Err(invalid(
                "Manifest or profile choices changed. Review again.",
            ));
        }
        let stamp = digest(&(
            host,
            &repository_id,
            entry.stable_parents(),
            local.stable_parents(),
            &files.profiles,
            temporary,
        ))?;
        Ok(Self {
            manifest,
            files,
            stamp,
            repository_id,
        })
    }
}
fn same(a: &Entry, b: &Entry) -> bool {
    a.path == b.path
        && adoption::same(a, b, true)
        && a.parents.len() == b.parents.len()
        && a.parents.iter().zip(&b.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}
fn directory(host: &HostContext, id: &str, create: bool) -> Result<Option<PrivateDir>> {
    let path = crate::local::repository_state_path(host, id)?.join("environment");
    PrivateDir::open(&path, create)
}
fn read<T: for<'a> Deserialize<'a>>(dir: &PrivateDir, name: &str) -> Result<Option<T>> {
    if observation::inspect(&dir.path.join(name), false)?.kind == Kind::Missing {
        return Ok(None);
    }
    let bytes = journal::read_bytes(dir, name)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| invalid("Cannot read the saved environment record."))
}
fn save(dir: &PrivateDir, name: &str, value: &impl Serialize) -> Result<()> {
    let text = journal::encode(value)?;
    if text.len() > 4 * 1024 * 1024 {
        return Err(invalid(
            "Environment record is too large. Choose fewer profiles.",
        ));
    }
    dir.replace(name, &text)
}
fn programs(scope: &install::npm::Catalog) -> Result<Vec<Entry>> {
    std::iter::once(&scope.executable)
        .chain(scope.npm_binding.as_ref().map(|n| &n.npm_cli))
        .map(|p| observation::inspect(p, false).map(Entry::stable_parents))
        .collect()
}
impl Catalog {
    fn current(&self) -> bool {
        programs(&self.scope).is_ok_and(|now| {
            now.len() == self.programs.len()
                && now.iter().zip(&self.programs).all(|(a, b)| same(a, b))
        })
    }
    fn report(&self, home: &Path) -> Result<inspection::Report> {
        Ok(inspection::Report {
            description: self.scope.preview(home)?.description().clone(),
            observation: self.observation.clone(),
        })
    }
}

/// Supplies only observed package metadata, not an executable approval.
pub fn save_check(
    host: &HostContext,
    root: &Path,
    temporary: &[String],
    dirs: Vec<PathBuf>,
    reports: impl IntoIterator<Item = (BTreeSet<String>, inspection::Report)>,
    still_current: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let lock = WriterLock::acquire(host)?;
    lock.check_context(host)?;
    still_current()?;
    let input = Inputs::read(host, root, temporary)?;
    binaries::validate(&input.manifest, &input.files.active_profiles, &dirs)?;
    let mut catalogs = vec![];
    for (tools, report) in reports {
        let d = report.description;
        if d.home != host.home || tools.is_empty() || catalogs.len() >= 16 {
            return Err(invalid("Invalid environment check scope."));
        }
        let scope = install::npm::Catalog {
            query: d.query,
            executable: d.executable,
            search_dirs: d.trusted_search_path,
            npm_binding: d.npm_binding,
            tools,
        };
        scope.preview(&host.home)?;
        catalogs.push(Catalog {
            programs: programs(&scope)?,
            scope,
            observation: report.observation,
        });
    }
    let dir = directory(host, &input.repository_id, true)?.expect("created environment directory");
    save(
        &dir,
        "check.json",
        &Check {
            input: input.stamp,
            generation: adoption::transaction_id(),
            when: now(),
            dirs,
            catalogs,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Item {
    Tool(String),
    File(String),
}
impl Item {
    fn id(&self) -> &str {
        match self {
            Self::Tool(id) | Self::File(id) => id,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Detail {
    Tool {
        provider: Provider,
        package: String,
        destination: Option<PathBuf>,
        version: Option<String>,
        command: Option<Vec<String>>,
    },
    File {
        source: Option<PathBuf>,
        destination: PathBuf,
        effects: Vec<planner::Effect>,
    },
}
#[derive(Serialize)]
struct Row {
    item: Item,
    requires: Vec<String>,
    detail: Detail,
    status: &'static str,
    blockers: Vec<String>,
}
struct PreparedRow {
    row: Row,
    install: Option<install::Preview>,
    proof: Option<String>,
    file: Option<planner::FileSnapshot>,
}
struct Plan {
    rows: Vec<PreparedRow>,
    check: Option<Check>,
    input: Inputs,
}
impl Plan {
    fn prepare(host: &HostContext, root: &Path, temporary: &[String]) -> Result<Self> {
        let input = Inputs::read(host, root, temporary)?;
        let check = directory(host, &input.repository_id, false)?
            .map(|dir| read::<Check>(&dir, "check.json"))
            .transpose()?
            .flatten()
            .filter(|check| {
                check.input == input.stamp
                    && now()
                        .checked_sub(check.when)
                        .is_some_and(|age| age <= CHECK_MAX_AGE)
            });
        Self::from_check(host, input, check)
    }

    fn from_check(host: &HostContext, input: Inputs, check: Option<Check>) -> Result<Self> {
        let active = &input.files.active_profiles;
        let mut reports = vec![];
        let mut catalogs = BTreeMap::new();
        let mut observed = planning::Observations::default();
        if let Some(check) = &check {
            for catalog in check.catalogs.iter().filter(|c| c.current()) {
                let report = catalog.report(&host.home)?;
                catalogs.extend(
                    catalog
                        .scope
                        .tools
                        .iter()
                        .cloned()
                        .map(|id| (id, reports.len())),
                );
                reports.push((catalog, report));
            }
            observed.catalogs = catalogs
                .iter()
                .map(|(id, idx)| (id.clone(), &reports[*idx].1))
                .collect();
            observed.binaries = binaries::observe(&input.manifest, active, &check.dirs)?
                .tools
                .into_iter()
                .map(|(id, tool)| (id, tool.observation))
                .collect();
        }
        let mut rows: Vec<PreparedRow> = vec![];
        let tool_plan = planning::plan(&input.manifest, host.platform, active, &observed)?;
        for tool in &tool_plan.rows {
            let declaration = tool.declaration.as_ref().expect("declared tool");
            let scope = catalogs.get(&tool.id).map(|idx| &reports[*idx].0.scope);
            let destination = scope.and_then(|scope| match &scope.npm_binding {
                Some(npm) => Some(npm.prefix.clone()),
                None => install::brew::prefix_hint(&scope.executable),
            });
            let mut row = Row {
                item: Item::Tool(tool.id.clone()),
                requires: tool.dependencies.clone(),
                detail: Detail::Tool {
                    provider: declaration.provider,
                    package: declaration.package.clone(),
                    destination,
                    version: None,
                    command: None,
                },
                status: "Blocked",
                blockers: vec![],
            };
            for dependency in &tool.dependencies {
                let advice = dependency_block(&rows, &input.manifest, dependency);
                row.blockers.extend(advice);
            }
            let mut installation = None;
            let mut proof = None;
            if row.blockers.is_empty() {
                match &tool.package {
                    planning::PackageFinding::NotObserved | planning::PackageFinding::Unavailable { .. } => {
                        row.status = "Unknown";
                        row.blockers.push("Run Environment check again.".into());
                    }
                    planning::PackageFinding::Present { versions } => {
                        let matching: Vec<_> = versions.iter().filter(|v| v.policy_match == VersionMatch::Satisfied).map(|v| v.version.as_str()).collect();
                        if matching.is_empty() { row.blockers.push("Installed version differs or cannot be verified. Use the package manager outside Dort to select the declared version, or correct the declaration, then run Environment check.".into()); }
                        else if let Some(proven) = scope.and_then(|scope| proof_of_tool(tool, scope, &matching).ok().flatten()) {
                            row.status = "Already correct";
                            if let Detail::Tool { version, .. } = &mut row.detail { *version = Some(matching.join(", ")); }
                            proof = Some(proven);
                        } else { row.blockers.push("The command does not resolve to the checked package. Fix the command or tool search paths outside this run, then run Environment check.".into()); }
                    }
                    planning::PackageFinding::Absent => match prepare_install(host, &input, check.as_ref(), &tool_plan.rows, tool, &reports, &catalogs) {
                        Ok(p) => {
                            row.status = "Install";
                            if let Detail::Tool { command, version, .. } = &mut row.detail {
                                let review = p.review();
                                *command = Some(std::iter::once(review.installer.display().to_string()).chain(review.arguments).collect());
                                *version = Some(review.scope.version.clone());
                            }
                            installation = Some(p);
                        }
                        Err(error) => row.blockers.push(error.to_string()),
                    },
                    _ => row.blockers.push("Package identity is unverified. Check the declaration and run Environment check.".into()),
                }
            }
            rows.push(PreparedRow {
                row,
                install: installation,
                proof,
                file: None,
            });
        }
        let files = planner::plan_environment(input.files.clone());
        for (file, proposed) in files.inputs.files.into_iter().zip(files.files) {
            let mut blockers = proposed.blockers;
            for dependency in &file.mapping.requires {
                blockers.extend(dependency_block(&rows, &input.manifest, dependency));
            }
            let status = if !blockers.is_empty() {
                "Blocked"
            } else if matches!(proposed.outcome, planner::Outcome::NoOp) {
                "Already correct"
            } else {
                "Link"
            };
            let proof = if status == "Already correct" {
                Some(digest(&(
                    file.source.clone().map(Entry::stable_parents),
                    file.destination.clone().map(Entry::stable_parents),
                ))?)
            } else {
                None
            };
            rows.push(PreparedRow {
                row: Row {
                    item: Item::File(file.mapping.id.clone()),
                    requires: file.mapping.requires.clone(),
                    detail: Detail::File {
                        source: file.source_path.clone(),
                        destination: file.target_path.clone(),
                        effects: proposed.effects,
                    },
                    status,
                    blockers,
                },
                install: None,
                proof,
                file: Some(file),
            });
        }
        if rows.len() > MAX_STEPS {
            return Err(invalid(
                "Too many environment steps. Choose fewer profiles.",
            ));
        }
        Ok(Self { rows, check, input })
    }
}

fn dependency_block(rows: &[PreparedRow], manifest: &Manifest, tool: &str) -> Option<String> {
    match rows.iter().find(|r| r.row.item == Item::Tool(tool.into())) {
        Some(row) if row.row.blockers.is_empty() => None,
        Some(_) => Some(format!("Required tool {tool} is blocked.")),
        None => Some(format!(
            "Required tool {tool} is off. Enable one of: {}.",
            manifest
                .tools
                .iter()
                .find(|t| t.id == tool)
                .map(|t| t.profiles.join(", "))
                .unwrap_or_default()
        )),
    }
}

fn proof_of_tool(
    tool: &planning::Row,
    scope: &install::npm::Catalog,
    versions: &[&str],
) -> Result<Option<String>> {
    let planning::BinaryObservation::Candidates { paths } = &tool.binary_observation else {
        return Ok(None);
    };
    let Some(command) = paths.first() else {
        return Ok(None);
    };
    let Some(d) = &tool.declaration else {
        return Ok(None);
    };
    let physical = match command.canonicalize() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let attributed = if let Some(npm) = &scope.npm_binding {
        command == &npm.prefix.join("bin").join(&tool.binary)
            && physical.starts_with(npm.prefix.join("lib/node_modules").join(&d.package))
    } else {
        install::brew::prefix_hint(&scope.executable).is_some_and(|prefix| {
            versions.iter().any(|v| {
                physical.starts_with(prefix.join("Cellar").join(&d.package).join(v).join("bin"))
            })
        })
    };
    if !attributed {
        return Ok(None);
    }
    let entry = observation::inspect(&physical, false)?;
    if entry.kind != Kind::Regular
        || entry
            .fingerprint
            .as_ref()
            .is_none_or(|f| f.mode & 0o111 == 0)
    {
        return Ok(None);
    }
    Ok(Some(digest(&(versions, entry.stable_parents()))?))
}

fn prepare_install(
    host: &HostContext,
    input: &Inputs,
    check: Option<&Check>,
    rows: &[planning::Row],
    tool: &planning::Row,
    reports: &[(&Catalog, inspection::Report)],
    bindings: &BTreeMap<String, usize>,
) -> Result<install::Preview> {
    let check = check.ok_or_else(|| invalid("Run Environment check first."))?;
    let mut ids = BTreeSet::new();
    let mut pending = vec![tool.id.clone()];
    let mut groups = BTreeMap::<usize, BTreeSet<String>>::new();
    while let Some(id) = pending.pop() {
        if !ids.insert(id.clone()) {
            continue;
        }
        let row = rows
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| invalid("A required tool is off."))?;
        pending.extend(row.dependencies.clone());
        groups
            .entry(
                *bindings
                    .get(&id)
                    .ok_or_else(|| invalid("Run Environment check for all required tools."))?,
            )
            .or_default()
            .insert(id);
    }
    let own = &reports[bindings[&tool.id]].0.scope;
    let request = install::Request {
        repository: input.files.repository.clone(),
        profiles: input.files.temporary_profiles.clone(),
        tool: tool.id.clone(),
        executable: own.executable.clone(),
        search_dirs: own.search_dirs.clone(),
        tool_search_dirs: check.dirs.clone(),
        bound_tools: ids,
    };
    let catalogs = groups
        .iter()
        .map(|(idx, ids)| {
            let mut scope = reports[*idx].0.scope.clone();
            scope.tools = ids.clone();
            scope
        })
        .collect::<Vec<_>>();
    let observations = groups
        .keys()
        .map(|idx| &reports[*idx].1)
        .collect::<Vec<_>>();
    if let Some(binding) = &own.npm_binding {
        install::npm::prepare_environment(
            host,
            request,
            install::npm::Scope {
                binding: binding.clone(),
                catalogs,
            },
            &observations,
        )
    } else {
        let prefix = install::brew::prefix_hint(&own.executable)
            .ok_or_else(|| invalid("Unknown Homebrew install destination."))?;
        let repository = own
            .executable
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| invalid("Unknown Homebrew install destination."))?
            .to_owned();
        let policy: VersionPolicy = tool
            .declaration
            .as_ref()
            .expect("declared tool")
            .policy
            .clone();
        install::brew::prepare_environment(
            host,
            request,
            install::brew::Scope {
                repository,
                cellar: prefix.join("Cellar"),
                prefix,
                policy,
                catalogs,
            },
            &observations,
        )
    }
}

pub struct Preview {
    plan: Plan,
    previous: Option<Run>,
    resume: bool,
    blockers: Vec<String>,
}

#[derive(Serialize)]
pub struct Review<'a> {
    repository: &'a Path,
    profiles: &'a [profiles::Row],
    rows: Vec<&'a Row>,
    inactive_links: &'a [planner::InactiveLink],
    last_run: Option<&'a Run>,
    resume: bool,
    blockers: &'a [String],
    checks: Vec<inspection::Description>,
    warning: &'static str,
}

const WARNING: &str = "Package managers may change caches and install dependencies. Dort stops on failure and does not roll back packages.";

impl Preview {
    pub fn prepare(
        host: &HostContext,
        root: &Path,
        temporary: &[String],
        resume: bool,
    ) -> Result<Self> {
        let plan = Plan::prepare(host, root, temporary)?;
        let previous = directory(host, &plan.input.repository_id, false)?
            .map(|dir| read::<Run>(&dir, "run.json"))
            .transpose()?
            .flatten();
        let mut blockers = vec![];
        if plan.rows.iter().any(|row| row.install.is_some())
            && install::diagnose(host)?
                .iter()
                .any(|attempt| attempt.unresolved)
        {
            blockers.push("An earlier install is unresolved. Review its record in Tools History before installing another package.".into());
        }
        match (&previous, resume) {
            (None, true) => {
                blockers.push("No run to resume. Review a new environment plan.".into())
            }
            (Some(run), true) => blockers.extend(run.review_resume(&plan)),
            (Some(run), false) if !run.complete() && !run.abandoned => {
                blockers.push("An unfinished run exists. Review Resume or Abandon environment run before starting another run.".into());
            }
            _ => (),
        }
        if plan
            .rows
            .iter()
            .any(|row| matches!(row.row.item, Item::Tool(_)))
            && previous
                .as_ref()
                .and_then(|run| run.stale_check.as_ref())
                .is_some_and(|stale| {
                    plan.check
                        .as_ref()
                        .is_none_or(|check| check.generation == *stale)
                })
        {
            blockers.push("Run a fresh Environment check after abandonment before reviewing tools in a new plan.".into());
        }
        Ok(Self {
            plan,
            previous,
            resume,
            blockers,
        })
    }

    pub fn scope(&self) -> (&Path, &BTreeSet<String>) {
        (
            &self.plan.input.files.repository,
            &self.plan.input.files.active_profiles,
        )
    }

    pub fn is_resume(&self) -> bool {
        self.resume
    }

    pub fn has_tools(&self) -> bool {
        self.plan
            .rows
            .iter()
            .any(|row| matches!(row.row.item, Item::Tool(_)))
    }

    pub fn blocked(&self) -> bool {
        !self.blockers.is_empty() || self.plan.rows.iter().any(|r| !r.row.blockers.is_empty())
    }

    pub fn review(&self) -> Review<'_> {
        Review {
            repository: &self.plan.input.files.repository,
            profiles: &self.plan.input.files.profiles,
            rows: self.plan.rows.iter().map(|r| &r.row).collect(),
            inactive_links: &self.plan.input.files.inactive_links,
            last_run: self.previous.as_ref(),
            resume: self.resume,
            blockers: &self.blockers,
            checks: self
                .plan
                .check
                .iter()
                .flat_map(|check| &check.catalogs)
                .filter_map(|catalog| {
                    catalog
                        .report(&self.plan.input.files.host.home)
                        .ok()
                        .map(|r| r.description)
                })
                .collect(),
            warning: WARNING,
        }
    }

    pub fn human(&self) -> String {
        use std::fmt::Write;
        let mut text = format!(
            "{}\nRepository: {:?}\n",
            if self.resume {
                "Review environment resume"
            } else {
                "Environment plan"
            },
            self.plan.input.files.repository
        );
        for profile in &self.plan.input.files.profiles {
            let _ = writeln!(text, "Profile {}", profile.label());
        }
        for (i, row) in self.plan.rows.iter().map(|p| &p.row).enumerate() {
            let _ = writeln!(text, "\n{}. {}: {}", i + 1, row.item.id(), row.status);
            if !row.requires.is_empty() {
                let _ = writeln!(text, "   Requires: {}", row.requires.join(", "));
            }
            match &row.detail {
                Detail::Tool {
                    provider,
                    package,
                    destination,
                    version,
                    command,
                } => {
                    let _ = writeln!(
                        text,
                        "   {provider:?} package: {package}\n   Destination: {}",
                        destination
                            .as_ref()
                            .map_or("Unknown".into(), |p| format!("{p:?}"))
                    );
                    if let Some(version) = version {
                        let _ = writeln!(text, "   Version: {version}");
                    }
                    if let Some(args) = command {
                        let _ = writeln!(text, "   Run: {args:?}");
                    }
                }
                Detail::File {
                    source,
                    destination,
                    effects,
                } => {
                    if let Some(source) = source {
                        let _ = writeln!(text, "   Source: {source:?}");
                    } else {
                        let _ = writeln!(text, "   Recipe: Inline in dort.toml");
                    }
                    let _ = writeln!(text, "   Target: {destination:?}");
                    for effect in effects {
                        let _ = writeln!(text, "   {effect:?}");
                    }
                }
            }
            for blocker in &row.blockers {
                let _ = writeln!(text, "   Blocked: {blocker}");
            }
        }
        for link in &self.plan.input.files.inactive_links {
            let _ = writeln!(
                text,
                "\n{}: No longer selected. Keep {:?} in place.",
                link.id, link.target
            );
        }
        for blocker in &self.blockers {
            let _ = writeln!(text, "\nBlocked: {blocker}");
        }
        if let Some(run) = &self.previous {
            let _ = writeln!(text, "\n{}", run.human());
        }
        if !self.review().checks.is_empty() {
            let _ = writeln!(text, "\nFresh checks before continuing:");
            for check in self.review().checks {
                let _ = writeln!(text, "   {:?} {:?}", check.executable, check.arguments);
            }
            let _ = writeln!(text, "\n{WARNING}");
        }
        text
    }

    pub fn approve(self) -> Result<Approval> {
        if self.blocked() {
            return Err(invalid("Fix the blocked rows and review again."));
        }
        Ok(Approval::new(self))
    }

    fn binding(&self) -> Result<String> {
        digest(&(
            &self.plan.input.stamp,
            &self.previous,
            &self.plan.check,
            self.plan
                .rows
                .iter()
                .map(PreparedRow::step)
                .collect::<Result<Vec<_>>>()?,
        ))
    }
}
