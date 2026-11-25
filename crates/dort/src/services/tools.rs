//! Shared check workflow. Detection and execution need explicit approval.
use dort_core::{Error, Result, context::HostContext, observation, tools::inspection};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Eq)]
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
    source: Option<Source>,
}
pub struct Approval {
    core: inspection::Approval,
    manifest: Option<PathBuf>,
    source: Option<Source>,
}
#[derive(Serialize)]
pub struct Review<'a> {
    pub manifest: Option<&'a Path>,
    pub provider: &'a inspection::Description,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_probe: Option<&'a inspection::Description>,
}
#[derive(Serialize)]
pub struct Report {
    pub manifest: Option<PathBuf>,
    pub provider: inspection::Report,
    #[serde(skip)]
    pub(super) source: Option<Source>,
}
// Session-only provenance, never serialized trust or execution authority.
pub(super) struct Source {
    fingerprint: Option<observation::Fingerprint>,
    hash: Option<String>,
}
impl Source {
    pub(super) fn from_entry(entry: &observation::Entry) -> Self {
        Self {
            fingerprint: entry.fingerprint.clone(),
            hash: entry.sha256.clone(),
        }
    }
    pub(super) fn matches(&self, entry: &observation::Entry) -> bool {
        self.fingerprint == entry.fingerprint && self.hash == entry.sha256
    }
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
impl Request {
    /// Only npm needs a manifest. This standalone workflow never reads registration
    /// or observes unrelated live files. All query paths must be supplied explicitly.
    pub fn prepare(self, host: &HostContext) -> Result<Preview> {
        let mut source = None;
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
            let (manifest, entry) = observation::read_manifest(path).map_err(|_| {
                invalid("inspection manifest unavailable, unsafe or invalid; contents withheld")
            })?;
            source = Some(Source::from_entry(&entry));
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
            source,
        })
    }
}
impl Preview {
    /// Standalone npm inventory never needs a manifest or a fabricated runtime ID.
    pub fn npm_inventory(
        host: &HostContext,
        executable: PathBuf,
        search_dirs: Vec<PathBuf>,
    ) -> Result<Self> {
        Ok(Self {
            core: inspection::Preview::npm_inventory(executable, host.home.clone(), search_dirs)
                .map_err(|e| invalid(&e.to_string()))?,
            manifest: None,
            source: None,
        })
    }
    pub fn review(&self) -> Review<'_> {
        Review {
            manifest: self.manifest.as_deref(),
            provider: self.core.description(),
            prefix_probe: self.core.prefix_probe(),
        }
    }
    pub fn approve(self) -> Approval {
        Approval {
            core: self.core.approve(),
            manifest: self.manifest,
            source: self.source,
        }
    }
}
impl Approval {
    pub fn inspect(self) -> Report {
        Report {
            manifest: self.manifest,
            provider: self.core.inspect(),
            source: self.source,
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
