//! Explicit, bounded metadata-only binary-name observation. Never executes a candidate.
use super::{
    inspection,
    planning::{BinaryObservation, MAX_SELECTED_TOOLS},
};
use crate::{
    Error, Manifest, Result,
    observation::{self, Fingerprint, Kind},
    profiles::selected,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub const MAX_DIRECTORIES: usize = 32;
pub const MAX_PROBES: usize = 32_768;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    DirectoryUnavailable,
    EntryUnavailable,
    Changed,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProbeState {
    Missing,
    Present {
        kind: Kind,
        fingerprint: Fingerprint,
        link_target: Option<PathBuf>,
    },
    Unavailable {
        reason: Failure,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub path: PathBuf,
    pub observation: ProbeState,
}
#[derive(Debug, Serialize)]
pub struct ToolObservation {
    pub binary: String,
    pub observation: BinaryObservation,
    pub probes: Vec<Probe>,
}
#[derive(Debug, Serialize)]
pub struct Directory {
    pub path: PathBuf,
    pub unavailable: Option<Failure>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub directories: Vec<Directory>,
    pub tools: BTreeMap<String, ToolObservation>,
}
impl Report {
    pub fn available(&self) -> bool {
        self.directories.iter().all(|d| d.unavailable.is_none())
            && self
                .tools
                .values()
                .all(|t| !matches!(t.observation, BinaryObservation::Unavailable))
    }
}
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message.into())
}
/// Validate request shape without reading directories or discovering PATH.
pub fn validate(
    manifest: &Manifest,
    active: &BTreeSet<String>,
    directories: &[PathBuf],
) -> Result<()> {
    manifest.validate()?;
    if !active.contains("base") || !active.is_subset(&manifest.profiles.known()) {
        return Err(invalid(
            "binary observation requires resolved known profiles including base",
        ));
    }
    let count = manifest
        .tools
        .iter()
        .filter(|t| selected(&t.profiles, active))
        .count();
    if count > MAX_SELECTED_TOOLS
        || directories.is_empty()
        || directories.len() > MAX_DIRECTORIES
        || count.saturating_mul(directories.len()) > MAX_PROBES
    {
        return Err(invalid(
            "binary observation scope exceeds supported tool/directory/probe limits",
        ));
    }
    let distinct: BTreeSet<_> = directories.iter().collect();
    if distinct.len() != directories.len()
        || directories.iter().any(|p| {
            !inspection::absolute(p) || p.as_os_str().len() > 4096 || p.components().count() > 64
        })
        || std::env::join_paths(directories).is_err()
    {
        return Err(invalid(
            "tool-search directories must be distinct absolute UTF-8 paths without dot components or PATH separators",
        ));
    }
    Ok(())
}
/// Inspect only selected declared names in explicit tool-search directories. These
/// are NOT inferred from provider child PATH or the process environment. No files
/// are opened for content, no links followed, no version/ownership inferred.
pub fn observe(
    manifest: &Manifest,
    active: &BTreeSet<String>,
    directories: &[PathBuf],
) -> Result<Report> {
    validate(manifest, active, directories)?;
    let before: Vec<_> = directories
        .iter()
        .map(|path| {
            observation::inspect(path, false)
                .ok()
                .filter(|e| e.kind == Kind::Directory)
        })
        .collect();
    let mut directory_results: Vec<_> = directories
        .iter()
        .zip(&before)
        .map(|(path, e)| Directory {
            path: path.clone(),
            unavailable: e.is_none().then_some(Failure::DirectoryUnavailable),
        })
        .collect();
    let names: BTreeSet<_> = manifest
        .tools
        .iter()
        .filter(|t| selected(&t.profiles, active))
        .map(|t| t.binary.as_str())
        .collect();
    let mut probes_by_name = BTreeMap::new();
    for name in names {
        let mut probes = vec![];
        for (index, directory) in directories.iter().enumerate() {
            let path = directory.join(name);
            let state = if before[index].is_none() {
                ProbeState::Unavailable {
                    reason: Failure::DirectoryUnavailable,
                }
            } else {
                inspect_entry(&path)
            };
            probes.push(Probe {
                path,
                observation: state,
            });
        }
        probes_by_name.insert(name, probes);
    }
    // Revalidate directory identities/metadata too, so changing parents or entries
    // cannot silently turn a raced/partial scope into a healthy empty observation.
    for (index, path) in directories.iter().enumerate() {
        if let Some(original) = &before[index]
            && observation::inspect(path, false).ok().as_ref() != Some(original)
        {
            directory_results[index].unavailable = Some(Failure::Changed);
        }
    }
    let incomplete_scope = directory_results.iter().any(|d| d.unavailable.is_some());
    let tools = manifest
        .tools
        .iter()
        .filter(|t| selected(&t.profiles, active))
        .map(|tool| {
            let probes = probes_by_name[tool.binary.as_str()].clone();
            let failed = incomplete_scope
                || probes
                    .iter()
                    .any(|p| matches!(p.observation, ProbeState::Unavailable { .. }));
            let paths: Vec<_> = probes
                .iter()
                .filter(|p| matches!(p.observation, ProbeState::Present { .. }))
                .map(|p| p.path.clone())
                .collect();
            let observation = if failed {
                BinaryObservation::Unavailable
            } else if paths.is_empty() {
                BinaryObservation::Missing
            } else {
                BinaryObservation::Candidates { paths }
            };
            (
                tool.id.clone(),
                ToolObservation {
                    binary: tool.binary.clone(),
                    observation,
                    probes,
                },
            )
        })
        .collect();
    Ok(Report {
        directories: directory_results,
        tools,
    })
}
fn inspect_entry(path: &std::path::Path) -> ProbeState {
    let Ok(entry) = observation::inspect(path, false) else {
        return ProbeState::Unavailable {
            reason: Failure::EntryUnavailable,
        };
    };
    if observation::inspect(path, false).ok().as_ref() != Some(&entry) {
        return ProbeState::Unavailable {
            reason: Failure::Changed,
        };
    }
    match (entry.kind, entry.fingerprint) {
        (Kind::Missing, _) => ProbeState::Missing,
        (kind, Some(fingerprint)) => ProbeState::Present {
            kind,
            fingerprint,
            link_target: entry.link_target,
        },
        _ => ProbeState::Unavailable {
            reason: Failure::EntryUnavailable,
        },
    }
}
