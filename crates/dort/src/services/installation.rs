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
            matches!(d.provider, Provider::Mise | Provider::Npm)
                && matches!(d.policy, VersionPolicy::Exact { .. })
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
            return Err(invalid(
                "select an eligible missing exact mise or npm package",
            ));
        }
        let mut bindings = report.bindings.clone();
        if report
            .plan
            .rows
            .iter()
            .find(|r| r.id == tool)
            .and_then(|r| r.declaration.as_ref())
            .is_some_and(|d| d.provider == Provider::Npm)
        {
            let mut needed = std::collections::BTreeSet::new();
            let mut pending = vec![tool.to_owned()];
            while let Some(id) = pending.pop() {
                if needed.insert(id.clone()) {
                    let row = report
                        .plan
                        .rows
                        .iter()
                        .find(|r| r.id == id)
                        .ok_or_else(|| invalid("installation prerequisite row missing"))?;
                    pending.extend(row.dependencies.clone());
                }
            }
            bindings.retain(|id, _| needed.contains(id));
            if bindings.len() != needed.len() {
                return Err(invalid(
                    "every installation prerequisite needs an explicit catalog binding",
                ));
            }
        }
        Ok(Self {
            scope: report.scope.clone(),
            bindings,
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
        let request = install::Request {
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
        };
        let preview = if let Some(binding) = &description.npm_binding {
            self.prepare_npm(host, request, binding.clone())?
        } else {
            install::prepare(host, request, &snapshot.report.provider)?
        };
        if preview.review().scope.profiles != self.scope.active_profiles
            || preview.review().scope.platform != self.scope.platform
        {
            return Err(invalid(
                "repository profile scope changed; collect and reconcile the selected scope again",
            ));
        }
        Ok(preview)
    }
    fn prepare_npm(
        &self,
        host: &HostContext,
        mut request: install::Request,
        binding: dort_core::tools::inspection::NpmBinding,
    ) -> Result<install::Preview> {
        let (_, entry) = dort_core::observation::read_manifest(&self.scope.manifest)
            .map_err(|_| invalid("installation manifest unavailable; contents withheld"))?;
        let mut groups: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        for (tool, id) in &self.bindings {
            groups.entry(id.clone()).or_default().insert(tool.clone());
        }
        let mut catalogs = Vec::new();
        let mut reports = Vec::new();
        for (id, tools) in groups {
            let mut candidates = self.snapshots.iter().filter(|s| s.id == id);
            let snapshot = candidates
                .next()
                .ok_or_else(|| invalid("selected prerequisite snapshot unavailable"))?;
            if candidates.next().is_some() {
                return Err(invalid("duplicate prerequisite snapshot ID"));
            }
            let d = &snapshot.report.provider.description;
            if d.npm_binding.is_some()
                && (snapshot.report.manifest.as_ref() != Some(&self.scope.manifest)
                    || snapshot
                        .report
                        .source
                        .as_ref()
                        .is_none_or(|source| !source.matches(&entry)))
            {
                return Err(invalid(
                    "npm snapshot manifest changed; collect a fresh inventory and bind it explicitly",
                ));
            }
            catalogs.push(install::npm::Catalog {
                query: d.query,
                executable: d.executable.clone(),
                search_dirs: d.trusted_search_path.clone(),
                npm_binding: d.npm_binding.clone(),
                tools,
            });
            reports.push(&snapshot.report.provider);
        }
        request.bound_tools = self.bindings.keys().cloned().collect();
        let preview = install::npm::prepare(
            host,
            request,
            install::npm::Scope { binding, catalogs },
            &reports,
        )?;
        if !preview.manifest_matches(&entry) {
            return Err(invalid("installation manifest changed during preparation"));
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
