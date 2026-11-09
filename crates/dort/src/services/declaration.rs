//! Shared declaration inputs only; never package adoption or provider execution.
use dort_core::{Error, Result, context::HostContext, model::Tool, tools::declaration};
use std::path::Path;
#[derive(Clone)]
pub struct Request {
    pub tool: Tool,
}
impl Request {
    pub fn parse(json: &str) -> Result<Self> {
        if json.len() > 32 * 1024 {
            return Err(Error::Invalid("tool declaration exceeds 32 KiB".into()));
        }
        let tool = serde_json::from_str(json).map_err(|_| {
            Error::Invalid("invalid tool declaration JSON (values withheld)".into())
        })?;
        Ok(Self { tool })
    }
    pub fn prepare(
        self,
        host: &HostContext,
        repository: &Path,
        profiles: &[String],
    ) -> Result<declaration::Preview> {
        let local = dort_core::local::LocalConfig::load(host)?;
        let mut effective = local.active_profiles;
        for profile in profiles {
            if !effective.contains(profile) {
                effective.push(profile.clone());
            }
        }
        declaration::preview(host, repository, &effective, self.tool)
    }
}
