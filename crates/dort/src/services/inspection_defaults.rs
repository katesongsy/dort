//! Read-only suggested paths and defaults. The application captures inputs.
use super::tools::Request;
use dort_core::{context::HostContext, observation, tools::inspection::Query};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Clone, Default)]
pub struct Inputs {
    pub search_dirs: Vec<PathBuf>,
    pub overrides: Vec<Request>,
}
pub struct Choice {
    pub request: Option<Request>,
    pub reason: &'static str,
}
pub struct Defaults {
    pub choices: Vec<Choice>,
}
fn executable(path: PathBuf) -> Option<PathBuf> {
    let metadata = path.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .then(|| path.canonicalize().ok())?
        .filter(|p| p.to_str().is_some())
}
impl Inputs {
    fn search_path(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for path in &self.search_dirs {
            if path.is_absolute() && path.to_str().is_some() && !dirs.contains(path) {
                dirs.push(path.clone());
            }
        }
        dirs
    }
    /// Standalone inventory uses npm itself, not a repository's managed-runtime scope.
    pub fn npm_inventory(&self, host: &HostContext) -> dort_core::Result<super::tools::Preview> {
        let dirs = self.search_path();
        let npm = dirs
            .iter()
            .find_map(|dir| executable(dir.join("npm")))
            .ok_or_else(|| {
                dort_core::Error::Invalid(
                    "npm was not found in PATH; make its program available and inspect again."
                        .into(),
                )
            })?;
        super::tools::Preview::npm_inventory(host, npm, dirs)
    }
    /// Optional user-facing tool settings for nonstandard layouts; no internal IDs.
    pub fn with_settings(
        mut self,
        host: &HostContext,
        manifest: &std::path::Path,
        programs: &[String],
        npm_clis: &[String],
        npm_prefixes: &[String],
    ) -> dort_core::Result<Self> {
        use dort_core::Error;
        let invalid = |s: &str| Error::Invalid(s.into());
        let mut settings = std::collections::BTreeMap::<String, [Option<PathBuf>; 3]>::new();
        for (field, values) in [programs, npm_clis, npm_prefixes].into_iter().enumerate() {
            for value in values {
                let (id, path) = value
                    .split_once('=')
                    .filter(|(id, path)| !id.is_empty() && !path.is_empty())
                    .ok_or_else(|| invalid("Tool settings use TOOL=/absolute/path."))?;
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err(invalid("Tool settings need absolute paths."));
                }
                let entry = settings.entry(id.into()).or_default();
                if entry[field].replace(path).is_some() {
                    return Err(invalid("A tool setting was supplied more than once."));
                }
            }
        }
        for (id, fields) in settings {
            let mut request = for_tool(host, manifest, &id, self.clone())?;
            if let Some(program) = &fields[0] {
                request.executable = program.clone();
            }
            if request.query != Query::NpmGlobals && (fields[1].is_some() || fields[2].is_some()) {
                return Err(invalid(
                    "npm CLI and prefix settings require a declared npm tool.",
                ));
            }
            if let Some(cli) = &fields[1] {
                request.npm_cli = Some(cli.clone());
            }
            if let Some(prefix) = &fields[2] {
                request.prefix = Some(prefix.clone());
            }
            request.clone().prepare(host)?;
            if self.overrides.iter().any(|r| {
                r.query == request.query
                    && r.runtime_tool == request.runtime_tool
                    && (r.executable != request.executable
                        || r.search_dirs != request.search_dirs
                        || r.npm_cli != request.npm_cli
                        || r.prefix != request.prefix)
            }) {
                return Err(invalid(
                    "Tools sharing one package-manager scope need matching settings.",
                ));
            }
            self.overrides
                .retain(|r| r.query != request.query || r.runtime_tool != request.runtime_tool);
            self.overrides.push(request);
        }
        Ok(self)
    }
    pub fn detect(self, host: &HostContext, manifest: Option<PathBuf>) -> Defaults {
        // Ignore relative, empty and non-UTF-8 PATH entries. Preserve PATH precedence.
        let dirs = self.search_path();
        let find = |name: &str| dirs.iter().find_map(|dir| executable(dir.join(name)));
        let mut choices = vec![];
        for (query, name, _label) in [
            (Query::BrewFormulae, "brew", "Homebrew formulae"),
            (Query::NpmGlobals, "node", "npm global packages"),
        ] {
            let mut reason = "Not found in PATH";
            let request = Some(Request {
                query,
                executable: find(name).unwrap_or_default(),
                search_dirs: dirs.clone(),
                manifest: None,
                runtime_tool: None,
                npm_cli: None,
                prefix: None,
            });
            let request = if query == Query::NpmGlobals {
                request.map(|mut request| {
                    // Only a unique declared Node runtime and a complete, matching
                    // installation layout qualify. Never run npm config or its launcher.
                    reason = "Select a repository declaring one Node runtime, or configure its Node/npm settings";
                    request.manifest = manifest.clone();
                    if let Some(path) = &manifest
                        && let Ok((parsed, _)) = observation::read_manifest(path)
                    {
                        let nodes: Vec<_> =
                            parsed.tools.iter().filter(|t| t.binary == "node").collect();
                        if nodes.len() == 1 {
                            request.runtime_tool = Some(nodes[0].id.clone());
                        }
                    }
                    if let Some(prefix) = request.executable.parent().and_then(|p| p.parent()) {
                        let prefix = prefix.to_path_buf();
                        let cli = prefix.join("lib/node_modules/npm/bin/npm-cli.js");
                        // Resolve only the conventional co-located layout, not a
                        // launcher from another runtime. Prefix is displayed for approval.
                        if cli.metadata().is_ok_and(|m| m.is_file())
                            && let Ok(cli) = cli.canonicalize()
                            && cli.starts_with(&prefix)
                            && cli.to_str().is_some()
                        {
                            request.npm_cli = Some(cli);
                            request.prefix = Some(prefix);
                        }
                    }
                    if request.executable.as_os_str().is_empty() {
                        reason = "Choose Node in Tool settings";
                    } else if request.runtime_tool.is_some() {
                        reason = "Choose npm CLI and global prefix in Tool settings";
                    }
                    request
                })
            } else {
                request
            };
            let request = if let Some(saved) = self.overrides.iter().find(|r| r.query == query) {
                reason = "Saved settings need adjustment in Tool settings";
                Some(saved.clone())
            } else {
                request
            };
            let preview = request.clone().and_then(|r| r.prepare(host).ok());
            if request
                .as_ref()
                .is_some_and(|r| !r.executable.as_os_str().is_empty())
                && preview.is_none()
                && query != Query::NpmGlobals
            {
                reason = "Defaults need adjustment in Tool settings";
            }
            choices.push(Choice { request, reason });
        }
        Defaults { choices }
    }
}
/// Suggestions for one declared tool. Provider and runtime associations are derived.
pub fn for_tool(
    host: &HostContext,
    manifest: &std::path::Path,
    id: &str,
    mut inputs: Inputs,
) -> dort_core::Result<Request> {
    use dort_core::{Error, model::Provider};
    let invalid = |s: &str| Error::Invalid(s.into());
    let (current, _) = dort_core::observation::read_manifest(manifest)
        .map_err(|_| invalid("Cannot read tool declarations; manifest contents withheld."))?;
    let declared = current
        .tools
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| invalid("Tool declaration changed; open its settings again."))?;
    let spec = &declared.install;
    let query = match spec.provider {
        Provider::Brew => Query::BrewFormulae,
        Provider::Npm => Query::NpmGlobals,
    };
    inputs
        .overrides
        .retain(|r| r.query != Query::NpmGlobals || r.runtime_tool == spec.runtime_tool);
    let mut defaults = inputs.detect(host, Some(manifest.to_owned()));
    let mut request = defaults
        .choices
        .iter_mut()
        .find(|c| c.request.as_ref().is_some_and(|r| r.query == query))
        .and_then(|c| c.request.take())
        .ok_or_else(|| invalid("Package manager settings are unavailable."))?;
    if query == Query::NpmGlobals {
        request.manifest = Some(manifest.to_owned());
        request.runtime_tool = spec.runtime_tool.clone();
    }
    Ok(request)
}
