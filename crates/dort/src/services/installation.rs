//! Reconstruct install authority from explicit session bindings, never command DTOs.
use super::reconciliation::{Report, Scope, Snapshot};
use dort_core::{
    Error, Result,
    context::HostContext,
    local::LocalConfig,
    model::Provider,
    tools::{
        VersionPolicy, install,
        planning::{PackageAction, Row},
    },
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Request {
    scope: Scope,
    bindings: BTreeMap<String, String>,
    tool: String,
    snapshots: Vec<Snapshot>,
}
pub fn eligible(row: &Row) -> bool {
    row.action == PackageAction::ReviewMissingPackage
        && row.blockers.is_empty()
        && row.declaration.as_ref().is_some_and(|d| {
            d.provider == Provider::Mise && matches!(d.policy, VersionPolicy::Exact { .. })
        })
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}
/// Application-level scope projection only; core preparation/execution revalidates it.
pub fn profiles(
    host: &HostContext,
    manifest: &std::path::Path,
    additions: &[String],
) -> Result<Vec<String>> {
    let root = manifest
        .parent()
        .ok_or_else(|| invalid("installation needs a registered repository manifest"))?;
    if root.join("dort.toml") != manifest {
        return Err(invalid(
            "installation needs the registered dort.toml manifest",
        ));
    }
    let local = LocalConfig::load(host)
        .map_err(|_| invalid("repository selection unavailable; contents withheld"))?;
    if !local.repositories.values().any(|r| r.root == root) {
        return Err(invalid(
            "tool installation requires a registered physical repository root",
        ));
    }
    let mut selected = local.active_profiles;
    for profile in additions {
        if !selected.contains(profile) {
            selected.push(profile.clone());
        }
    }
    Ok(selected)
}
impl Request {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn selected(report: &Report, tool: &str, snapshots: &[Snapshot]) -> Result<Self> {
        if !report.plan.rows.iter().any(|r| r.id == tool && eligible(r)) {
            return Err(invalid("select an eligible missing exact mise package"));
        }
        Ok(Self {
            scope: report.scope.clone(),
            bindings: report.bindings.clone(),
            tool: tool.into(),
            snapshots: snapshots.to_vec(),
        })
    }
    pub fn prepare(&self, host: &HostContext) -> Result<install::Preview> {
        let repository = self
            .scope
            .manifest
            .parent()
            .ok_or_else(|| invalid("installation needs a registered repository manifest"))?
            .to_path_buf();
        if repository.join("dort.toml") != self.scope.manifest {
            return Err(invalid(
                "installation needs the registered dort.toml manifest",
            ));
        }
        let profiles = profiles(
            host,
            &self.scope.manifest,
            &self
                .scope
                .active_profiles
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )?;
        let id = self
            .bindings
            .get(&self.tool)
            .ok_or_else(|| invalid("explicit snapshot binding required"))?;
        let mut selected = self.snapshots.iter().filter(|s| &s.id == id);
        let snapshot = selected
            .next()
            .ok_or_else(|| invalid("selected snapshot unavailable"))?;
        if selected.next().is_some() {
            return Err(invalid("duplicate selected snapshot ID"));
        }
        let description = &snapshot.report.provider.description;
        let preview = install::prepare(
            host,
            install::Request {
                repository,
                profiles,
                tool: self.tool.clone(),
                executable: description.executable.clone(),
                search_dirs: description.trusted_search_path.clone(),
                tool_search_dirs: self.scope.tool_search_dirs.clone(),
                bound_tools: self
                    .bindings
                    .iter()
                    .filter(|(_, s)| *s == id)
                    .map(|(tool, _)| tool.clone())
                    .collect(),
            },
            &snapshot.report.provider,
        )?;
        if preview.review().scope.profiles != self.scope.active_profiles
            || preview.review().scope.platform != self.scope.platform
        {
            return Err(invalid(
                "repository profile scope changed; collect and reconcile the selected scope again",
            ));
        }
        Ok(preview)
    }
}

pub fn succeeded(report: &install::Report) -> bool {
    report.no_op
        || report.outcome.is_some_and(|o| {
            o.process == install::ProcessOutcome::Success
                && o.package == install::PackageState::ExactPresent
        })
}
