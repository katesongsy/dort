//! Current installed-package discovery. Results are not stored as history.
use super::{inspection_defaults::Inputs, tools};
use dort_core::{
    Error, Result,
    context::HostContext,
    observation,
    tools::inspection::{self, Query},
};
use serde::Serialize;
use std::path::PathBuf;
pub fn label(query: Query) -> &'static str {
    match query {
        Query::BrewFormulae => "Homebrew",
        Query::NpmGlobals => "npm",
    }
}
const MAX_SCOPES: usize = 16;
const MAX_RECORDS: usize = 65_536;
#[derive(Serialize)]
pub struct Unavailable {
    pub provider: Query,
    pub reason: String,
}
pub struct Preview {
    queries: Vec<tools::Preview>,
    unavailable: Vec<Unavailable>,
    runtime_hint: Option<String>,
}
pub struct Approval(Preview);
#[derive(Serialize)]
pub struct Review<'a> {
    pub inspections: Vec<tools::Review<'a>>,
    pub unavailable: &'a [Unavailable],
}
#[derive(Serialize)]
pub struct Current {
    pub reports: Vec<tools::Report>,
    pub unavailable: Vec<Unavailable>,
    #[serde(skip)]
    runtime_hint: Option<String>,
}
#[derive(Clone)]
pub struct Seed {
    pub query: Query,
    pub package: String,
    pub version: String,
    pub runtime: Option<String>,
}
pub fn prepare(host: &HostContext, manifest: Option<PathBuf>, input: Inputs) -> Result<Preview> {
    let npm_settings: Vec<_> = input
        .overrides
        .iter()
        .filter(|r| r.query == Query::NpmGlobals)
        .cloned()
        .collect();
    // A known declaration can seed a desired runtime name, never bind this inventory.
    let runtime_hint = manifest
        .as_ref()
        .and_then(|path| observation::read_manifest(path).ok())
        .and_then(|(m, _)| {
            let mut nodes = m.tools.iter().filter(|t| t.binary == "node");
            let id = nodes.next()?.id.clone();
            nodes.next().is_none().then_some(id)
        });
    let mut npm_inventory = npm_settings.is_empty().then(|| input.npm_inventory(host));
    let defaults = input.detect(host, None);
    let mut queries = vec![];
    let mut unavailable = vec![];
    for choice in defaults.choices {
        let Some(request) = choice.request else {
            continue;
        };
        let provider = request.query;
        if provider == Query::NpmGlobals
            && let Some(preview) = npm_inventory.take()
        {
            match preview {
                Ok(preview) => queries.push(preview),
                Err(error) => unavailable.push(Unavailable {
                    provider,
                    reason: error.to_string(),
                }),
            }
            continue;
        }
        let requests = if provider == Query::NpmGlobals && !npm_settings.is_empty() {
            npm_settings.clone()
        } else {
            vec![request]
        };
        for request in requests {
            if queries.len() == MAX_SCOPES {
                return Err(Error::Invalid(
                    "Too many package-manager settings. Check a smaller environment.".into(),
                ));
            }
            match request.prepare(host) {
                Ok(preview) => queries.push(preview),
                Err(_) => unavailable.push(Unavailable {
                    provider,
                    reason: choice.reason.into(),
                }),
            }
        }
    }
    Ok(Preview {
        queries,
        unavailable,
        runtime_hint,
    })
}
impl Preview {
    pub fn review(&self) -> Review<'_> {
        Review {
            inspections: self.queries.iter().map(tools::Preview::review).collect(),
            unavailable: &self.unavailable,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }
    pub fn approve(self) -> Approval {
        Approval(self)
    }
}
impl Approval {
    pub fn review(&self) -> Review<'_> {
        self.0.review()
    }
    pub fn inspect(self) -> Result<Current> {
        let mut reports = vec![];
        let mut count = 0;
        for preview in self.0.queries {
            let report = preview.approve().inspect();
            if let inspection::Observation::Complete { installations } =
                &report.provider.observation
            {
                count += installations.len();
            }
            if count > MAX_RECORDS {
                return Err(Error::Invalid(
                    "The check returned too many results. Nothing was kept.".into(),
                ));
            }
            reports.push(report);
        }
        Ok(Current {
            reports,
            unavailable: self.0.unavailable,
            runtime_hint: self.0.runtime_hint,
        })
    }
}
impl Current {
    pub fn available(&self) -> bool {
        !self.reports.is_empty() && self.reports.iter().all(tools::Report::available)
    }
    pub fn rows(&self) -> Vec<(usize, usize)> {
        self.reports
            .iter()
            .enumerate()
            .flat_map(|(r, report)| match &report.provider.observation {
                inspection::Observation::Complete { installations } => {
                    (0..installations.len()).map(|p| (r, p)).collect::<Vec<_>>()
                }
                _ => vec![],
            })
            .collect()
    }
    pub fn package(
        &self,
        row: (usize, usize),
    ) -> Option<(&inspection::Description, &inspection::Installation)> {
        let report = self.reports.get(row.0)?;
        let inspection::Observation::Complete { installations } = &report.provider.observation
        else {
            return None;
        };
        Some((&report.provider.description, installations.get(row.1)?))
    }
    pub fn seed(&self, row: (usize, usize)) -> Result<Seed> {
        let (scope, package) = self
            .package(row)
            .ok_or_else(|| Error::Invalid("Select an installed package first.".into()))?;
        Ok(Seed {
            query: scope.query,
            package: package.package.clone(),
            version: package.version.clone(),
            runtime: scope
                .npm_binding
                .as_ref()
                .map(|b| b.runtime_tool.clone())
                .or_else(|| {
                    (scope.query == Query::NpmGlobals)
                        .then(|| self.runtime_hint.clone())
                        .flatten()
                }),
        })
    }
}
