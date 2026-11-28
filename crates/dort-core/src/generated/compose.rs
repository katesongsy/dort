//! One request that creates a template file and the mapping that renders it.
use crate::{
    Error, Result,
    context::HostContext,
    journal,
    manifest_edit::{self, FileChange},
    model::{FileMode, ManagedFile},
    observation::{self, Kind},
    state::WriterLock,
    template_files,
};
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

const DIRECTORY: &str = "templates";
const PERMISSIONS: u32 = 0o644;
const AGAIN: &str = "Review the mapping again.";

fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid(reason.into())
}

pub struct Preview {
    root: PathBuf,
    source: PathBuf,
    text: String,
    /// Set when this request creates the file and, when absent, its folder.
    file: Option<template_files::Preview>,
    manifest: manifest_edit::Preview,
}

pub struct Review<'a> {
    /// Folder and file effect. None when the reviewed file is already in place.
    pub template: Option<template_files::Review<'a>>,
    pub source: &'a Path,
    pub permissions: u32,
    /// Rendered output of the reviewed manifest replacement, when inputs are set.
    pub rendered: Option<&'a super::Preview>,
}

pub struct Approval(Preview);

#[derive(Debug)]
pub enum Outcome {
    /// The template file and its mapping are written.
    Created {
        folder_created: bool,
        backup: Option<PathBuf>,
    },
    /// The reviewed file was already in place. Only the mapping is written.
    Mapped { backup: Option<PathBuf> },
    /// The template file is on disk and the mapping is not written.
    MappingNotWritten {
        file: PathBuf,
        folder_created: bool,
        reason: String,
        remedy: &'static str,
    },
}

impl Outcome {
    pub fn human(&self) -> String {
        match self {
            Self::Created { .. } => "Template file and mapping written.".into(),
            Self::Mapped { .. } => "Template file was already in place. Mapping written.".into(),
            Self::MappingNotWritten {
                file,
                folder_created,
                reason,
                remedy,
            } => format!(
                "{}Template file {:?} is in place. Mapping not written: {reason} {remedy}",
                if *folder_created {
                    "templates/ made. "
                } else {
                    ""
                },
                file
            ),
        }
    }
}

impl Preview {
    pub fn prepare(
        host: &HostContext,
        root: &Path,
        mapping: ManagedFile,
        text: String,
    ) -> Result<Self> {
        let source = checked_source(root, &mapping)?;
        let in_place = in_place(host, root, &source, &text)?;
        let file = match in_place {
            true => None,
            false => Some(template_files::Preview::prepare(
                host,
                root,
                template_files::Change::Create(source.clone()),
                text.clone(),
            )?),
        };
        let pending = file.is_some().then_some((source.as_path(), text.as_str()));
        let change = manifest_edit::Change::File(Box::new(FileChange {
            mapping,
            before: None,
        }));
        let manifest = manifest_edit::Preview::prepare_with_source(host, root, &change, pending)?;
        Ok(Self {
            root: root.to_owned(),
            source,
            text,
            file,
            manifest,
        })
    }

    pub fn review(&self) -> Review<'_> {
        Review {
            template: self.file.as_ref().map(template_files::Preview::review),
            source: &self.source,
            permissions: PERMISSIONS,
            rendered: self.manifest.generated(),
        }
    }

    /// Set when the mapping needs local input values before it can render.
    pub fn local_input_setup(&self) -> Option<&super::LocalInputSetup> {
        self.manifest.local_input_setup()
    }

    pub fn human(&self) -> String {
        let mut text = format!(
            "Repository: {:?}\nTemplate file: {:?}\n",
            self.root, self.source
        );
        if self
            .file
            .as_ref()
            .is_some_and(|file| file.review().folder.is_some())
        {
            text.push_str("\nFolder: templates/ (0755)\n");
        }
        text.push_str(&match self.manifest.generated() {
            Some(rendered) => format!("Home destination: {:?}\n", rendered.destination),
            None => "No rendered output yet. Set the local input values first.\n".into(),
        });
        text.push_str("\nSave the template file and its mapping in dort.toml.\n");
        text
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }

    fn recheck(&self, host: &HostContext) -> Result<bool> {
        in_place(host, &self.root, &self.source, &self.text)
    }
}

impl Approval {
    pub fn execute(self, host: &HostContext) -> Result<Outcome> {
        self.execute_before_manifest(host, || {})
    }

    fn execute_before_manifest(
        self,
        host: &HostContext,
        before_manifest: impl FnOnce(),
    ) -> Result<Outcome> {
        let p = self.0;
        let _lock = WriterLock::acquire(host)?;
        journal::ensure_writable(host)?;
        let (created, folder_created) = match p.file {
            Some(file) => match file.approve().execute_locked(host)? {
                template_files::Outcome::Created { folder_created } => (true, folder_created),
                template_files::Outcome::NeedsAttention {
                    folder_created,
                    competing_change,
                    ..
                } => {
                    return Ok(Outcome::MappingNotWritten {
                        file: p.source,
                        folder_created,
                        reason: if competing_change {
                            "Another change touched the template file.".into()
                        } else {
                            "Dort could not confirm the template file was saved.".into()
                        },
                        remedy: "Check the template file in Files, then review the mapping again.",
                    });
                }
                _ => return Err(invalid(format!("Review the template file. {AGAIN}"))),
            },
            None => {
                if !p.recheck(host)? {
                    return Err(invalid(format!("The template file changed. {AGAIN}")));
                }
                (false, false)
            }
        };
        before_manifest();
        let backup = match p.manifest.approve().execute_locked(host) {
            Ok(backup) => backup,
            Err(error) => {
                return Ok(Outcome::MappingNotWritten {
                    file: p.source,
                    folder_created,
                    reason: error.to_string(),
                    remedy: "The template file stays in place.",
                });
            }
        };
        Ok(if created {
            Outcome::Created {
                folder_created,
                backup,
            }
        } else {
            Outcome::Mapped { backup }
        })
    }
}

fn checked_source(root: &Path, mapping: &ManagedFile) -> Result<PathBuf> {
    if mapping.mode != FileMode::Template {
        return Err(invalid("Only template mappings use a templates/ file."));
    }
    if mapping.template.is_some() {
        return Err(invalid(
            "Remove the inline template. The mapping renders the template file.",
        ));
    }
    if !mapping.overrides.is_empty() {
        return Err(invalid(
            "Remove platform source overrides. A templates/ file renders for every platform.",
        ));
    }
    let source = mapping
        .source
        .clone()
        .ok_or_else(|| invalid("Enter a template file path."))?;
    let name = source.strip_prefix(DIRECTORY).map_err(|_| {
        invalid("Template files live under templates/. Use templates/<name>.jinja.")
    })?;
    let mut parts = name.components();
    let single = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
    if !single || source.extension() != Some(OsStr::new("jinja")) {
        return Err(invalid("Use templates/<name>.jinja for the template file."));
    }
    let (manifest, _) =
        observation::read_manifest(&root.join("dort.toml")).map_err(|error| match error {
            Error::Parse(_) => invalid("Invalid manifest. Contents withheld."),
            other => other,
        })?;
    let users: Vec<&str> = manifest
        .files
        .iter()
        .filter(|file| file.source_paths().any(|path| path == &source))
        .map(|file| file.id.as_str())
        .collect();
    if !users.is_empty() {
        return Err(invalid(format!(
            "These mappings already use {:?}: {}. Use a new file or edit one of those mappings.",
            source,
            users.join(", ")
        )));
    }
    Ok(source)
}

/// A reviewed file already in place must hold the reviewed bytes and mode.
fn in_place(host: &HostContext, root: &Path, source: &Path, text: &str) -> Result<bool> {
    if observation::inspect(&root.join(source), false)?.kind == Kind::Missing {
        return Ok(false);
    }
    let current = template_files::show(host, root, source).map_err(|_| {
        invalid(format!(
            "A file exists at {source:?} and Dort cannot use it. Check its owner, links, permissions and contents."
        ))
    })?;
    if current.text() != text || current.permissions() != PERMISSIONS {
        return Err(invalid(format!(
            "A different file already exists at {source:?}. Review the existing template or choose another name."
        )));
    }
    Ok(true)
}
