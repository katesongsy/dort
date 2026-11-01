//! Pure preview planner. Observation is explicit; `plan` performs no filesystem reads.
use crate::{
    Error, Manifest, Result,
    context::HostContext,
    model::{FileMode, ManagedFile},
    observation::{self, Entry, Kind},
    profiles::selected,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Request {
    Apply,
    Adopt { mapping: ManagedFile },
    Unmanage { id: String, mode: UnmanageMode },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmanageMode {
    Keep,
    Remove,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileSnapshot {
    pub mapping: ManagedFile,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub source: Option<Entry>,
    pub destination: Option<Entry>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub user_id: u32,
    pub host: HostContext,
    pub repository: PathBuf,
    pub manifest: Entry,
    pub local_config: Entry,
    pub active_profiles: BTreeSet<String>,
    pub request: Request,
    pub files: Vec<FileSnapshot>,
}

/// Observe only explicitly selected mappings. Unsafe paths become per-file blockers.
/// This is not an atomic system-wide snapshot: a future executor must lock and revalidate.
pub fn observe(
    host: &HostContext,
    repository: &Path,
    profiles: &[String],
    request: Request,
) -> Result<Snapshot> {
    observation::validate_output_path(repository)?;
    let (manifest, manifest_entry) = observation::read_manifest(&repository.join("dort.toml"))?;
    let active = manifest.validate_for_host(host, profiles, Some(repository))?;
    if let Request::Adopt { mapping } = &request {
        validate_adoption(&manifest, mapping, host, profiles, repository)?;
    }
    let mappings: Vec<_> = match &request {
        Request::Adopt { mapping } => vec![mapping.clone()],
        Request::Apply => manifest
            .files
            .iter()
            .filter(|f| selected(&f.profiles, &active))
            .cloned()
            .collect(),
        Request::Unmanage { id, .. } => vec![
            manifest
                .files
                .iter()
                .find(|f| &f.id == id && selected(&f.profiles, &active))
                .cloned()
                .ok_or_else(|| Error::Invalid(format!("no active file mapping: {id}")))?,
        ],
    };
    let mut files = vec![];
    for mapping in mappings {
        let source_path = repository.join(
            mapping
                .overrides
                .get(&host.platform)
                .unwrap_or(&mapping.source),
        );
        let target_path = host.target_path(&mapping.target, Some(repository))?;
        let mut blockers = vec![];
        if mapping.mode != FileMode::Link {
            blockers.push(format!(
                "{:?} deployment is unsupported; never treated as link",
                mapping.mode
            ));
        }
        if !mapping.requires.is_empty() {
            blockers.push("tool prerequisites cannot yet be verified".into());
        }
        let hash = blockers.is_empty();
        let mut capture = |path: &Path| match observation::inspect(path, hash) {
            Ok(entry) => Some(entry),
            Err(error) => {
                blockers.push(error.to_string());
                None
            }
        };
        let source = capture(&source_path);
        let destination = capture(&target_path);
        files.push(FileSnapshot {
            mapping,
            source_path,
            target_path,
            source,
            destination,
            blockers,
        });
    }
    files.sort_by(|a, b| a.mapping.id.cmp(&b.mapping.id));
    Ok(Snapshot {
        user_id: rustix::process::geteuid().as_raw(),
        host: host.clone(),
        repository: repository.to_owned(),
        manifest: manifest_entry,
        local_config: observation::inspect(&host.local_config_path(), false)?,
        active_profiles: active,
        request,
        files,
    })
}

fn validate_adoption(
    manifest: &Manifest,
    mapping: &ManagedFile,
    host: &HostContext,
    profiles: &[String],
    repository: &Path,
) -> Result<()> {
    let mut candidate = manifest.clone();
    candidate.files.push(mapping.clone());
    let active = candidate.validate_for_host(host, profiles, Some(repository))?;
    if !selected(&mapping.profiles, &active) {
        return Err(Error::Invalid("adoption mapping must be active".into()));
    }
    if !mapping.overrides.is_empty() {
        return Err(Error::Invalid(
            "adoption cannot create platform overrides".into(),
        ));
    }
    for existing in &manifest.files {
        for source in std::iter::once(&existing.source).chain(existing.overrides.values()) {
            if source.starts_with(&mapping.source) || mapping.source.starts_with(source) {
                return Err(Error::Invalid(
                    "adoption source overlaps an existing mapping".into(),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Effect {
    Backup {
        path: PathBuf,
    },
    EnsureParents {
        path: PathBuf,
        permissions: String,
    },
    CopySource {
        from: PathBuf,
        to: PathBuf,
        permissions: String,
    },
    AddMapping {
        manifest: PathBuf,
        mapping: ManagedFile,
    },
    RemoveMapping {
        manifest: PathBuf,
        id: String,
    },
    Link {
        target: PathBuf,
        source: PathBuf,
    },
    Materialize {
        source: PathBuf,
        target: PathBuf,
        permissions: String,
    },
    RemoveDestination {
        target: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Change,
    NoOp,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilePlan {
    pub id: String,
    pub outcome: Outcome,
    pub effects: Vec<Effect>,
    pub blockers: Vec<String>,
    pub destructive: bool,
    pub requires_removal_acknowledgement: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub schema: u32,
    pub preview_only: bool,
    pub inputs: Snapshot,
    pub files: Vec<FilePlan>,
    pub warnings: Vec<String>,
}

fn regular(entry: &Entry, label: &str, user_id: u32, blockers: &mut Vec<String>) {
    if entry.kind != Kind::Regular {
        blockers.push(format!(
            "{label} must be a regular file (found {:?})",
            entry.kind
        ));
        return;
    }
    if entry
        .fingerprint
        .as_ref()
        .is_none_or(|f| f.links != 1 || f.uid != user_id || f.mode & 0o7000 != 0)
    {
        blockers.push(format!(
            "{label} must be owned by this user, single-link, without special permission bits"
        ));
    }
    if entry.sensitive {
        blockers.push(format!(
            "{label} appears sensitive; ordinary link/adoption is refused"
        ));
    }
    if entry.sha256.is_none() {
        blockers.push(format!("{label} has no safe content fingerprint"));
    }
}

fn permissions(entry: &Entry) -> String {
    format!(
        "{:04o}",
        entry.fingerprint.as_ref().map_or(0, |f| f.mode & 0o777)
    )
}

/// Identical snapshots produce identical plans. No reads, writes, clocks or random IDs.
pub fn plan(snapshot: Snapshot) -> Plan {
    let mut files = vec![];
    let manifest = snapshot.manifest.path.clone();
    for file in &snapshot.files {
        let mut result = FilePlan {
            id: file.mapping.id.clone(),
            outcome: Outcome::Change,
            effects: vec![],
            blockers: file.blockers.clone(),
            destructive: false,
            requires_removal_acknowledgement: false,
        };
        if file.mapping.mode != FileMode::Link {
            result.blockers.push("only link mode is supported".into());
        }
        if !file.mapping.requires.is_empty() {
            result
                .blockers
                .push("tool prerequisite verification is unsupported".into());
        }
        if let (Some(source), Some(destination)) = (&file.source, &file.destination) {
            if destination.sensitive {
                result.blockers.push(
                    "destination appears sensitive; ordinary link operations are refused".into(),
                );
            }
            if destination
                .fingerprint
                .as_ref()
                .is_some_and(|f| f.links != 1 || f.uid != snapshot.user_id)
            {
                result
                    .blockers
                    .push("destination must be owned by this user and not hard-linked".into());
            }
            match &snapshot.request {
                Request::Adopt { .. } => {
                    if source.kind != Kind::Missing {
                        result
                            .blockers
                            .push("adoption source must not exist".into());
                    }
                    regular(
                        destination,
                        "destination",
                        snapshot.user_id,
                        &mut result.blockers,
                    );
                    if observation::sensitive_path(&file.source_path) {
                        result
                            .blockers
                            .push("adoption source path appears sensitive".into());
                    }
                    result.destructive = true;
                    result.effects = vec![
                        Effect::Backup {
                            path: manifest.clone(),
                        },
                        Effect::EnsureParents {
                            path: file.source_path.clone(),
                            permissions: "0700".into(),
                        },
                        Effect::CopySource {
                            from: file.target_path.clone(),
                            to: file.source_path.clone(),
                            permissions: permissions(destination),
                        },
                        Effect::AddMapping {
                            manifest: manifest.clone(),
                            mapping: file.mapping.clone(),
                        },
                        Effect::Backup {
                            path: file.target_path.clone(),
                        },
                        Effect::Link {
                            target: file.target_path.clone(),
                            source: file.source_path.clone(),
                        },
                    ];
                }
                Request::Apply => {
                    regular(source, "source", snapshot.user_id, &mut result.blockers);
                    if destination.kind == Kind::Symlink
                        && observation::link_path(destination).as_ref() == Some(&file.source_path)
                    {
                        result.outcome = Outcome::NoOp;
                    } else {
                        match destination.kind {
                            Kind::Missing => {},
                            Kind::Regular => {
                                regular(destination, "destination", snapshot.user_id, &mut result.blockers);
                                result.effects.push(Effect::Backup { path: file.target_path.clone() });
                                result.destructive = true;
                            }
                            _ => result.blockers.push("destination is a foreign/broken link, directory or special file; replacement is refused".into()),
                        }
                        result.effects.push(Effect::EnsureParents {
                            path: file.target_path.clone(),
                            permissions: "0700".into(),
                        });
                        result.effects.push(Effect::Link {
                            target: file.target_path.clone(),
                            source: file.source_path.clone(),
                        });
                    }
                }
                Request::Unmanage { mode, .. } => {
                    regular(source, "source", snapshot.user_id, &mut result.blockers);
                    if destination.kind != Kind::Symlink
                        || observation::link_path(destination).as_ref() != Some(&file.source_path)
                    {
                        result.blockers.push("unmanage requires the currently correct managed link; destination drift is refused".into());
                    }
                    result.destructive = true;
                    result.requires_removal_acknowledgement = *mode == UnmanageMode::Remove;
                    result.effects.push(Effect::Backup {
                        path: manifest.clone(),
                    });
                    result.effects.push(Effect::Backup {
                        path: file.target_path.clone(),
                    });
                    result.effects.push(match mode {
                        UnmanageMode::Keep => Effect::Materialize {
                            source: file.source_path.clone(),
                            target: file.target_path.clone(),
                            permissions: permissions(source),
                        },
                        UnmanageMode::Remove => Effect::RemoveDestination {
                            target: file.target_path.clone(),
                        },
                    });
                    result.effects.push(Effect::RemoveMapping {
                        manifest: manifest.clone(),
                        id: file.mapping.id.clone(),
                    });
                }
            }
        } else if result.blockers.is_empty() {
            result.blockers.push("incomplete observation".into());
        }
        if !result.blockers.is_empty() {
            result.outcome = Outcome::Blocked;
            result.effects.clear();
            result.destructive = false;
            result.requires_removal_acknowledgement = false;
        }
        files.push(result);
    }
    Plan { schema: 1, preview_only: true, inputs: snapshot, files, warnings: vec!["Preview only: planning creates no backups or journals. Core execution requires explicit approval, the writer lock and independent re-observation.".into(), "Backups must be durable and transaction intent persisted before destructive effects. Adoption's source copy, manifest edit and link replacement are one recoverable transaction, not independent writes.".into(), "Sensitivity checks are heuristic. Review content privately before adoption; no file contents are printed. Unmanage always retains the repository source.".into()] }
}

impl Plan {
    pub fn has_blockers(&self) -> bool {
        self.files.iter().any(|f| !f.blockers.is_empty())
    }

    pub fn human(&self) -> String {
        use std::fmt::Write;
        let mut text = String::from("Read-only plan (schema 1; not executable)\n");
        for file in &self.files {
            let _ = writeln!(text, "{}: {:?}", file.id, file.outcome);
            if let Some(input) = self
                .inputs
                .files
                .iter()
                .find(|input| input.mapping.id == file.id)
            {
                let _ = writeln!(
                    text,
                    "  Source: {} (permissions: {})",
                    input.source_path.display(),
                    input
                        .source
                        .as_ref()
                        .filter(|e| e.fingerprint.is_some())
                        .map_or("absent/unknown".into(), permissions)
                );
                let _ = writeln!(
                    text,
                    "  Destination: {} (permissions: {})",
                    input.target_path.display(),
                    input
                        .destination
                        .as_ref()
                        .filter(|e| e.fingerprint.is_some())
                        .map_or("absent/unknown".into(), permissions)
                );
            }
            for blocker in &file.blockers {
                let _ = writeln!(text, "  BLOCKED: {blocker}");
            }
            for effect in &file.effects {
                let _ = writeln!(text, "  {effect:?}");
            }
            if file.destructive {
                text.push_str(
                    "  Destructive destination replacement/removal; durable backup required.\n",
                );
            }
            if file.requires_removal_acknowledgement {
                text.push_str("  Separate destructive-removal acknowledgement required.\n");
            }
        }
        for warning in &self.warnings {
            let _ = writeln!(text, "Warning: {warning}");
        }
        text
    }
}
