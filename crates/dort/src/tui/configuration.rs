use dort_core::{
    Error, Result,
    manifest_edit::{Change, FileChange, Profile},
    model::{FileMode, ManagedFile},
    platform::Platform,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Profile,
    DeleteProfile,
    File,
    EditFile,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Define profile",
            Self::DeleteProfile => "Delete profile",
            Self::File => "Add file mapping",
            Self::EditFile => "Edit file mapping",
        }
    }

    fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Profile => &[
                "Profile name",
                "Existing name (edit or rename)",
                "Defaults: repository, macos, linux",
            ],
            Self::DeleteProfile => &["Profile name"],
            Self::File => Self::FILE_FIELDS,
            Self::EditFile => &Self::FILE_FIELDS[1..],
        }
    }

    const FILE_FIELDS: &'static [&'static str] = &[
        "Mapping ID",
        "Repository source",
        "Home destination",
        "Profiles (blank uses base)",
        "Required tools",
        "macOS source (optional)",
        "Linux source (optional)",
    ];
}

pub struct Draft {
    pub kind: Kind,
    pub focus: usize,
    fields: Vec<String>,
    mapping: Option<ManagedFile>,
}

pub(super) fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Draft {
    pub fn new(kind: Kind, mapping: Option<ManagedFile>) -> Self {
        let mut fields = vec![String::new(); kind.fields().len()];
        if let Some(file) = &mapping {
            fields = vec![
                file.source.display().to_string(),
                file.target.display().to_string(),
                file.profiles.join(", "),
                file.requires.join(", "),
                file.overrides
                    .get(&Platform::Macos)
                    .map_or_else(String::new, |p| p.display().to_string()),
                file.overrides
                    .get(&Platform::Linux)
                    .map_or_else(String::new, |p| p.display().to_string()),
            ];
        }
        Self {
            kind,
            focus: 0,
            fields,
            mapping,
        }
    }

    pub fn fields(&self) -> Vec<(&str, &str)> {
        self.kind
            .fields()
            .iter()
            .copied()
            .zip(self.fields.iter().map(String::as_str))
            .collect()
    }

    pub fn focus_at(&mut self, index: usize) {
        self.focus = index.min(self.fields.len() - 1);
    }

    pub fn key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::KeyCode;
        match key.code {
            KeyCode::Tab => self.focus = (self.focus + 1) % self.fields.len(),
            KeyCode::BackTab => {
                self.focus = (self.focus + self.fields.len() - 1) % self.fields.len()
            }
            _ => super::shell::edit_text(&mut self.fields[self.focus], key),
        }
    }

    pub fn change(&self) -> Result<Change> {
        let value = |i: usize| self.fields[i].trim();
        let required = |index: usize| -> Result<&str> {
            let text = value(index);
            if text.is_empty() {
                Err(Error::Invalid(format!(
                    "Enter {}.",
                    self.kind.fields()[index]
                )))
            } else {
                Ok(text)
            }
        };
        Ok(match self.kind {
            Kind::DeleteProfile => Change::Profile {
                old: Some(required(0)?.into()),
                new: None,
            },
            Kind::Profile => {
                let flags = list(value(2));
                if flags
                    .iter()
                    .any(|flag| !matches!(flag.as_str(), "repository" | "macos" | "linux"))
                {
                    return Err(Error::Invalid(
                        "Use repository, macos or linux for defaults.".into(),
                    ));
                }
                let has = |name: &str| flags.iter().any(|flag| flag == name);
                let name = required(0)?;
                let old = if value(1).is_empty() {
                    None
                } else {
                    Some(value(1).into())
                };
                Change::Profile {
                    old,
                    new: Some(Profile {
                        name: name.into(),
                        default: has("repository"),
                        macos: has("macos"),
                        linux: has("linux"),
                    }),
                }
            }
            Kind::File | Kind::EditFile => {
                let (id, i) = if let Some(file) = &self.mapping {
                    (file.id.clone(), 0)
                } else {
                    (required(0)?.into(), 1)
                };
                let mut profiles = list(value(i + 2));
                if profiles.is_empty() {
                    profiles = dort_core::model::base_profiles();
                }
                Change::File(Box::new(FileChange {
                    before: self.mapping.clone(),
                    mapping: ManagedFile {
                        id,
                        source: required(i)?.into(),
                        target: required(i + 1)?.into(),
                        mode: FileMode::Link,
                        profiles,
                        requires: list(value(i + 3)),
                        permissions: None,
                        overrides: [
                            (Platform::Macos, value(i + 4)),
                            (Platform::Linux, value(i + 5)),
                        ]
                        .into_iter()
                        .filter(|(_, path)| !path.is_empty())
                        .map(|(os, path)| (os, path.into()))
                        .collect(),
                    },
                }))
            }
        })
    }
}
