//! Shared explicit inspection workflow. No provider discovery or automatic execution.
use dort_core::{Error, Result, context::HostContext, observation, tools::inspection};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Request {
    pub query: inspection::Query,
    pub executable: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub manifest: Option<PathBuf>,
    pub runtime_tool: Option<String>,
    pub npm_cli: Option<PathBuf>,
    pub prefix: Option<PathBuf>,
}
pub struct Preview {
    core: inspection::Preview,
    manifest: Option<PathBuf>,
}
pub struct Approval {
    core: inspection::Approval,
    manifest: Option<PathBuf>,
}
#[derive(Serialize)]
pub struct Review<'a> {
    pub manifest: Option<&'a Path>,
    pub provider: &'a inspection::Description,
}
#[derive(Serialize)]
pub struct Report {
    pub manifest: Option<PathBuf>,
    pub provider: inspection::Report,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
impl Request {
    /// Only npm needs a manifest. This standalone workflow never reads registration
    /// or observes unrelated live files. All query paths must be supplied explicitly.
    pub fn prepare(self, host: &HostContext) -> Result<Preview> {
        let core = if self.query == inspection::Query::NpmGlobals {
            let path = self
                .manifest
                .as_ref()
                .ok_or_else(|| invalid("npm inspection requires --manifest"))?;
            if !path.is_absolute() || path.to_str().is_none() {
                return Err(invalid(
                    "inspection manifest must be an absolute UTF-8 path",
                ));
            }
            // Bounded no-follow stable-identity read; raw TOML parser diagnostics can
            // contain source lines and must never reach either frontend.
            let (manifest, _) = observation::read_manifest(path).map_err(|_| {
                invalid("inspection manifest unavailable, unsafe or invalid; contents withheld")
            })?;
            inspection::Preview::npm(
                &manifest,
                inspection::NpmBinding {
                    runtime_tool: self
                        .runtime_tool
                        .ok_or_else(|| invalid("npm inspection requires --runtime-tool"))?,
                    node: self.executable,
                    npm_cli: self
                        .npm_cli
                        .ok_or_else(|| invalid("npm inspection requires --npm-cli"))?,
                    prefix: self
                        .prefix
                        .ok_or_else(|| invalid("npm inspection requires --prefix"))?,
                },
                host.home.clone(),
                self.search_dirs,
            )
        } else {
            if self.manifest.is_some()
                || self.runtime_tool.is_some()
                || self.npm_cli.is_some()
                || self.prefix.is_some()
            {
                return Err(invalid(
                    "manifest/runtime/npm CLI/prefix fields apply only to npm inspection",
                ));
            }
            inspection::Preview::new(
                self.query,
                self.executable,
                host.home.clone(),
                self.search_dirs,
            )
        }
        .map_err(|e| invalid(&e.to_string()))?;
        Ok(Preview {
            core,
            manifest: self.manifest,
        })
    }
}
impl Preview {
    pub fn review(&self) -> Review<'_> {
        Review {
            manifest: self.manifest.as_deref(),
            provider: self.core.description(),
        }
    }
    pub fn approve(self) -> Approval {
        Approval {
            core: self.core.approve(),
            manifest: self.manifest,
        }
    }
}
impl Approval {
    pub fn inspect(self) -> Report {
        Report {
            manifest: self.manifest,
            provider: self.core.inspect(),
        }
    }
}
impl Report {
    pub fn available(&self) -> bool {
        matches!(
            self.provider.observation,
            inspection::Observation::Complete { .. }
        )
    }
}
