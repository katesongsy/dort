//! Shared application dispatch; presentation must obtain approval before calling.
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

pub fn requested_profiles(local: &LocalConfig, requested: &[String]) -> Vec<String> {
    local
        .active_profiles
        .iter()
        .chain(requested)
        .cloned()
        .collect()
}

pub fn observe(
    host: &HostContext,
    local: &LocalConfig,
    explicit: Option<&Path>,
    requested: &[String],
    request: Request,
) -> Result<Snapshot> {
    let root = local.select_repository(explicit)?;
    planner::observe(host, &root, &requested_profiles(local, requested), request)
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
pub fn check_manifest(
    host: &HostContext,
    local: &LocalConfig,
    path: PathBuf,
    requested: &[String],
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
    let active =
        manifest.validate_for_host(host, &requested_profiles(local, requested), Some(&root))?;
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
        unsupported.push("tool inventory and installation".into());
    }
    for file in &files {
        if file.mode != FileMode::Link {
            unsupported.push(format!("{}: {:?} deployment", file.id, file.mode));
        }
        if !file.requires.is_empty() {
            unsupported.push(format!("{}: tool prerequisite verification", file.id));
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
        mandatory: manifest
            .profiles
            .resolve(host.platform, &local.active_profiles)?,
    };
    Ok(CheckReport {
        manifest: path,
        schema: manifest.schema,
        selected_tools: tools.len(),
        selected_files: files.len(),
        tool_dependency_order: manifest.dependency_order(host.platform, &active)?,
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
    let profiles = requested_profiles(local, requested);
    Ok(dort_core::batch::plan(dort_core::batch::observe(
        host, &root, &profiles, request,
    )?))
}

pub fn adopt_request(target: &Path, id: &str, source: &Path) -> Request {
    Request::Adopt {
        mapping: ManagedFile {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            mode: FileMode::Link,
            profiles: vec!["base".into()],
            requires: vec![],
            overrides: Default::default(),
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
