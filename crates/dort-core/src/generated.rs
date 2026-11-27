//! Previews and deployment of repository-declared generated files.
mod deploy;
mod inputs;
pub mod local_inputs;
mod record;
mod update;
pub use crate::model::{HostInput, HostInputs};
use crate::{
    Error, Manifest, Result,
    context::HostContext,
    local::LocalConfig,
    manifest::paths_overlap,
    model::{FileContent, FileMode, ManagedFile},
    observation::{self, Entry, Kind},
    template::{self, Context, Dialect, Input, Issue, Template, TemplateError},
};
pub use deploy::{Approval, Deployment, DeploymentReview, Published};
pub use inputs::{InputProvenance, ResolvedInput, ResolvedInputs};
use rustix::{fs, io::Errno};
use serde::Serialize;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub(crate) use update::ensure_no_pending_update;
pub use update::{
    ExchangeStatus, Update, UpdateAction, UpdateApproval, UpdateLocation, UpdateOutcome,
    UpdateReview, pending_update, recover_reviewed_update, recover_update,
};

use template::OUTPUT_LIMIT;
pub use template::{SOURCE_LIMIT as TEMPLATE_LIMIT, VALUE_LIMIT};

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
    pub syntax: Dialect,
    /// All resolved declarations, including unused values. Jinja names are qualified.
    pub resolved_inputs: ResolvedInputs,
    pub rendered: String,
    pub destination_status: DestinationStatus,
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_status: Option<DriftStatus>,
}

/// An unrendered declaration with machine-local values still unset.
#[derive(Debug)]
pub struct LocalInputSetup {
    /// Declared but unset names referenced by the template. Some branches may not use them.
    pub names: Vec<String>,
    guidance: String,
}

impl LocalInputSetup {
    pub fn guidance(&self) -> &str {
        &self.guidance
    }
}

pub(crate) type DeclarationPreview = std::result::Result<Preview, LocalInputSetup>;

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
                "Keep your local changes or review a replacement."
            }
            Self::LocallyDeleted => "Leave the file missing or review recreation.",
            Self::Unsafe => "Check the destination path and access. Dort leaves it alone.",
            Self::Unverifiable => {
                "Check file access, size and Dort's local state, then preview again."
            }
        }
    }
}

impl Preview {
    fn same_render(&self, other: &Self) -> bool {
        self.syntax == other.syntax
            && self.resolved_inputs == other.resolved_inputs
            && self.rendered == other.rendered
    }

    pub fn proposed_permissions(&self) -> String {
        Mode::declared(self.permissions.as_deref()).label()
    }

    pub fn prepare(host: &HostContext, repository: &Path, id: &str) -> Result<Self> {
        observation::validate_output_path(repository)?;
        let (manifest, _) = observation::read_manifest(&repository.join("dort.toml")).map_err(
            |error| match error {
                Error::Parse(_) => Error::Invalid(
                    "Invalid dort.toml. Use literal strings for generated inputs. Check syntax, host_inputs and local_inputs.".into(),
                ),
                other => other,
            },
        )?;
        let local = LocalConfig::load(host)?;
        let repository_id = local.repository_id(repository);
        let mut preview = Self::from_manifest(host, repository, repository_id, &manifest, id)?
            .map_err(|setup| {
                let guidance = setup.guidance();
                Error::Invalid(if repository_id.is_some() {
                    guidance.to_owned()
                } else {
                    format!("Register this repository first. {guidance}")
                })
            })?;
        preview.drift_status = Some(preview.drift_status(host, &local, repository, &manifest));
        Ok(preview)
    }

    pub(crate) fn from_manifest(
        host: &HostContext,
        repository: &Path,
        repository_id: Option<&str>,
        manifest: &Manifest,
        id: &str,
    ) -> Result<DeclarationPreview> {
        let file = find_file(manifest, id)?;
        let destination = host.target_path(&file.target, Some(repository))?;
        Self::render_file(host, repository, repository_id, manifest, file, destination)
            .map(|(preview, _)| preview)
    }

    fn render_file(
        host: &HostContext,
        repository: &Path,
        repository_id: Option<&str>,
        manifest: &Manifest,
        file: &ManagedFile,
        destination: PathBuf,
    ) -> Result<(DeclarationPreview, Option<Entry>)> {
        if file.mode != FileMode::Template {
            return Err(Error::Invalid(
                "Preview requires a file with mode = \"template\".".into(),
            ));
        }
        let (source, text) = match file.content_for(host.platform) {
            Some(FileContent::Source(path)) => {
                let (text, entry) =
                    read_template(&repository.join(path), repository, &manifest.files)?;
                (Some(entry), Cow::Owned(text))
            }
            Some(FileContent::Template(text)) => (None, Cow::Borrowed(text)),
            None => {
                return Err(Error::Invalid(
                    "Template needs a source or an inline template.".into(),
                ));
            }
        };
        let syntax = file.syntax.unwrap_or_default();
        let template = compile(&file.id, syntax, &text)?;
        let local = local_inputs::load(host, repository_id, file)?;
        let (context, resolved_inputs) = inputs::resolve(file, host, &local)?;
        let rendered = if syntax == Dialect::Jinja {
            match template.render(&context) {
                Ok(rendered) => rendered,
                Err(error) if error.issue == Issue::MissingInput => {
                    let setup = missing_jinja_inputs(file, &template, &resolved_inputs, error)?;
                    return Ok((Err(setup), source));
                }
                Err(error) => return Err(renderer_error(syntax, error, None)),
            }
        } else {
            render(&template, syntax, &context)?
        };
        Ok((
            Ok(Self {
                id: file.id.clone(),
                source: source.as_ref().map(|entry| entry.path.clone()),
                destination_status: destination_status(&destination),
                destination,
                inputs: file.inputs.clone(),
                syntax,
                resolved_inputs,
                rendered,
                permissions: file.permissions.clone(),
                drift_status: None,
            }),
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
        if overlaps_secret_target(host, repository, manifest, path, stat.as_ref().ok()) {
            return DriftStatus::Unsafe;
        }
        match stat {
            Err(Errno::NOENT) if handle.missing => DriftStatus::MissingParent,
            Err(Errno::NOENT) => match stamp {
                Some(_) if mode.permits_update() => DriftStatus::LocallyDeleted,
                None if mode.permits_deployment() => DriftStatus::Missing,
                _ => DriftStatus::Unsafe,
            },
            Ok(stat) => match stamp {
                Some(record) => {
                    let drift = record.deployed.observe(
                        path,
                        &handle,
                        &stat,
                        &record::Stamp::new(self.rendered.as_bytes(), mode),
                    );
                    if UpdateAction::for_drift(drift).is_some() && !mode.permits_update() {
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
    stat: Option<&fs::Stat>,
) -> bool {
    let fp = stat.map(observation::Fingerprint::from_stat);
    manifest
        .files
        .iter()
        .filter(|file| file.mode == FileMode::Secret)
        .any(|file| {
            host.target_path(&file.target, Some(repository))
                .ok()
                .is_some_and(|secret| {
                    paths_overlap(&secret, path)
                        || fp.as_ref().is_some_and(|fp| {
                            observation::inspect(&secret, false)
                                .ok()
                                .and_then(|e| e.fingerprint)
                                .is_some_and(|other| {
                                    fp.device == other.device && fp.inode == other.inode
                                })
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
    if file.mode != FileMode::Template {
        for (present, message) in [
            (!file.inputs.is_empty(), "inputs require template mode"),
            (file.syntax.is_some(), "syntax requires template mode"),
            (
                file.host_inputs.is_some(),
                "host_inputs requires template mode",
            ),
            (
                file.local_inputs.is_some(),
                "local_inputs requires template mode",
            ),
        ] {
            if present {
                issues.push(format!("{}: {message}", file.id));
            }
        }
        return issues.into_iter();
    }
    let syntax = file.syntax.unwrap_or_default();
    if file.host_inputs.is_some() && syntax != Dialect::Jinja {
        issues.push(format!(
            "{}: host_inputs requires syntax = \"jinja\"",
            file.id
        ));
    }
    if let Some(names) = &file.local_inputs {
        if syntax != Dialect::Jinja {
            issues.push(format!(
                "{}: local_inputs requires syntax = \"jinja\"",
                file.id
            ));
        }
        if names.iter().any(|name| !template::is_input_name(name)) {
            issues.push(format!("{}: invalid local input name; use letters, digits and underscores, starting with a letter or underscore", file.id));
        }
        if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
            issues.push(format!("{}: list each local input once", file.id));
        }
    }
    for (name, value) in &file.inputs {
        if !template::is_input_name(name) {
            issues.push(format!("{}: invalid input name; use letters, digits and underscores, starting with a letter or underscore", file.id));
        } else if value.len() > VALUE_LIMIT {
            issues.push(format!("{}: input {name} exceeds 16 KiB", file.id));
        }
    }
    if issues.is_empty()
        && let Err(error) = validate_declaration(file, syntax)
    {
        issues.push(match syntax {
            Dialect::Literal => format!("{}: {error}", file.id),
            Dialect::Jinja => error.to_string(),
        });
    }
    issues.into_iter()
}

fn validate_declaration(file: &ManagedFile, syntax: Dialect) -> Result<()> {
    match (syntax, &file.template) {
        (Dialect::Literal, Some(text)) => render(
            &compile(&file.id, syntax, text)?,
            syntax,
            &inputs::manifest_context(file),
        )
        .map(|_| ()),
        (Dialect::Literal, None) => Ok(()),
        (Dialect::Jinja, text) => {
            if let Some(text) = text {
                compile(&file.id, syntax, text)?;
            }
            let mut context = inputs::manifest_context(file);
            if let Some(names) = &file.local_inputs {
                let declarations = names
                    .iter()
                    .map(|name| (name.clone(), Input::Text(String::new())))
                    .collect();
                context.insert("local".into(), Input::Map(declarations));
            }
            template::validate_context(&file.id, syntax, &context)
                .map_err(|error| renderer_error(syntax, error, None))
        }
    }
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

pub fn input_names(text: &str) -> Result<BTreeSet<&str>> {
    Ok(compile("", Dialect::Literal, text)?
        .literal_input_names()
        .collect())
}

/// Discover possible input references without resolving any values or reading the host.
/// Jinja names are qualified, such as `inputs.theme`, `host.os` and `local.editor`.
pub fn input_names_for(id: &str, syntax: Dialect, text: &str) -> Result<BTreeSet<String>> {
    Ok(compile(id, syntax, text)?.input_names().clone())
}

fn compile<'a>(id: &'a str, syntax: Dialect, text: &'a str) -> Result<Template<'a>> {
    Template::compile(id, syntax, text).map_err(|error| renderer_error(syntax, error, None))
}

fn render(template: &Template<'_>, syntax: Dialect, context: &Context) -> Result<String> {
    template.render(context).map_err(|error| {
        let missing = (syntax == Dialect::Literal && error.issue == Issue::MissingInput)
            .then(|| {
                template
                    .literal_input_names()
                    .find(|name| !context.contains_key(*name))
            })
            .flatten();
        renderer_error(syntax, error, missing)
    })
}

fn missing_jinja_inputs(
    file: &ManagedFile,
    template: &Template<'_>,
    resolved: &ResolvedInputs,
    error: TemplateError,
) -> Result<LocalInputSetup> {
    let declared: BTreeSet<_> = file
        .local_inputs
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let mut unset = Vec::new();
    let mut undeclared = Vec::new();
    let mut other = false;
    for name in template
        .input_names()
        .iter()
        .filter(|name| !resolved.contains_key(*name))
    {
        match name.strip_prefix("local.") {
            Some(key) if declared.contains(key) => {
                unset.push(key.to_owned());
            }
            Some(key) => undeclared.push(key),
            None => other = true,
        }
    }
    let mut message = error.to_string();
    if other || (unset.is_empty() && undeclared.is_empty()) {
        message.push_str(" Declare manifest inputs in inputs and host inputs in host_inputs.");
    }
    if !undeclared.is_empty() {
        message.push_str(&format!(
            " Add these names to local_inputs in dort.toml, then set their machine-local values: {}.",
            undeclared.join(", ")
        ));
    }
    if unset.is_empty() {
        return Err(Error::Invalid(message));
    }
    message.push_str(&format!(
        " Set the machine-local inputs the template uses, then preview again: {}.",
        unset.join(", ")
    ));
    Ok(LocalInputSetup {
        names: unset,
        guidance: message,
    })
}

fn renderer_error(syntax: Dialect, error: TemplateError, missing: Option<&str>) -> Error {
    if syntax == Dialect::Jinja {
        let mut message = error.to_string();
        if error.issue == Issue::MissingInput {
            message.push_str(" Declare manifest inputs in inputs and host inputs in host_inputs.");
        }
        return Error::Invalid(message);
    }
    let message = match error.issue {
        Issue::SourceLimit => "Template exceeds 64 KiB.",
        Issue::SensitiveSource => {
            "Template may contain secrets. Use text without credentials or private keys."
        }
        Issue::LiteralSyntax => {
            "Invalid template expression. Use {{ name }} with a single input name."
        }
        Issue::SensitiveInput => "An input may contain secrets. Use only non-secret values.",
        Issue::MissingInput => {
            return Error::Invalid(format!(
                "Missing input {}. Add a literal string for it in inputs in dort.toml.",
                missing.expect("literal render identifies a missing input"),
            ));
        }
        Issue::OutputLimit => "Rendered text exceeds 128 KiB.",
        Issue::SensitiveOutput => "Rendered text may contain secrets. Use only non-secret content.",
        _ => return Error::Invalid(error.issue.to_string()),
    };
    Error::Invalid(message.into())
}
