//! Pure, non-executable command descriptions for missing-package review proposals.
//! No process runner, approval, deserialization or persistence API exists here.
use super::{
    VersionPolicy, inspection,
    planning::{PackageAction, PackageFinding, Row},
};
use crate::model::Provider;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

pub const MAX_COMMAND_BYTES: usize = 65_536;
pub const MAX_TOTAL_COMMAND_BYTES: usize = 4 * 1024 * 1024;
pub const WARNING: &str = "PREVIEW ONLY: Dort cannot execute this description. It is not approval, a tested provider capability or proof of safe global absence. Native installation may download code, execute provider/plugins/package code, change dependencies, links and caches, or fail with partial effects. No rollback or tool-readiness guarantee. Execution needs a separately agreed fresh-validation, approval, persistence and failure design.";
#[derive(Debug, Serialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Preview {
    NoPackageChange,
    #[default]
    Blocked,
    Deferred {
        reason: Deferred,
    },
    PreviewOnly {
        command: Command,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Deferred {
    MissingScope,
    InvalidScope,
    UnsupportedPolicy,
    DescriptionLimit,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionSelection {
    ProviderDefaultUnresolved,
    LatestUnresolved,
    ExactRequested,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    NetworkAndDownloads,
    ProviderConfigurationAndCode,
    DependencyChangesMayIncludeUpgrades,
    SharedLinksCachesAndData,
    NoAutomaticRollback,
    NpmLifecycleScriptsDisabled,
    SkippedScriptsMayLeavePackageUnusable,
}
#[derive(Debug, Serialize)]
pub struct Command {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: &'static str,
    pub environment_cleared: bool,
    /// Whitelisted controls and explicit scope paths, never inherited environment values.
    pub environment: BTreeMap<String, String>,
    pub version_selection: VersionSelection,
    pub effects: Vec<Effect>,
    pub warning: &'static str,
    pub execution_authorized: bool,
}
/// Internal construction from the pure planner's already-validated row only.
/// This function never creates a process capability or converts metadata to approval.
pub(super) fn describe(
    row: &Row,
    trusted_search_path: &[PathBuf],
    total_bytes: &mut usize,
) -> Preview {
    match row.action {
        PackageAction::NoChange => return Preview::NoPackageChange,
        PackageAction::Blocked => return Preview::Blocked,
        PackageAction::ReviewMissingPackage => (),
    }
    if !row.blockers.is_empty()
        || !matches!(row.package, PackageFinding::Absent)
        || !matches!(
            row.binary_observation,
            super::planning::BinaryObservation::Missing
        )
    {
        return Preview::Blocked;
    }
    let (Some(declaration), Some(scope)) = (&row.declaration, &row.catalog_scope) else {
        return Preview::Deferred {
            reason: Deferred::MissingScope,
        };
    };
    if scope.query.provider() != declaration.provider
        || !inspection::absolute(&scope.executable)
        || !inspection::absolute(&scope.home)
        || trusted_search_path.is_empty()
        || !trusted_search_path.iter().all(|p| inspection::absolute(p))
    {
        return Preview::Deferred {
            reason: Deferred::InvalidScope,
        };
    }
    let Ok(path) = std::env::join_paths(trusted_search_path) else {
        return Preview::Deferred {
            reason: Deferred::InvalidScope,
        };
    };
    let Some(path) = path.to_str() else {
        return Preview::Deferred {
            reason: Deferred::InvalidScope,
        };
    };
    let mut environment: BTreeMap<String, String> = [
        ("HOME", scope.home.clone()),
        ("XDG_CONFIG_HOME", scope.home.join(".config")),
        ("XDG_CACHE_HOME", scope.home.join(".cache")),
        ("XDG_DATA_HOME", scope.home.join(".local/share")),
        ("XDG_STATE_HOME", scope.home.join(".local/state")),
    ]
    .into_iter()
    .map(|(key, value)| {
        (
            key.into(),
            value.to_str().expect("validated UTF-8 home scope").into(),
        )
    })
    .collect();
    for (key, value) in [
        ("PATH", path),
        ("LC_ALL", "C"),
        ("LANG", "C"),
        ("NO_COLOR", "1"),
    ] {
        environment.insert(key.into(), value.into());
    }
    let selection = match declaration.policy {
        VersionPolicy::ProviderDefault {} => VersionSelection::ProviderDefaultUnresolved,
        VersionPolicy::Latest {} => VersionSelection::LatestUnresolved,
        VersionPolicy::Exact { .. } => VersionSelection::ExactRequested,
    };
    let mut effects = vec![
        Effect::NetworkAndDownloads,
        Effect::ProviderConfigurationAndCode,
        Effect::DependencyChangesMayIncludeUpgrades,
        Effect::SharedLinksCachesAndData,
        Effect::NoAutomaticRollback,
    ];
    let arguments = match declaration.provider {
        Provider::Brew => {
            if !matches!(
                declaration.policy,
                VersionPolicy::ProviderDefault {} | VersionPolicy::Latest {}
            ) {
                return Preview::Deferred {
                    reason: Deferred::UnsupportedPolicy,
                };
            }
            // Suppress native root upgrades, dependent repair and cleanup where the
            // provider supports these controls. They do NOT prohibit dependency changes.
            for key in [
                "HOMEBREW_NO_AUTO_UPDATE",
                "HOMEBREW_NO_ANALYTICS",
                "HOMEBREW_NO_ENV_HINTS",
                "HOMEBREW_NO_INSTALL_FROM_API",
                "HOMEBREW_NO_INSTALL_UPGRADE",
                "HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
                "HOMEBREW_NO_INSTALL_CLEANUP",
            ] {
                environment.insert(key.into(), "1".into());
            }
            vec![
                "install".into(),
                "--formula".into(),
                declaration.package.clone(),
            ]
        }
        Provider::Npm => {
            let Some(binding) = scope.npm_binding.as_ref().filter(|b| {
                b.node == scope.executable
                    && Some(b.runtime_tool.as_str()) == declaration.runtime_tool.as_deref()
                    && inspection::absolute(&b.npm_cli)
                    && inspection::absolute(&b.prefix)
            }) else {
                return Preview::Deferred {
                    reason: Deferred::InvalidScope,
                };
            };
            let package = match &declaration.policy {
                VersionPolicy::ProviderDefault {} => declaration.package.clone(),
                VersionPolicy::Latest {} => format!("{}@latest", declaration.package),
                VersionPolicy::Exact { version } => format!("{}@{version}", declaration.package),
            };
            effects.push(Effect::NpmLifecycleScriptsDisabled);
            effects.push(Effect::SkippedScriptsMayLeavePackageUnusable);
            vec![
                binding
                    .npm_cli
                    .to_str()
                    .expect("validated UTF-8 npm CLI")
                    .into(),
                "install".into(),
                "--global".into(),
                "--prefix".into(),
                binding
                    .prefix
                    .to_str()
                    .expect("validated UTF-8 npm prefix")
                    .into(),
                "--ignore-scripts".into(),
                "--no-audit".into(),
                "--no-fund".into(),
                "--logs-max=0".into(),
                "--update-notifier=false".into(),
                package,
            ]
        }
    };
    let preview = Preview::PreviewOnly {
        command: Command {
            executable: scope.executable.clone(),
            arguments,
            working_directory: "/",
            environment_cleared: true,
            environment,
            version_selection: selection,
            effects,
            warning: WARNING,
            execution_authorized: false,
        },
    };
    let Ok(encoded) = serde_json::to_vec(&preview) else {
        return Preview::Deferred {
            reason: Deferred::InvalidScope,
        };
    };
    let bytes = encoded.len();
    if bytes > MAX_COMMAND_BYTES || total_bytes.saturating_add(bytes) > MAX_TOTAL_COMMAND_BYTES {
        return Preview::Deferred {
            reason: Deferred::DescriptionLimit,
        };
    }
    *total_bytes += bytes;
    preview
}
