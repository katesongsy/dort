//! Explicit, homogeneous selected-file batches. Planning never acquires a lock or writes.
use crate::{
    Error, Manifest, Result,
    adoption::canonical,
    context::HostContext,
    journal,
    model::ManagedFile,
    planner::{self, Request as Single, UnmanageMode},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_FILES: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Adopt,
    Apply,
    Keep,
    Remove,
}
#[derive(Clone)]
pub enum Request {
    Adopt(Vec<ManagedFile>),
    Apply(Vec<String>),
    Keep(Vec<String>),
    Remove(Vec<String>),
}
impl Request {
    pub fn action(&self) -> Action {
        match self {
            Self::Adopt(_) => Action::Adopt,
            Self::Apply(_) => Action::Apply,
            Self::Keep(_) => Action::Keep,
            Self::Remove(_) => Action::Remove,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub action: Action,
    pub items: Vec<planner::Snapshot>,
    pub blockers: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub schema: u32,
    pub inputs: Snapshot,
    pub items: Vec<planner::Plan>,
    pub blockers: Vec<String>,
}

pub fn observe(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    request: Request,
) -> Result<Snapshot> {
    let action = request.action();
    let singles: Vec<_> = match request {
        Request::Adopt(mappings) => mappings
            .into_iter()
            .map(|mapping| Single::Adopt { mapping })
            .collect(),
        Request::Apply(ids) | Request::Keep(ids) | Request::Remove(ids) => ids
            .into_iter()
            .map(|id| match action {
                Action::Apply => Single::Unmanage {
                    id,
                    mode: UnmanageMode::Keep,
                },
                Action::Keep => Single::Unmanage {
                    id,
                    mode: UnmanageMode::Keep,
                },
                Action::Remove => Single::Unmanage {
                    id,
                    mode: UnmanageMode::Remove,
                },
                Action::Adopt => unreachable!(),
            })
            .collect(),
    };
    if singles.is_empty() || singles.len() > MAX_FILES {
        return Err(Error::Invalid(
            "batch requires 1–32 explicitly selected files".into(),
        ));
    }
    let mut items = vec![];
    for single in singles {
        let mut observed = planner::observe(host, repository, profiles, single)?;
        // Observe exactly the requested ID, not full-scope Apply. The pure per-file
        // planner then computes Apply semantics on that explicitly observed snapshot.
        if action == Action::Apply {
            observed.request = Single::Apply;
        }
        items.push(observed);
    }
    items.sort_by(|a, b| a.files[0].mapping.id.cmp(&b.files[0].mapping.id));
    let mut blockers = vec![];
    if action == Action::Adopt {
        let (mut manifest, entry) =
            crate::observation::read_manifest(&repository.join("dort.toml"))?;
        if items
            .iter()
            .any(|s| canonical(&s.manifest).ok() != canonical(&entry).ok())
        {
            return Err(Error::Invalid(
                "manifest changed during batch observation".into(),
            ));
        }
        let mappings: Vec<_> = items.iter().map(|s| s.files[0].mapping.clone()).collect();
        validate_adoptions(&mut manifest, &mappings, host, profiles, repository)?;
    }
    if items.windows(2).any(|w| {
        canonical(&w[0].manifest).ok() != canonical(&w[1].manifest).ok()
            || canonical(&w[0].local_config).ok() != canonical(&w[1].local_config).ok()
    }) {
        blockers.push("configuration changed during batch observation".into());
    }
    Ok(Snapshot {
        action,
        items,
        blockers,
    })
}

pub(crate) fn validate_adoptions(
    manifest: &mut Manifest,
    mappings: &[ManagedFile],
    host: &HostContext,
    profiles: &[String],
    repo: &Path,
) -> Result<()> {
    for mapping in mappings {
        if !mapping.overrides.is_empty() {
            return Err(Error::Invalid("batch adoption cannot add overrides".into()));
        }
        for existing in &manifest.files {
            for source in std::iter::once(&existing.source).chain(existing.overrides.values()) {
                if overlap(source, &mapping.source) {
                    return Err(Error::Invalid("batch adoption sources overlap".into()));
                }
            }
        }
        manifest.files.push(mapping.clone());
    }
    let active = manifest.validate_for_host(host, profiles, Some(repo))?;
    if mappings
        .iter()
        .any(|m| !crate::profiles::selected(&m.profiles, &active))
    {
        return Err(Error::Invalid(
            "batch adoption mappings must all be active".into(),
        ));
    }
    Ok(())
}
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
/// Pure deterministic combined planning, including cross-item identity/path conflicts.
pub fn plan(inputs: Snapshot) -> Plan {
    let mut blockers = inputs.blockers.clone();
    if inputs.items.is_empty() || inputs.items.len() > MAX_FILES {
        blockers.push("batch requires 1–32 files".into());
    }
    for (i, item) in inputs.items.iter().enumerate() {
        let valid_request = matches!(
            (inputs.action, &item.request),
            (Action::Adopt, Single::Adopt { .. })
                | (Action::Apply, Single::Apply)
                | (
                    Action::Keep,
                    Single::Unmanage {
                        mode: UnmanageMode::Keep,
                        ..
                    }
                )
                | (
                    Action::Remove,
                    Single::Unmanage {
                        mode: UnmanageMode::Remove,
                        ..
                    }
                )
        );
        if item.files.len() != 1 || !valid_request {
            blockers.push("batch item scope mismatch".into());
            continue;
        }
        for other in inputs.items.iter().take(i) {
            if item.repository != other.repository
                || journal::encode(&item.host).ok() != journal::encode(&other.host).ok()
                || item.active_profiles != other.active_profiles
                || canonical(&item.manifest).ok() != canonical(&other.manifest).ok()
                || canonical(&item.local_config).ok() != canonical(&other.local_config).ok()
            {
                blockers
                    .push("batch inputs must share one host, repository and configuration".into());
            }
            for a in &item.files {
                for b in &other.files {
                    if a.mapping.id == b.mapping.id
                        || overlap(&a.target_path, &b.target_path)
                        || overlap(&a.source_path, &b.source_path)
                        || overlap(&a.source_path, &b.target_path)
                        || overlap(&a.target_path, &b.source_path)
                    {
                        blockers.push("batch IDs, sources or destinations conflict".into());
                    }
                }
            }
        }
    }
    let items = inputs.items.iter().cloned().map(planner::plan).collect();
    Plan {
        schema: 4,
        inputs,
        items,
        blockers,
    }
}
impl Plan {
    pub fn has_blockers(&self) -> bool {
        !self.blockers.is_empty() || self.items.iter().any(planner::Plan::has_blockers)
    }
    pub fn human(&self) -> String {
        let mut text = format!(
            "Combined {:?} preview · {} selected files\nOne approval / one transaction; partial effects are possible on failure.\nFiltering and folding do not change this exact set.\n",
            self.inputs.action,
            self.items.len()
        );
        for b in &self.blockers {
            text.push_str(&format!("BLOCKED: {b}\n"));
        }
        for item in &self.items {
            text.push_str(&item.human());
        }
        text
    }
    pub(crate) fn request(&self) -> Request {
        let mappings: Vec<_> = self
            .inputs
            .items
            .iter()
            .flat_map(|s| s.files.iter().map(|f| f.mapping.clone()))
            .collect();
        let ids = mappings.iter().map(|m| m.id.clone()).collect();
        match self.inputs.action {
            Action::Adopt => Request::Adopt(mappings),
            Action::Apply => Request::Apply(ids),
            Action::Keep => Request::Keep(ids),
            Action::Remove => Request::Remove(ids),
        }
    }
}
/// One separately obtained exact-ID acknowledgement for every link to delete.
pub struct RemovalAcknowledgements(Vec<String>);
impl RemovalAcknowledgements {
    pub fn confirm(ids: Vec<String>) -> Self {
        Self(ids)
    }
}
pub struct Approval {
    pub(crate) plan: Plan,
    pub(crate) digest: String,
}
impl Approval {
    pub fn from_plan(plan: Plan) -> Result<Self> {
        Self::approve(plan, None)
    }
    pub fn from_removal_plan(
        plan: Plan,
        acknowledgements: RemovalAcknowledgements,
    ) -> Result<Self> {
        Self::approve(plan, Some(acknowledgements))
    }
    fn approve(plan: Plan, ack: Option<RemovalAcknowledgements>) -> Result<Self> {
        let mut expected: Vec<_> = plan
            .inputs
            .items
            .iter()
            .flat_map(|s| s.files.iter().map(|f| f.mapping.id.clone()))
            .collect();
        expected.sort();
        let valid_ack = match (plan.inputs.action, ack) {
            (Action::Remove, Some(mut ack)) => {
                ack.0.sort();
                ack.0 == expected
            }
            (Action::Remove, None) | (_, Some(_)) => false,
            (_, None) => true,
        };
        if !valid_ack
            || plan.has_blockers()
            || canonical(&plan)? != canonical(&self::plan(plan.inputs.clone()))?
        {
            return Err(Error::Invalid("batch approval requires an unmodified unblocked combined plan and every removal ID separately acknowledged".into()));
        }
        let digest = journal::digest(canonical(&plan)?.as_bytes());
        Ok(Self { plan, digest })
    }
    pub fn sha256(&self) -> &str {
        &self.digest
    }
}
pub use crate::batch_execution::{execute, rollback};
