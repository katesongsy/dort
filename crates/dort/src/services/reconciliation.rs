//! Session check results and package comparisons.
use super::tools;
use dort_core::{
    Error, Manifest, Result,
    context::HostContext,
    model::Provider,
    observation,
    platform::Platform,
    profiles::selected,
    tools::{binaries, inspection, planning},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub const MAX_SNAPSHOTS: usize = 16;
pub const MAX_SNAPSHOT_RECORDS: usize = 65_536;
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub id: String,
    #[serde(serialize_with = "serialize_report")]
    pub report: std::sync::Arc<tools::Report>,
}
fn serialize_report<S: serde::Serializer>(
    report: &std::sync::Arc<tools::Report>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    report.as_ref().serialize(serializer)
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}
#[derive(Clone)]
pub struct Request {
    pub manifest: PathBuf,
    pub tool_search_dirs: Vec<PathBuf>,
    pub profiles: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct Scope {
    pub manifest: PathBuf,
    pub tool_search_dirs: Vec<PathBuf>,
    pub platform: Platform,
    pub active_profiles: BTreeSet<String>,
    #[serde(skip)]
    pub temporary_profiles: Vec<String>,
}
pub struct ToolChoice {
    pub id: String,
    pub provider: Option<Provider>,
}
pub struct Prepared {
    pub scope: Scope,
    pub tools: Vec<ToolChoice>,
    manifest: Manifest,
    source: tools::Source,
}
#[derive(Serialize)]
pub struct Report {
    pub scope: Scope,
    pub bindings: BTreeMap<String, String>,
    pub binaries: binaries::Report,
    pub plan: planning::Plan,
}
impl Request {
    /// Standalone read: no registration, provider process, PATH discovery or writes.
    pub fn prepare(&self, host: &HostContext) -> Result<Prepared> {
        if !self.manifest.is_absolute() || self.manifest.to_str().is_none() {
            return Err(invalid(
                "reconciliation requires an absolute UTF-8 manifest path",
            ));
        }
        let (manifest, entry) = observation::read_manifest(&self.manifest).map_err(|_| {
            invalid("reconciliation manifest unavailable, unsafe or invalid; contents withheld")
        })?;
        let local = dort_core::local::LocalConfig::load(host)?;
        let root = self.manifest.parent().filter(|root| {
            root.join("dort.toml") == self.manifest
                && local.repositories.values().any(|r| r.root == *root)
        });
        let active = if root.is_some() {
            dort_core::profiles::active(&local.profile_rows(
                &manifest.profiles,
                host.platform,
                root,
                &self.profiles,
            )?)
        } else {
            manifest.profiles.resolve(host.platform, &self.profiles)?
        };
        binaries::validate(&manifest, &active, &self.tool_search_dirs)?;
        let mut tools: Vec<_> = manifest
            .tools
            .iter()
            .filter(|t| selected(&t.profiles, &active))
            .map(|t| ToolChoice {
                id: t.id.clone(),
                provider: Some(t.install.provider),
            })
            .collect();
        tools.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Prepared {
            scope: Scope {
                manifest: self.manifest.clone(),
                tool_search_dirs: self.tool_search_dirs.clone(),
                platform: host.platform,
                active_profiles: active,
                temporary_profiles: self.profiles.clone(),
            },
            tools,
            source: tools::Source::from_entry(&entry),
            manifest,
        })
    }
}
impl Prepared {
    pub fn validate_bindings(
        &self,
        bindings: &BTreeMap<String, String>,
        ids: &BTreeSet<String>,
    ) -> Result<()> {
        if ids.len() > MAX_SNAPSHOTS || ids.iter().any(|id| !identifier(id)) {
            return Err(invalid("Too many or invalid check results."));
        }
        for (tool, scope) in bindings {
            if !self.tools.iter().any(|t| &t.id == tool) || !ids.contains(scope) {
                return Err(invalid("Choose a checked tool first."));
            }
        }
        Ok(())
    }
    /// Uses chosen snapshots as recorded, never refreshes or replays approval.
    pub fn reconcile(
        self,
        snapshots: &[Snapshot],
        bindings: BTreeMap<String, String>,
    ) -> Result<Report> {
        let ids: BTreeSet<_> = snapshots.iter().map(|s| s.id.clone()).collect();
        if ids.len() != snapshots.len() {
            return Err(invalid("Duplicate check result."));
        }
        self.validate_bindings(&bindings, &ids)?;
        if snapshot_records(snapshots) > MAX_SNAPSHOT_RECORDS {
            return Err(invalid(
                "Too many check results. Clear old results and check again.",
            ));
        }
        let (_, entry) = observation::read_manifest(&self.scope.manifest).map_err(|_| {
            invalid("reconciliation manifest unavailable or changed; contents withheld")
        })?;
        if !self.source.matches(&entry) {
            return Err(invalid(
                "reconciliation manifest changed; reload declarations before retrying",
            ));
        }
        let mut catalogs = BTreeMap::new();
        for (tool, id) in &bindings {
            let snapshot = snapshots
                .iter()
                .find(|s| &s.id == id)
                .ok_or_else(|| invalid("The selected check result is unavailable."))?;
            if snapshot.report.provider.description.query == inspection::Query::NpmGlobals
                && (snapshot.report.manifest.as_ref() != Some(&self.scope.manifest)
                    || snapshot
                        .report
                        .source
                        .as_ref()
                        .is_none_or(|source| !source.matches(&entry)))
            {
                return Err(invalid("The manifest changed. Run a fresh check."));
            }
            catalogs.insert(tool.clone(), &snapshot.report.provider);
        }
        let binaries = binaries::observe(
            &self.manifest,
            &self.scope.active_profiles,
            &self.scope.tool_search_dirs,
        )?;
        let (_, after) = observation::read_manifest(&self.scope.manifest).map_err(|_| {
            invalid("reconciliation manifest unavailable or changed; contents withheld")
        })?;
        if !self.source.matches(&after) {
            return Err(invalid(
                "The manifest changed. Reload declarations and try again.",
            ));
        }
        let observations = planning::Observations {
            catalogs,
            binaries: binaries
                .tools
                .iter()
                .map(|(id, t)| (id.clone(), t.observation.clone()))
                .collect(),
        };
        let plan = planning::plan(
            &self.manifest,
            self.scope.platform,
            &self.scope.active_profiles,
            &observations,
        )?;
        Ok(Report {
            scope: self.scope,
            bindings,
            binaries,
            plan,
        })
    }
}
pub fn snapshot_records(snapshots: &[Snapshot]) -> usize {
    snapshots
        .iter()
        .map(|s| match &s.report.provider.observation {
            inspection::Observation::Complete { installations } => installations.len(),
            _ => 0,
        })
        .sum()
}
