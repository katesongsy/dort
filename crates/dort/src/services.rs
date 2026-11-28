//! Application dispatch shared by CLI and TUI. Callers approve writes first.
pub mod declaration;
pub mod declared_tools;
#[cfg(test)]
pub(crate) mod declared_tools_tests;
pub mod generated_update;
pub mod inspection_defaults;
#[cfg(test)]
mod inspection_defaults_tests;
pub mod installation;
pub mod installed_tools;
pub mod presentation;
pub mod reconciliation;
pub mod tools;
use dort_core::{
    Manifest, Result, adoption, bootstrap,
    context::HostContext,
    discovery, lifecycle,
    local::LocalConfig,
    model::{FileMode, ManagedFile},
    planner::{self, Plan, Request, Snapshot, UnmanageMode},
    profiles::selected,
    state::WriterLock,
};

use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// Shared doctor metadata; never includes local variable values.
#[derive(Serialize)]
pub struct DoctorReport {
    pub host: HostContext,
    pub registered_repositories: usize,
    pub active_repository: Option<String>,
    pub mode: &'static str,
}
pub fn doctor(host: &HostContext, local: &LocalConfig) -> DoctorReport {
    DoctorReport {
        host: host.clone(),
        registered_repositories: local.repositories.len(),
        active_repository: local.active_repository.clone(),
        mode: "read-only foundation",
    }
}

pub fn observe(
    host: &HostContext,
    local: &LocalConfig,
    explicit: Option<&Path>,
    requested: &[String],
    request: Request,
) -> Result<Snapshot> {
    let root = local.select_repository(explicit)?;
    planner::observe(host, &root, requested, request)
}

pub fn preview_generated(
    host: &HostContext,
    local: &LocalConfig,
    repository: Option<&Path>,
    id: &str,
) -> Result<dort_core::generated::Preview> {
    let root = local.select_repository(repository)?;
    dort_core::generated::Preview::prepare(host, &root, id)
}

pub fn review_generated_deployment(
    host: &HostContext,
    repository: Option<&Path>,
    id: &str,
    profiles: &[String],
) -> Result<dort_core::generated::Deployment> {
    let root = absolute_repository(repository)?;
    dort_core::generated::Deployment::prepare(host, root.as_deref(), id, profiles)
}
pub fn review_generated_update(
    host: &HostContext,
    repository: Option<&Path>,
    id: &str,
    profiles: &[String],
    action: dort_core::generated::UpdateAction,
) -> Result<dort_core::generated::Update> {
    let root = absolute_repository(repository)?;
    dort_core::generated::Update::prepare_action(host, root.as_deref(), id, profiles, action)
}
fn absolute_repository(repository: Option<&Path>) -> Result<Option<PathBuf>> {
    repository
        .map(|path| {
            std::path::absolute(path).map_err(|source| dort_core::Error::Io {
                operation: "resolve repository path",
                path: path.to_owned(),
                source,
            })
        })
        .transpose()
}
pub struct ProfileChoices {
    pub known: BTreeSet<String>,
    pub defaults: Vec<String>,
    pub platform: Vec<String>,
    pub local: Vec<String>,
    pub mandatory: BTreeSet<String>,
}

#[derive(Serialize)]
pub struct CheckReport {
    pub manifest: PathBuf,
    pub schema: u32,
    pub active_profiles: BTreeSet<String>,
    pub selected_tools: usize,
    pub selected_files: usize,
    pub tool_dependency_order: Vec<String>,
    pub unsupported_capabilities: Vec<String>,
    pub filesystem_and_tools_inspected: bool,
    #[serde(skip)]
    pub choices: ProfileChoices,
}

/// Preserve CLI structural-check semantics, including its ordinary manifest read.
/// This is not file observation and deliberately does not inspect source contents.
pub const TOOL_CHECK_LIMIT: &str =
    "tool inventory and installation in structural checks (use Tools)";

pub fn check_manifest(
    host: &HostContext,
    local: &LocalConfig,
    path: PathBuf,
    requested: &[String],
) -> Result<CheckReport> {
    manifest_report(host, local, path, requested, true)
}

pub fn profile_picker(
    host: &HostContext,
    local: &LocalConfig,
    path: PathBuf,
    requested: &[String],
) -> Result<CheckReport> {
    manifest_report(host, local, path, requested, false)
}

fn manifest_report(
    host: &HostContext,
    local: &LocalConfig,
    path: PathBuf,
    requested: &[String],
    validate_selection: bool,
) -> Result<CheckReport> {
    let text = std::fs::read_to_string(&path).map_err(|source| dort_core::Error::Io {
        operation: "read manifest",
        path: path.clone(),
        source,
    })?;
    tracing::debug!("validating environment manifest");
    let manifest = Manifest::parse(&text)?;
    let root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|source| dort_core::Error::Io {
            operation: "resolve manifest directory",
            path: path.clone(),
            source,
        })?;
    let registered =
        (path.file_name().is_some_and(|name| name == "dort.toml")).then_some(root.as_path());
    let active = if validate_selection {
        manifest.validate_local_selection(host, requested, registered)?
    } else {
        dort_core::profiles::active(&local.profile_rows(
            &manifest.profiles,
            host.platform,
            registered,
            requested,
        )?)
    };
    let tools: Vec<_> = manifest
        .tools
        .iter()
        .filter(|t| selected(&t.profiles, &active))
        .collect();
    let files: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| selected(&f.profiles, &active))
        .collect();
    let mut unsupported = vec![];
    if !tools.is_empty() {
        unsupported.push(TOOL_CHECK_LIMIT.into());
    }
    for file in &files {
        if file.mode != FileMode::Link {
            unsupported.push(format!("{}: {:?} deployment", file.id, file.mode));
        }
        if !file.requires.is_empty() {
            unsupported.push(format!(
                "{}: review the environment to check file prerequisites",
                file.id
            ));
        }
    }
    let choices = ProfileChoices {
        known: manifest.profiles.known(),
        defaults: manifest.profiles.default.clone(),
        platform: manifest
            .profiles
            .platform
            .get(&host.platform)
            .cloned()
            .unwrap_or_default(),
        local: local.active_profiles.clone(),
        mandatory: dort_core::profiles::active(&local.profile_rows(
            &manifest.profiles,
            host.platform,
            registered,
            &[],
        )?),
    };
    Ok(CheckReport {
        manifest: path,
        schema: manifest.schema,
        selected_tools: tools.len(),
        selected_files: files.len(),
        tool_dependency_order: if validate_selection {
            manifest.dependency_order(host.platform, &active)?
        } else {
            vec![]
        },
        active_profiles: active,
        unsupported_capabilities: unsupported,
        filesystem_and_tools_inspected: false,
        choices,
    })
}

pub fn resolve_root(path: &Path, launch_directory: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        launch_directory.join(path)
    }
}

pub fn batch_plan(
    host: &HostContext,
    local: &LocalConfig,
    repository: Option<&Path>,
    requested: &[String],
    request: dort_core::batch::Request,
) -> Result<dort_core::batch::Plan> {
    let root = local.select_repository(repository)?;
    Ok(dort_core::batch::plan(dort_core::batch::observe(
        host, &root, requested, request,
    )?))
}

pub fn adopt_request(target: &Path, id: &str, source: &Path) -> Request {
    Request::Adopt {
        mapping: ManagedFile {
            id: id.into(),
            source: Some(source.into()),
            template: None,
            target: target.into(),
            mode: FileMode::Link,
            profiles: vec!["base".into()],
            requires: vec![],
            overrides: Default::default(),
            inputs: Default::default(),
            syntax: None,
            host_inputs: None,
            permissions: None,
        },
    }
}

pub struct Discovery {
    /// Explicit discovery root for HOME-relative presentation defaults, never environment lookup.
    pub home: PathBuf,
    pub repository: Option<PathBuf>,
    pub report: discovery::Discovery,
}

/// Missing selection is allowed; invalid selected scope must never become unscoped.
pub fn discover(
    host: &HostContext,
    local: &LocalConfig,
    explicit: Option<&Path>,
) -> Result<Discovery> {
    let repository = if explicit.is_some() || local.active_repository.is_some() {
        let root = local.select_repository(explicit)?;
        if dort_core::observation::inspect(&root, false)?.kind
            != dort_core::observation::Kind::Directory
        {
            return Err(dort_core::Error::Invalid(
                "selected discovery repository must be an existing no-follow directory".into(),
            ));
        }
        dort_core::observation::validate_output_path(&root)?;
        Some(root)
    } else {
        None
    };
    let report = discovery::discover(host, repository.as_deref())?;
    Ok(Discovery {
        home: host.home.clone(),
        repository,
        report,
    })
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
