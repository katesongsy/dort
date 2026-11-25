use dort_core::{
    Error, Result,
    local::registry_edit,
    manifest_edit::{self, Change, FileChange, Profile},
    model::{FileMode, ManagedFile},
    platform::Platform,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Profile,
    DeleteProfile,
    File,
    EditFile,
    Startup,
    Forget,
    Reconnect,
    Repair,
}

pub enum Request {
    Manifest(Change),
    Registry(registry_edit::Change),
    OpenRepair(PathBuf, PathBuf),
    Repair(Arc<manifest_edit::RepairDraft>, String),
}

pub enum Preview {
    Manifest(Box<manifest_edit::Preview>),
    Registry(Box<registry_edit::Preview>),
    Repair(Box<manifest_edit::RepairPreview>),
}

pub enum Approval {
    Manifest(Box<manifest_edit::Approval>),
    Registry(Box<registry_edit::Approval>),
    Repair(Box<manifest_edit::Approval>),
}

impl Preview {
    pub fn human(&self) -> String {
        match self {
            Self::Manifest(p) => p.human(),
            Self::Registry(p) => p.human(),
            Self::Repair(p) => p.human(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Manifest(_) => "Save manifest change",
            Self::Registry(_) => "Save repository change",
            Self::Repair(_) => "Save repaired manifest",
        }
    }

    pub fn approve(self) -> Approval {
        match self {
            Self::Manifest(p) => Approval::Manifest(Box::new(p.approve())),
            Self::Registry(p) => Approval::Registry(Box::new(p.approve())),
            Self::Repair(p) => Approval::Repair(Box::new(p.approve())),
        }
    }
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Define profile",
            Self::DeleteProfile => "Delete profile",
            Self::File => "Add file mapping",
            Self::EditFile => "Edit file mapping",
            Self::Startup => "Set startup repository",
            Self::Forget => "Forget repository",
            Self::Reconnect => "Reconnect moved repository",
            Self::Repair => "Repair unregistered manifest",
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
            Self::Startup | Self::Forget => &["Registered repository path"],
            Self::Reconnect => &["Previous registered path", "New repository path"],
            Self::Repair => &["Exact unregistered Git repository path"],
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
    pub fn new(kind: Kind, mapping: Option<ManagedFile>, root: Option<&std::path::Path>) -> Self {
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
        if matches!(
            kind,
            Kind::Startup | Kind::Forget | Kind::Reconnect | Kind::Repair
        ) && let Some(root) = root
        {
            fields[0] = root.display().to_string();
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

    pub fn primary(&self) -> &'static str {
        if self.kind == Kind::Repair {
            "Open repair draft"
        } else {
            "Review changes"
        }
    }

    pub fn change(&self, git: Option<&PathBuf>) -> Result<Request> {
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
        if self.kind == Kind::Repair {
            required(0)?;
            let git = git.ok_or_else(|| {
                Error::Invalid(
                    "Install Git separately and restart Dort before opening this draft.".into(),
                )
            })?;
            return Ok(Request::OpenRepair(
                self.fields[0].clone().into(),
                git.clone(),
            ));
        }
        let registry = match self.kind {
            Kind::Startup => Some(registry_edit::Change::Startup(required(0)?.into())),
            Kind::Forget => Some(registry_edit::Change::Forget(required(0)?.into())),
            Kind::Reconnect => Some(registry_edit::Change::Reconnect {
                previous: required(0)?.into(),
                destination: required(1)?.into(),
            }),
            _ => None,
        };
        if let Some(change) = registry {
            return Ok(Request::Registry(change));
        }
        Ok(Request::Manifest(match self.kind {
            Kind::Startup | Kind::Forget | Kind::Reconnect | Kind::Repair => unreachable!(),
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
        }))
    }
}

pub struct RepairEditor {
    draft: Arc<manifest_edit::RepairDraft>,
    lines: Vec<String>,
    pub focus: usize,
}

impl RepairEditor {
    pub fn new(draft: manifest_edit::RepairDraft) -> Self {
        let lines = draft.suggested().split('\n').map(str::to_owned).collect();
        Self {
            draft: Arc::new(draft),
            lines,
            focus: 0,
        }
    }

    pub fn repository(&self) -> &std::path::Path {
        self.draft.repository()
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn change(&self) -> Request {
        Request::Repair(self.draft.clone(), self.lines.join("\n"))
    }

    pub fn focus_at(&mut self, index: usize) {
        self.focus = index.min(self.lines.len() - 1);
    }

    pub fn insert(&mut self) {
        if self.lines.len() < 256 {
            self.focus += 1;
            self.lines.insert(self.focus, String::new());
        }
    }

    pub fn key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::KeyCode;
        match key.code {
            KeyCode::Up | KeyCode::BackTab => self.focus = self.focus.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.focus_at(self.focus + 1),
            KeyCode::Home => self.focus = 0,
            KeyCode::End => self.focus_at(self.lines.len() - 1),
            KeyCode::PageUp => self.focus = self.focus.saturating_sub(5),
            KeyCode::PageDown => self.focus_at(self.focus + 5),
            KeyCode::Delete if self.lines.len() > 1 => {
                self.lines.remove(self.focus);
                self.focus_at(self.focus);
            }
            KeyCode::F(4) => self.insert(),
            _ => super::shell::edit_text(&mut self.lines[self.focus], key),
        }
    }
}
