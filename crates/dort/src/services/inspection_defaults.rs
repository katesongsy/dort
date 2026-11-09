//! Metadata-only suggestions. Inputs are captured by the application boundary.
use super::tools::{Preview, Request};
use dort_core::{context::HostContext, observation, tools::inspection::Query};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Clone, Default)]
pub struct Inputs {
    pub search_dirs: Vec<PathBuf>,
    pub overrides: Vec<Request>,
}
pub struct Choice {
    pub label: &'static str,
    pub request: Option<Request>,
    pub preview: Option<Preview>,
    pub reason: &'static str,
    pub selected: bool,
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
    pub fn detect(self, host: &HostContext, manifest: Option<PathBuf>) -> Defaults {
        // Ignore relative, empty and non-UTF-8 PATH entries. Preserve PATH precedence.
        let mut dirs = vec![];
        for path in self.search_dirs {
            if path.is_absolute() && path.to_str().is_some() && !dirs.contains(&path) {
                dirs.push(path);
            }
        }
        let find = |name: &str| dirs.iter().find_map(|dir| executable(dir.join(name)));
        let mut choices = vec![];
        for (query, name, label) in [
            (Query::BrewFormulae, "brew", "Homebrew formulae"),
            (Query::DpkgPackages, "dpkg-query", "apt / dpkg packages"),
            (Query::MiseInstalled, "mise", "mise runtimes"),
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
                    reason = "Select a repository declaring one Node runtime, or use Advanced";
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
                        reason = "Choose Node in Advanced";
                    } else if request.runtime_tool.is_some() {
                        reason = "Choose npm CLI and global prefix in Advanced";
                    }
                    request
                })
            } else {
                request
            };
            let request = if let Some(saved) = self.overrides.iter().find(|r| r.query == query) {
                reason = "Saved settings need adjustment in Advanced";
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
                reason = "Defaults need adjustment in Advanced";
            }
            let selected = preview.is_some();
            choices.push(Choice {
                label,
                request,
                preview,
                reason,
                selected,
            });
        }
        Defaults { choices }
    }
}
