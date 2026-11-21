//! Shared declaration inputs only; never package adoption or provider execution.
use dort_core::{Error, Result, context::HostContext, model::Tool, tools::declaration};
use std::{fmt::Write, path::Path};
pub fn report(preview: &declaration::Preview) -> String {
    let verb = match preview.change {
        None => "Add",
        Some(declaration::Change::Edit) => "Edit",
        Some(declaration::Change::Remove) => "Remove",
    };
    let mut out = format!(
        "{verb} tool declaration: {}\nManifest: {}\nInstalled packages stay untouched.\n",
        preview.tool.id,
        preview.repository.join("dort.toml").display()
    );
    if preview.no_op {
        out.push_str("No changes are needed.\n");
    }
    if let Some(before) = &preview.before {
        tool_report(&mut out, "Current declaration", before);
    }
    if preview.change != Some(declaration::Change::Remove) {
        tool_report(&mut out, "Desired declaration", &preview.tool);
    }
    out.push_str(
        "\nRecovery can restore the unchanged manifest result. Check installed after saving.\n",
    );
    out
}
fn tool_report(out: &mut String, title: &str, tool: &Tool) {
    writeln!(
        out,
        "\n{title}\nTool: {}\nCommand: {}\nProfiles: {}\nRequires: {}",
        tool.id,
        tool.binary,
        tool.profiles.join(", "),
        if tool.requires.is_empty() {
            "None".into()
        } else {
            tool.requires.join(", ")
        }
    )
    .unwrap();
    {
        let spec = &tool.install;
        writeln!(
            out,
            "{:?} / {} / {}",
            spec.provider,
            spec.package,
            spec.version.as_deref().unwrap_or("Provider default")
        )
        .unwrap();
        if let Some(runtime) = &spec.runtime_tool {
            writeln!(out, "Node runtime: {runtime}").unwrap();
        }
    }
}
#[derive(Clone)]
pub struct Request {
    pub tool: Tool,
    pub change: Option<declaration::Change>,
}
impl Request {
    pub fn parse(json: &str) -> Result<Self> {
        if json.len() > 32 * 1024 {
            return Err(Error::Invalid("tool declaration exceeds 32 KiB".into()));
        }
        let tool = serde_json::from_str(json).map_err(|_| {
            Error::Invalid("invalid tool declaration JSON (values withheld)".into())
        })?;
        Ok(Self { tool, change: None })
    }
    pub fn prepare(
        self,
        host: &HostContext,
        repository: &Path,
        profiles: &[String],
    ) -> Result<declaration::Preview> {
        let effective = effective_profiles(host, profiles)?;
        match self.change {
            None => declaration::preview(host, repository, &effective, self.tool),
            Some(declaration::Change::Edit) => {
                declaration::preview_edit(host, repository, &effective, self.tool)
            }
            Some(declaration::Change::Remove) => {
                declaration::preview_remove(host, repository, &effective, &self.tool.id)
            }
        }
    }
}
pub fn prepare_remove(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    id: &str,
) -> Result<declaration::Preview> {
    declaration::preview_remove(host, repository, &effective_profiles(host, profiles)?, id)
}
fn effective_profiles(host: &HostContext, profiles: &[String]) -> Result<Vec<String>> {
    let mut effective = dort_core::local::LocalConfig::load(host)?.active_profiles;
    for profile in profiles {
        if !effective.contains(profile) {
            effective.push(profile.clone());
        }
    }
    Ok(effective)
}
