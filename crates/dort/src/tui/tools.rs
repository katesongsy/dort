//! Tools pages. Package-manager details stay internal.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::inspection_defaults::{self, SettingsEditor};
use dort_core::{
    model::Tool,
    tools::{declaration::Change, inspection::Query},
};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    List,
    Details,
    Declaration,
    Settings,
    Manifest,
    History,
    Installed,
}
#[derive(Default)]
pub struct Settings {
    pub tool: String,
    pub fields: [String; 4],
    pub focus: usize,
    pub editor: Option<SettingsEditor>,
    pub error: Option<String>,
}
impl Settings {
    pub fn count(&self) -> usize {
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.request.query == Query::NpmGlobals)
        {
            4
        } else {
            2
        }
    }
    pub fn prepare(&self, preview_only: bool) -> Option<Task> {
        let mut editor = self.editor.clone()?;
        let request = &mut editor.request;
        request.executable = PathBuf::from(&self.fields[0]);
        request.search_dirs = self.fields[1]
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        if request.query == Query::NpmGlobals {
            request.npm_cli = Some(PathBuf::from(&self.fields[2]));
            request.prefix = Some(PathBuf::from(&self.fields[3]));
        }
        Some(Task::ToolsSettingsSave {
            editor,
            preview_only,
        })
    }
}
#[derive(Default)]
pub struct State {
    parents: Vec<Page>,
    pub declared: super::declared_tools::State,
    pub declared_selected: usize,
    pub declared_details: bool,
    pub declaration_open: bool,
    pub declaration: super::tool_declaration::State,
    declaration_key: Option<String>,
    saved_declarations: BTreeMap<String, super::tool_declaration::State>,
    pub settings_open: bool,
    pub manifest_open: bool,
    manifest_path: String,
    pub(super) manifest_error: Option<String>,
    pub settings: Settings,
    settings_key: Option<(PathBuf, String)>,
    settings_drafts: BTreeMap<(PathBuf, String), Settings>,
    pub install_open: bool,
    pub installation: super::tools_install::State,
    pub installed_open: bool,
    pub installed: super::installed_tools::State,
}
impl State {
    pub(super) fn page(&self) -> Page {
        if self.declaration_open {
            Page::Declaration
        } else if self.settings_open {
            Page::Settings
        } else if self.manifest_open {
            Page::Manifest
        } else if self.installed_open {
            Page::Installed
        } else if self.install_open {
            Page::History
        } else if self.declared_details {
            Page::Details
        } else {
            Page::List
        }
    }
    fn remember(&mut self) {
        if self.parents.len() == 32 {
            self.parents.remove(0);
        }
        self.parents.push(self.page());
    }
    pub fn landing(&self) -> bool {
        !self.declaration_open
            && !self.settings_open
            && !self.manifest_open
            && !self.install_open
            && !self.installed_open
    }
    pub fn at_root(&self) -> bool {
        self.landing()
            && !self.declared_details
            && self.declared.check_preview.is_none()
            && self.declared.install_preview.is_none()
            && !self.declared.result_open
            && self.declared.standalone_result.is_none()
    }
    pub fn workspace_visible(&self) -> bool {
        self.landing()
            && self.declared.check_preview.is_none()
            && self.declared.install_preview.is_none()
            && !self.declared.result_open
            && self.declared.standalone_result.is_none()
            || self.installed_open
                && !self.installed.waiting
                && self.installed.preview.is_none()
                && self.installed.error.is_none()
            || self.install_open
                && self.installation.mode == super::tools_install::Mode::Attempts
                && self.installation.close_preview.is_none()
    }
    pub fn collection_visible(&self) -> bool {
        self.workspace_visible()
            && !(self.landing() && self.declared_details
                || self.installed_open && self.installed.details
                || self.install_open && self.installation.details)
    }
    pub fn text_entry(&self) -> bool {
        self.installed_open && self.installed.searching
            || self.settings_open
            || self.manifest_open
            || self.declaration_open && self.declaration.change != Some(Change::Remove)
            || self.install_open && self.installation.text_entry()
    }
    pub fn inspection_inputs(&self) -> inspection_defaults::Inputs {
        crate::environment::inspection_inputs()
    }
    pub fn leave(&mut self) {
        self.declared.clear_reviews();
        self.declared.result_open = false;
        self.declared.standalone_result = None;
        self.declared_details = false;
        self.declaration_open = false;
        self.settings_open = false;
        self.manifest_open = false;
        self.install_open = false;
        self.installation.clear_approvals();
        self.installed.clear_review();
        self.installed_open = false;
    }
    pub fn leave_section(&mut self) {
        self.leave();
        self.parents.clear();
    }
    pub fn back(&mut self) -> Option<Task> {
        let parent = self.parents.pop().unwrap_or(Page::List);
        self.leave();
        match parent {
            Page::Details => self.declared_details = true,
            Page::Declaration => self.declaration_open = true,
            Page::Settings => self.settings_open = true,
            Page::Manifest => self.manifest_open = true,
            Page::History => self.install_open = true,
            Page::Installed => self.installed_open = true,
            Page::List => (),
        }
        None
    }
    pub fn open_manifest(&mut self) {
        self.remember();
        self.leave();
        self.manifest_open = true;
    }
    pub fn manifest_check(&mut self) -> Task {
        self.manifest_error = None;
        self.declared.check_waiting = true;
        Task::ToolsStandaloneCheckPrepare(crate::services::declared_tools::Input {
            manifest: PathBuf::from(&self.manifest_path),
            profiles: Vec::new(),
            detection: self.inspection_inputs(),
        })
    }
    fn switch_declaration(&mut self, key: Option<String>) {
        self.remember();
        if self.declaration_key != key {
            if self.saved_declarations.len() >= 128 {
                self.saved_declarations.pop_first();
            }
            self.saved_declarations.insert(
                self.declaration_key.take().unwrap_or_default(),
                std::mem::take(&mut self.declaration),
            );
            self.declaration = self
                .saved_declarations
                .remove(key.as_deref().unwrap_or_default())
                .unwrap_or_default();
            self.declaration_key = key;
        }
        self.leave();
        self.declaration_open = true;
    }
    pub fn declare(&mut self) {
        self.switch_declaration(None);
    }
    pub fn change_declaration(&mut self, tool: &Tool, change: Change) {
        let key = format!(
            "{change:?}:{}",
            serde_json::to_string(tool).expect("typed metadata serializes")
        );
        self.switch_declaration(Some(key));
        if self.declaration.change.is_none() {
            self.declaration.seed_tool(tool, change);
        }
    }
    pub fn declaration_finished(&mut self) -> bool {
        let from_installed = self.parents.last() == Some(&Page::Installed);
        if from_installed {
            self.back();
            self.installed.close_details();
        } else {
            self.leave_section();
        }
        self.declaration_saved();
        from_installed
    }
    pub fn declaration_saved(&mut self) {
        self.saved_declarations
            .remove(self.declaration_key.as_deref().unwrap_or_default());
        self.declaration = Default::default();
        self.declaration_key = None;
    }
    pub fn configure(&mut self, manifest: PathBuf, tool: Tool) -> Option<Task> {
        self.remember();
        self.leave();
        let key = (manifest, tool.id.clone());
        if self.settings_key.as_ref() != Some(&key) {
            if self.settings_drafts.len() >= 128 {
                self.settings_drafts.pop_first();
            }
            let next = self.settings_drafts.remove(&key).unwrap_or_default();
            let previous = std::mem::replace(&mut self.settings, next);
            if let Some(previous_key) = self.settings_key.replace(key) {
                self.settings_drafts.insert(previous_key, previous);
            }
        }
        self.settings.tool = tool.id.clone();
        self.settings_open = true;
        Some(Task::ToolsSettingsDetect {
            tool,
            inputs: self.inspection_inputs(),
        })
    }
    pub fn settings_ready(&mut self, editor: SettingsEditor) {
        if !self.settings_open
            || self
                .settings_key
                .as_ref()
                .is_none_or(|(path, id)| path != &editor.manifest || id != &editor.tool)
        {
            return;
        }
        if self
            .settings
            .editor
            .as_ref()
            .is_none_or(|previous| !previous.matches(&editor))
        {
            let request = &editor.request;
            self.settings.focus = 0;
            self.settings.fields = [
                request.executable.display().to_string(),
                request
                    .search_dirs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
                request
                    .npm_cli
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                request
                    .prefix
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ];
        }
        self.settings.editor = Some(editor);
        self.settings.error = None;
    }
    pub fn settings_saved(&mut self) {
        self.settings = Settings::default();
        self.settings_key = None;
        self.declared.invalidate();
        self.back();
    }
    pub fn shrink(&mut self) {
        self.declared.clear_reviews();
        self.installation.clear_approvals();
        self.installed.clear_review();
    }
    pub fn open_installed(&mut self) -> Option<Task> {
        self.remember();
        self.leave();
        self.installed_open = true;
        if self.installed.current.is_none() {
            Some(self.prepare_installed())
        } else {
            None
        }
    }
    pub fn prepare_installed(&mut self) -> Task {
        self.installed.waiting = true;
        Task::ToolsInstalledPrepare(self.inspection_inputs())
    }
    fn declare_installed(&mut self) {
        match self.installed.seed() {
            Ok(seed) => {
                let key = format!(
                    "installed:{:?}:{}:{}:{:?}",
                    seed.query, seed.package, seed.version, seed.runtime
                );
                self.switch_declaration(Some(key));
                if self.declaration.fields.iter().all(String::is_empty) {
                    self.declaration.seed_installed(seed);
                }
            }
            Err(error) => self.installed_error(error.to_string()),
        }
    }
    fn installed_error(&mut self, error: String) {
        self.installed.error = Some(error);
    }
    pub fn open_attempts(&mut self) -> Task {
        self.remember();
        self.leave();
        self.install_open = true;
        self.installation.open_attempts()
    }
    pub fn key(
        &mut self,
        key: KeyEvent,
        dry_run: bool,
        hits: &super::input::Hits,
    ) -> Option<Option<Task>> {
        if self.installed_open {
            if !self.installed.searching
                && self.installed.preview.is_none()
                && self.installed.error.is_none()
            {
                match key.code {
                    KeyCode::Char('a') => {
                        self.declare_installed();
                        return Some(None);
                    }
                    KeyCode::Char('r' | 'R') => return Some(Some(self.prepare_installed())),
                    KeyCode::Esc if !self.installed.details => return Some(self.back()),
                    _ => (),
                }
            }
            return Some(self.installed.key(key, dry_run, hits));
        }
        if self.install_open {
            return match self.installation.key(key, dry_run, hits) {
                Some(super::tools_install::Outcome::Back) => Some(self.back()),
                Some(super::tools_install::Outcome::Task(task)) => Some(Some(*task)),
                None => Some(None),
            };
        }
        if self.declaration_open {
            if key.code == KeyCode::Esc {
                return Some(self.back());
            }
            return Some(self.declaration.key(key));
        }
        if self.manifest_open {
            return Some(match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Enter => Some(self.manifest_check()),
                _ => {
                    self.manifest_error = None;
                    edit_text(&mut self.manifest_path, key);
                    None
                }
            });
        }
        if self.settings_open {
            let n = self.settings.count();
            return Some(match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Tab => {
                    self.settings.focus = (self.settings.focus + 1) % n;
                    None
                }
                KeyCode::BackTab => {
                    self.settings.focus = (self.settings.focus + n - 1) % n;
                    None
                }
                KeyCode::Enter => self.settings.prepare(dry_run),
                _ => {
                    self.settings.error = None;
                    edit_text(
                        &mut self.settings.fields[self.settings.focus.min(n - 1)],
                        key,
                    );
                    None
                }
            });
        }
        None
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        hits.wheel(area, super::input::Wheel::Tools);
        if self.installed_open {
            self.installed.draw(frame, area, dry_run, hits);
            return;
        }
        if self.declaration_open {
            self.declaration.draw(frame, area, hits);
            return;
        }
        if self.install_open {
            self.installation.draw(frame, area, dry_run, hits);
            return;
        }
        if self.manifest_open {
            ui::field_page(
                frame,
                area,
                &[("Absolute manifest path", &self.manifest_path)],
                0,
                hits,
                |_| Target::ToolsControl(0, false),
                ui::FormActions {
                    choices: &[],
                    toggles: &[],
                    buttons: &[
                        ui::ActionButton {
                            label: "Review checks",
                            key: "Enter",
                            target: Target::ToolsControl(1, true),
                        },
                        ui::ActionButton {
                            label: "Back",
                            key: "Esc",
                            target: Target::Back,
                        },
                    ],
                    feedback: Some(ui::Feedback {
                        text: self.manifest_error.as_deref().unwrap_or(
                            "Check only. This does not select a repository or prepare installs.",
                        ),
                        tone: if self.manifest_error.is_some() {
                            ui::design::Tone::Attention
                        } else {
                            ui::design::Tone::Neutral
                        },
                    }),
                },
            );
            return;
        }
        if self.settings_open {
            let labels = [
                if self
                    .settings
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.request.query == Query::NpmGlobals)
                {
                    "Node program"
                } else {
                    "Package manager program"
                },
                "Command search folders (colon separated)",
                "npm CLI script",
                "Global package prefix",
            ];
            let fields = labels
                .iter()
                .zip(&self.settings.fields)
                .take(self.settings.count())
                .map(|(a, b)| (*a, b.as_str()))
                .collect::<Vec<_>>();
            let scope = if self.settings.count() == 4 {
                "tools sharing this Node runtime"
            } else {
                "this tool"
            };
            let note = if dry_run {
                "Dry run validates paths without saving.".to_string()
            } else {
                format!(
                    "Save settings for {} on this host. They apply to {scope} in this repository.",
                    ui::inline(&self.settings.tool)
                )
            };
            ui::field_page(
                frame,
                area,
                &fields,
                self.settings.focus,
                hits,
                |i| Target::ToolsControl(i, false),
                ui::FormActions {
                    choices: &[],
                    toggles: &[],
                    buttons: &[
                        ui::ActionButton {
                            label: if dry_run {
                                "Validate paths"
                            } else {
                                "Save on this host"
                            },
                            key: "Enter",
                            target: Target::ToolsControl(4, true),
                        },
                        ui::ActionButton {
                            label: "Back",
                            key: "Esc",
                            target: Target::Back,
                        },
                    ],
                    feedback: Some(ui::Feedback {
                        text: self.settings.error.as_deref().unwrap_or(&note),
                        tone: if self.settings.error.is_some() {
                            ui::design::Tone::Error
                        } else {
                            ui::design::Tone::Neutral
                        },
                    }),
                },
            );
        }
    }
}
