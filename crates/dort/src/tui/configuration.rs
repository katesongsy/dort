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
    CreateProfile,
    EditProfileDefaults,
    RenameProfile,
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
            Self::Manifest(p) => p.generated().map_or_else(
                || p.human(),
                |generated| {
                    format!(
                        "{}\n\n{}",
                        p.human(),
                        crate::services::presentation::generated_details(generated)
                    )
                },
            ),
            Self::Registry(p) => p.human(),
            Self::Repair(p) => p.human(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Manifest(p) if p.generated().is_some() => "Save generated file",
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
            Self::CreateProfile => "Create profile",
            Self::EditProfileDefaults => "Edit profile defaults",
            Self::RenameProfile => "Rename profile",
            Self::DeleteProfile => "Delete profile",
            Self::File => "Add file mapping",
            Self::EditFile => "Edit file mapping",
            Self::Startup => "Set startup repository",
            Self::Forget => "Forget repository",
            Self::Reconnect => "Reconnect moved repository",
            Self::Repair => "Repair unregistered manifest",
        }
    }

    pub fn edits_profile(self) -> bool {
        matches!(
            self,
            Self::EditProfileDefaults | Self::RenameProfile | Self::DeleteProfile
        )
    }

    /// Dropdown-free defaults: each host group is its own on/off row.
    fn default_rows(self) -> std::ops::Range<usize> {
        match self {
            Self::CreateProfile => 1..4,
            Self::EditProfileDefaults => 0..3,
            _ => 0..0,
        }
    }

    fn fields(self) -> &'static [&'static str] {
        match self {
            Self::CreateProfile => &[
                "Profile name",
                "Default for all hosts",
                "Default on macOS",
                "Default on Linux",
            ],
            Self::EditProfileDefaults => &[
                "Default for all hosts",
                "Default on macOS",
                "Default on Linux",
            ],
            Self::RenameProfile => &["New profile name"],
            Self::DeleteProfile => &[],
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
    pub feedback: Option<(String, bool)>,
    pub kind: Kind,
    pub focus: usize,
    fields: Vec<String>,
    /// `repository`, `macos`, `linux` in that order.
    defaults: [bool; 3],
    profile: Option<String>,
    profile_field_label: Option<String>,
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
                file.source
                    .as_ref()
                    .map_or_else(String::new, |p| p.display().to_string()),
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
            feedback: None,
            defaults: [false; 3],
            profile: None,
            profile_field_label: None,
            mapping,
        }
    }

    pub fn for_profile(kind: Kind, profile: Profile) -> Self {
        let mut draft = Self::new(kind, None, None);
        draft.defaults = [profile.default, profile.macos, profile.linux];
        if kind == Kind::RenameProfile {
            draft.profile_field_label = Some(format!("New name for {}", profile.name));
        }
        draft.profile = Some(profile.name);
        draft
    }

    pub fn fields(&self) -> Vec<(&str, &str)> {
        let rows = self.kind.default_rows();
        self.kind
            .fields()
            .iter()
            .enumerate()
            .map(|(i, &label)| {
                if rows.contains(&i) {
                    (
                        label,
                        if self.defaults[i - rows.start] {
                            "On"
                        } else {
                            "Off"
                        },
                    )
                } else {
                    let label = self.profile_field_label.as_deref().unwrap_or(label);
                    (label, self.fields[i].as_str())
                }
            })
            .collect()
    }

    const fn reference_rows(&self) -> &'static [(usize, super::choice::ReferenceField)] {
        use super::choice::ReferenceField::{MappingProfiles, MappingTools};
        match self.kind {
            Kind::File => &[(3, MappingProfiles), (4, MappingTools)],
            Kind::EditFile => &[(2, MappingProfiles), (3, MappingTools)],
            _ => &[],
        }
    }
    pub(super) fn reference_field(&self, index: usize) -> Option<super::choice::ReferenceField> {
        self.reference_rows()
            .iter()
            .find_map(|&(row, field)| (row == index).then_some(field))
    }
    pub(super) fn choice_rows(&self) -> Vec<usize> {
        self.reference_rows().iter().map(|&(row, _)| row).collect()
    }
    fn reference_index(&self, field: super::choice::ReferenceField) -> Option<usize> {
        self.reference_rows()
            .iter()
            .find_map(|&(row, candidate)| (candidate == field).then_some(row))
    }
    pub(super) fn reference_value(&self, field: super::choice::ReferenceField) -> Option<&String> {
        self.fields.get(self.reference_index(field)?)
    }
    pub(super) fn set_reference(&mut self, field: super::choice::ReferenceField, value: String) {
        if let Some(row) = self.reference_index(field)
            && self.fields[row] != value
        {
            self.fields[row] = value;
            self.feedback = None;
        }
    }
    pub fn is_default_row(&self, index: usize) -> bool {
        self.kind.default_rows().contains(&index)
    }

    pub fn toggle_default(&mut self) -> bool {
        if !self.is_default_row(self.focus) {
            return false;
        }
        let slot = self.focus - self.kind.default_rows().start;
        self.defaults[slot] = !self.defaults[slot];
        true
    }

    pub fn toggle_rows(&self) -> Vec<(usize, bool)> {
        let rows = self.kind.default_rows();
        rows.clone()
            .map(|i| (i, self.defaults[i - rows.start]))
            .collect()
    }

    pub fn focus_at(&mut self, index: usize) {
        if !self.fields.is_empty() {
            self.focus = index.min(self.fields.len() - 1);
        }
    }

    pub fn key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::KeyCode;
        if self.fields.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Tab => self.focus = (self.focus + 1) % self.fields.len(),
            KeyCode::BackTab => {
                self.focus = (self.focus + self.fields.len() - 1) % self.fields.len()
            }
            KeyCode::Char(' ') if self.is_default_row(self.focus) => {
                self.toggle_default();
            }
            _ if !self.is_default_row(self.focus) && self.reference_field(self.focus).is_none() => {
                self.feedback = None;
                super::shell::edit_text(&mut self.fields[self.focus], key)
            }
            _ => (),
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
            Kind::Startup | Kind::Forget | Kind::Reconnect | Kind::Repair | Kind::DeleteProfile => {
                unreachable!()
            }
            Kind::RenameProfile => Change::RenameProfile {
                old: self.profile.clone().expect("selected profile"),
                new: required(0)?.into(),
            },
            Kind::CreateProfile | Kind::EditProfileDefaults => {
                let [default, macos, linux] = self.defaults;
                let name = match self.profile.as_deref() {
                    Some(name) => name,
                    None => required(0)?,
                };
                Change::Profile {
                    old: self.profile.clone(),
                    new: Some(Profile {
                        name: name.into(),
                        default,
                        macos,
                        linux,
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
                        source: Some(required(i)?.into()),
                        template: None,
                        target: required(i + 1)?.into(),
                        mode: FileMode::Link,
                        profiles,
                        requires: list(value(i + 3)),
                        inputs: Default::default(),
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
