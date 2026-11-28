//! Previews and deployment of repository-declared generated files.
mod deploy;
mod record;
mod update;
use crate::{
    Error, Manifest, Result,
    context::HostContext,
    local::LocalConfig,
    manifest::paths_overlap,
    model::{FileContent, FileMode, ManagedFile},
    observation::{self, Entry, Kind},
};
pub use deploy::{Approval, Deployment, DeploymentReview, Published};
use rustix::{fs, io::Errno};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub(crate) use update::ensure_no_pending_update;
pub use update::{
    ExchangeStatus, Update, UpdateApproval, UpdateLocation, UpdateOutcome, UpdateReview,
    pending_update, recover_reviewed_update, recover_update,
};

pub const TEMPLATE_LIMIT: usize = 64 * 1024;
pub const VALUE_LIMIT: usize = 16 * 1024;
const OUTPUT_LIMIT: usize = 128 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Mode(u32);

impl Mode {
    fn declared(permissions: Option<&str>) -> Self {
        Self(permissions.map_or(0o600, |mode| {
            u32::from_str_radix(mode, 8).expect("validated manifest permissions")
        }))
    }

    fn permits_deployment(self) -> bool {
        self.0 & 0o022 == 0
    }

    fn permits_update(self) -> bool {
        self.permits_deployment() && self.0 & 0o400 != 0
    }

    fn label(self) -> String {
        format!("{:04o}", self.0)
    }
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub id: String,
    pub source: Option<PathBuf>,
    pub destination: PathBuf,
    pub inputs: BTreeMap<String, String>,
    pub rendered: String,
    pub destination_status: DestinationStatus,
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_status: Option<DriftStatus>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationStatus {
    Missing,
    Existing,
    Blocked,
}

impl DestinationStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Missing => "Missing",
            Self::Existing => "Existing file. Do not overwrite.",
            Self::Blocked => "Blocked. Unsafe destination.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftStatus {
    Missing,
    MissingParent,
    UnrecordedExisting,
    UpToDate,
    UpdateAvailable,
    LocallyEdited,
    LocalPermissionsChanged,
    LocallyEditedAndPermissionsChanged,
    LocallyDeleted,
    Unsafe,
    Unverifiable,
}

impl DriftStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "New file",
            Self::MissingParent => "Destination folder missing",
            Self::UnrecordedExisting => "Unrecorded existing file",
            Self::UpToDate => "Up to date",
            Self::UpdateAvailable => "Update available",
            Self::LocallyEdited => "Local edits",
            Self::LocalPermissionsChanged => "Local permissions changed",
            Self::LocallyEditedAndPermissionsChanged => "Local edits and permissions changed",
            Self::LocallyDeleted => "Local file deleted",
            Self::Unsafe => "Unsafe destination",
            Self::Unverifiable => "Cannot verify deployment",
        }
    }

    pub fn guidance(self) -> &'static str {
        match self {
            Self::Missing => "Review first deployment to create the file.",
            Self::MissingParent => "Create the destination folder, then preview again.",
            Self::UnrecordedExisting => {
                "Dort leaves the existing file alone. Choose a different destination to deploy."
            }
            Self::UpToDate => "No action needed.",
            Self::UpdateAvailable => {
                "The template output or permissions changed. Dort leaves the deployed file alone."
            }
            Self::LocallyEdited
            | Self::LocalPermissionsChanged
            | Self::LocallyEditedAndPermissionsChanged => {
                "Inspect the local file before making changes. Dort leaves it alone."
            }
            Self::LocallyDeleted => {
                "Restore the file yourself or leave it missing. First deployment cannot recreate it."
            }
            Self::Unsafe => "Check the destination path and access. Dort leaves it alone.",
            Self::Unverifiable => {
                "Check file access, size and Dort's local state, then preview again."
            }
        }
    }
}

impl Preview {
    pub fn proposed_permissions(&self) -> String {
        Mode::declared(self.permissions.as_deref()).label()
    }

    pub fn prepare(host: &HostContext, repository: &Path, id: &str) -> Result<Self> {
        observation::validate_output_path(repository)?;
        let (manifest, _) = observation::read_manifest(&repository.join("dort.toml")).map_err(
            |error| match error {
                Error::Parse(_) => Error::Invalid(
                    "Invalid dort.toml. Use literal strings for generated inputs.".into(),
                ),
                other => other,
            },
        )?;
        let mut preview = Self::from_manifest(host, repository, &manifest, id)?;
        let local = LocalConfig::load(host)?;
        preview.drift_status = Some(preview.drift_status(host, &local, repository, &manifest));
        Ok(preview)
    }

    pub(crate) fn from_manifest(
        host: &HostContext,
        repository: &Path,
        manifest: &Manifest,
        id: &str,
    ) -> Result<Self> {
        let file = find_file(manifest, id)?;
        let destination = host.target_path(&file.target, Some(repository))?;
        Self::render_file(host, repository, manifest, file, destination).map(|(preview, _)| preview)
    }

    fn render_file(
        host: &HostContext,
        repository: &Path,
        manifest: &Manifest,
        file: &ManagedFile,
        destination: PathBuf,
    ) -> Result<(Self, Option<Entry>)> {
        if file.mode != FileMode::Template {
            return Err(Error::Invalid(
                "Preview requires a file with mode = \"template\".".into(),
            ));
        }
        let (source, rendered) = match file.content_for(host.platform) {
            Some(FileContent::Source(path)) => {
                let (text, entry) =
                    read_template(&repository.join(path), repository, &manifest.files)?;
                (Some(entry), render(&text, &file.inputs)?)
            }
            Some(FileContent::Template(text)) => (None, render(text, &file.inputs)?),
            None => {
                return Err(Error::Invalid(
                    "Template needs a source or an inline template.".into(),
                ));
            }
        };
        Ok((
            Self {
                id: file.id.clone(),
                source: source.as_ref().map(|entry| entry.path.clone()),
                destination_status: destination_status(&destination),
                destination,
                inputs: file.inputs.clone(),
                rendered,
                permissions: file.permissions.clone(),
                drift_status: None,
            },
            source,
        ))
    }

    fn drift_status(
        &self,
        host: &HostContext,
        local: &LocalConfig,
        repository: &Path,
        manifest: &Manifest,
    ) -> DriftStatus {
        if !matches!(pending_update(host), Ok(None)) {
            return DriftStatus::Unverifiable;
        }
        let path = &self.destination;
        let Ok(handle) = deploy::destination_handle(host, local, path) else {
            return DriftStatus::Unsafe;
        };
        let receipt = local
            .repositories
            .iter()
            .find(|(_, record)| record.root == repository);
        let stamp = match receipt {
            Some((id, _)) => {
                match record::Slot::new(host, id, &self.id, path).and_then(|slot| slot.load()) {
                    Ok(stamp) => stamp,
                    Err(_) => return DriftStatus::Unverifiable,
                }
            }
            None => None,
        };
        let stat = if handle.missing {
            Err(Errno::NOENT)
        } else {
            fs::statat(
                &handle.directory,
                &handle.name,
                fs::AtFlags::SYMLINK_NOFOLLOW,
            )
        };
        let mode = Mode::declared(self.permissions.as_deref());
        match stat {
            Err(Errno::NOENT) => match stamp {
                Some(_) => DriftStatus::LocallyDeleted,
                None if !mode.permits_deployment() => DriftStatus::Unsafe,
                None if handle.missing => DriftStatus::MissingParent,
                None => DriftStatus::Missing,
            },
            Ok(stat) => match stamp {
                Some(record)
                    if !overlaps_secret_target(host, repository, manifest, path, &stat) =>
                {
                    let drift = record.deployed.observe(
                        path,
                        &handle,
                        &stat,
                        &record::Stamp::new(self.rendered.as_bytes(), mode),
                    );
                    if drift == DriftStatus::UpdateAvailable && !mode.permits_update() {
                        DriftStatus::Unsafe
                    } else {
                        drift
                    }
                }
                None if record::owned_regular(
                    &stat,
                    &observation::Fingerprint::from_stat(&stat),
                ) =>
                {
                    DriftStatus::UnrecordedExisting
                }
                _ => DriftStatus::Unsafe,
            },
            Err(_) => DriftStatus::Unverifiable,
        }
    }
}

fn overlaps_secret_target(
    host: &HostContext,
    repository: &Path,
    manifest: &Manifest,
    path: &Path,
    stat: &fs::Stat,
) -> bool {
    let fp = observation::Fingerprint::from_stat(stat);
    manifest
        .files
        .iter()
        .filter(|file| file.mode == FileMode::Secret)
        .any(|file| {
            host.target_path(&file.target, Some(repository))
                .ok()
                .is_some_and(|secret| {
                    paths_overlap(&secret, path)
                        || observation::inspect(&secret, false)
                            .ok()
                            .and_then(|e| e.fingerprint)
                            .is_some_and(|other| {
                                fp.device == other.device && fp.inode == other.inode
                            })
                })
        })
}

fn find_file<'a>(manifest: &'a Manifest, id: &str) -> Result<&'a ManagedFile> {
    manifest
        .files
        .iter()
        .find(|file| file.id == id)
        .ok_or_else(|| Error::Invalid("No file with that ID in dort.toml.".into()))
}

fn read_template(
    source: &Path,
    repository: &Path,
    files: &[ManagedFile],
) -> Result<(String, Entry)> {
    let mut template = inspect_template(source)?;
    check_secret_sources(&template, repository, files)?;
    let expected = template
        .fingerprint
        .as_ref()
        .expect("validated template fingerprint");
    let unavailable = || {
        Error::Invalid(
            "Template source changed or could not be read safely. Check the source and retry."
                .into(),
        )
    };
    let handle = observation::parent_handle(source).map_err(|_| unavailable())?;
    if !observation::same_parent_identity_and_access(&handle.parents, &template.parents) {
        return Err(unavailable());
    }
    let bytes = observation::read_checked_bounded(&handle, source, expected, TEMPLATE_LIMIT as u64)
        .map_err(|_| unavailable())?;
    if observation::inspect(source, false)
        .ok()
        .is_none_or(|after| {
            after.fingerprint.as_ref() != Some(expected)
                || !observation::same_parent_identity_and_access(&after.parents, &template.parents)
        })
    {
        return Err(unavailable());
    }
    template.sha256 = Some(crate::journal::digest(&bytes));
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Invalid("Template source must be UTF-8 text.".into()))?;
    Ok((text, template))
}

pub(crate) fn input_issues(file: &ManagedFile) -> impl Iterator<Item = String> {
    let mut issues = Vec::new();
    if file.mode == FileMode::Template {
        for (name, value) in &file.inputs {
            if !input_name(name) {
                issues.push(format!("{}: invalid input name; use letters, digits and underscores, starting with a letter or underscore", file.id));
            } else if value.len() > VALUE_LIMIT {
                issues.push(format!("{}: input {name} exceeds 16 KiB", file.id));
            }
        }
        if issues.is_empty()
            && let Some(text) = &file.template
            && let Err(error) = render(text, &file.inputs)
        {
            issues.push(format!("{}: {error}", file.id));
        }
    } else if !file.inputs.is_empty() {
        issues.push(format!("{}: inputs require template mode", file.id));
    }
    issues.into_iter()
}

fn input_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn inspect_template(path: &Path) -> Result<Entry> {
    if observation::sensitive_path(path) {
        return Err(Error::Invalid(
            "Template source is in a sensitive path. Use a non-sensitive source in the repository."
                .into(),
        ));
    }
    let entry = observation::inspect(path, false).map_err(|_| {
        Error::Invalid("Cannot inspect template source safely. Check its path in dort.toml.".into())
    })?;
    if entry.kind == Kind::Missing {
        return Err(Error::Invalid(
            "Template source is missing. Check its path in dort.toml.".into(),
        ));
    }
    let expected = entry
        .fingerprint
        .as_ref()
        .filter(|f| {
            entry.kind == Kind::Regular
                && f.links == 1
                && f.uid == rustix::process::geteuid().as_raw()
        })
        .ok_or_else(|| {
            Error::Invalid("Template source must be an owned regular file without links.".into())
        })?;
    if expected.mode & 0o7000 != 0 {
        return Err(Error::Invalid(
            "Template source has special permission bits. Use an ordinary file.".into(),
        ));
    }
    if expected.size < 0 || expected.size as u64 > TEMPLATE_LIMIT as u64 {
        return Err(Error::Invalid("Template source exceeds 64 KiB.".into()));
    }
    Ok(entry)
}

fn check_secret_sources(template: &Entry, repository: &Path, files: &[ManagedFile]) -> Result<()> {
    let paths = files
        .iter()
        .filter(|file| file.mode == FileMode::Secret)
        .flat_map(ManagedFile::source_paths);
    for path in paths {
        let path = repository.join(path);
        let overlaps = || {
            Error::Invalid(
                "Template source overlaps a declared secret source. Use a separate source file."
                    .into(),
            )
        };
        if paths_overlap(&path, &template.path) {
            return Err(overlaps());
        }
        let cannot_rule_out_alias = || {
            Error::Invalid(
                "Cannot rule out a secret source alias. Use secret source paths without symlinks."
                    .into(),
            )
        };
        let entry = observation::inspect(&path, false).map_err(|_| cannot_rule_out_alias())?;
        if entry.kind == Kind::Symlink {
            return Err(cannot_rule_out_alias());
        }
        if let Some(secret) = entry.fingerprint {
            let same_file = |other: &observation::Fingerprint| {
                other.device == secret.device && other.inode == secret.inode
            };
            if template.fingerprint.as_ref().is_some_and(same_file)
                || template
                    .parents
                    .iter()
                    .any(|parent| same_file(&parent.fingerprint))
            {
                return Err(overlaps());
            }
        }
    }
    Ok(())
}

fn destination_status(path: &Path) -> DestinationStatus {
    match observation::inspect(path, false) {
        Ok(entry) if !entry.sensitive => match entry.kind {
            Kind::Missing => DestinationStatus::Missing,
            Kind::Regular
                if entry.fingerprint.is_some_and(|f| {
                    f.links == 1
                        && f.uid == rustix::process::geteuid().as_raw()
                        && f.mode & 0o7000 == 0
                }) =>
            {
                DestinationStatus::Existing
            }
            _ => DestinationStatus::Blocked,
        },
        _ => DestinationStatus::Blocked,
    }
}

enum Part<'a> {
    Literal(&'a str),
    Input(&'a str),
}

fn parse_template(mut text: &str) -> Result<Vec<Part<'_>>> {
    if text.len() > TEMPLATE_LIMIT {
        return Err(Error::Invalid("Template exceeds 64 KiB.".into()));
    }
    if observation::sensitive_content(text.as_bytes()) {
        return Err(Error::Invalid(
            "Template may contain secrets. Use text without credentials or private keys.".into(),
        ));
    }
    let invalid = || {
        Error::Invalid(
            "Invalid template expression. Use {{ name }} with a single input name.".into(),
        )
    };
    let mut parts = Vec::new();
    while let Some((literal, rest)) = text.split_once("{{") {
        let (expression, remaining) = rest.split_once("}}").ok_or_else(invalid)?;
        let name = expression.trim_matches([' ', '\t']);
        if !input_name(name) {
            return Err(invalid());
        }
        parts.extend([Part::Literal(literal), Part::Input(name)]);
        text = remaining;
    }
    parts.push(Part::Literal(text));
    Ok(parts)
}

pub fn input_names(text: &str) -> Result<BTreeSet<&str>> {
    Ok(parse_template(text)?
        .into_iter()
        .filter_map(|part| match part {
            Part::Input(name) => Some(name),
            Part::Literal(_) => None,
        })
        .collect())
}

fn render(text: &str, inputs: &BTreeMap<String, String>) -> Result<String> {
    let parts = parse_template(text)?;
    if inputs
        .values()
        .any(|value| observation::sensitive_content(value.as_bytes()))
    {
        return Err(Error::Invalid(
            "An input may contain secrets. Use only non-secret values.".into(),
        ));
    }
    let mut rendered = String::new();
    for part in parts {
        let text = match part {
            Part::Literal(text) => text,
            Part::Input(name) => inputs.get(name).ok_or_else(|| {
                Error::Invalid(format!(
                    "Missing input {name}. Add a literal string for it in inputs in dort.toml."
                ))
            })?,
        };
        if rendered.len() + text.len() > OUTPUT_LIMIT {
            return Err(Error::Invalid("Rendered text exceeds 128 KiB.".into()));
        }
        rendered.push_str(text);
    }
    if observation::sensitive_content(rendered.as_bytes()) {
        return Err(Error::Invalid(
            "Rendered text may contain secrets. Use only non-secret content.".into(),
        ));
    }
    Ok(rendered)
}
