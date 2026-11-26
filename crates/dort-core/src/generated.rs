//! Read-only previews of repository-declared generated files.
use crate::{
    Error, Result,
    context::HostContext,
    manifest::paths_overlap,
    model::{FileMode, ManagedFile},
    observation::{self, Entry, Kind},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const RECIPE_LIMIT: u64 = 64 * 1024;
const VALUE_LIMIT: usize = 16 * 1024;
const OUTPUT_LIMIT: usize = 128 * 1024;

#[derive(Debug, Serialize)]
pub struct Preview {
    pub id: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub inputs: BTreeMap<String, String>,
    pub rendered: String,
    pub destination_status: DestinationStatus,
    pub permissions: Option<String>,
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

impl Preview {
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
        let file = manifest
            .files
            .iter()
            .find(|file| file.id == id)
            .ok_or_else(|| Error::Invalid("No file with that ID in dort.toml.".into()))?;
        if file.mode != FileMode::Template {
            return Err(Error::Invalid(
                "Preview requires a file with mode = \"template\".".into(),
            ));
        }
        let source = repository.join(file.source_for(host.platform));
        let destination = host.target_path(&file.target, Some(repository))?;
        let recipe = inspect_recipe(&source)?;
        check_secret_sources(&recipe, repository, &manifest.files)?;
        let expected = recipe
            .fingerprint
            .as_ref()
            .expect("validated recipe fingerprint");
        let unavailable = || {
            Error::Invalid(
                "Template source changed or could not be read safely. Check the source and retry."
                    .into(),
            )
        };
        let handle = observation::parent_handle(&source).map_err(|_| unavailable())?;
        if !observation::same_parent_identity_and_access(&handle.parents, &recipe.parents) {
            return Err(unavailable());
        }
        let bytes = observation::read_checked_bounded(&handle, &source, expected, RECIPE_LIMIT)
            .map_err(|_| unavailable())?;
        if observation::inspect(&source, false)
            .ok()
            .is_none_or(|after| {
                after.fingerprint.as_ref() != Some(expected)
                    || !observation::same_parent_identity_and_access(
                        &after.parents,
                        &recipe.parents,
                    )
            })
        {
            return Err(unavailable());
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Invalid("Template source must be UTF-8 text.".into()))?;
        if observation::sensitive_content(&bytes) {
            return Err(Error::Invalid("Template source may contain secrets. Use a source with no credentials or private keys.".into()));
        }
        let rendered = render(text, &file.inputs)?;
        Ok(Self {
            id: file.id.clone(),
            source,
            destination_status: destination_status(&destination),
            destination,
            inputs: file.inputs.clone(),
            rendered,
            permissions: file.permissions.clone(),
        })
    }
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

fn inspect_recipe(path: &Path) -> Result<Entry> {
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
    if expected.size < 0 || expected.size as u64 > RECIPE_LIMIT {
        return Err(Error::Invalid("Template source exceeds 64 KiB.".into()));
    }
    Ok(entry)
}

fn check_secret_sources(recipe: &Entry, repository: &Path, files: &[ManagedFile]) -> Result<()> {
    let paths = files
        .iter()
        .filter(|file| file.mode == FileMode::Secret)
        .flat_map(|file| std::iter::once(&file.source).chain(file.overrides.values()));
    for path in paths {
        let path = repository.join(path);
        let overlaps = || {
            Error::Invalid(
                "Template source overlaps a declared secret source. Use a separate source file."
                    .into(),
            )
        };
        if paths_overlap(&path, &recipe.path) {
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
            if recipe.fingerprint.as_ref().is_some_and(same_file)
                || recipe
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

fn render(mut text: &str, inputs: &BTreeMap<String, String>) -> Result<String> {
    let mut rendered = String::new();
    let mut append = |part: &str| {
        if rendered.len() + part.len() > OUTPUT_LIMIT {
            return Err(Error::Invalid("Rendered text exceeds 128 KiB.".into()));
        }
        rendered.push_str(part);
        Ok(())
    };
    let invalid = || {
        Error::Invalid(
            "Invalid template expression. Use {{ name }} with a single input name.".into(),
        )
    };
    while let Some((literal, rest)) = text.split_once("{{") {
        let (expression, remaining) = rest.split_once("}}").ok_or_else(invalid)?;
        let name = expression.trim_matches([' ', '\t']);
        if !input_name(name) {
            return Err(invalid());
        }
        let value = inputs.get(name).ok_or_else(|| {
            Error::Invalid(format!(
                "Missing input {name}. Add a literal string for it in inputs in dort.toml."
            ))
        })?;
        append(literal)?;
        append(value)?;
        text = remaining;
    }
    append(text)?;
    Ok(rendered)
}
