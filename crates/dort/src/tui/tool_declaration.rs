//! Retained declaration form. The saved manifest is the source of truth.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::declaration::Request;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::Line,
};
use std::collections::BTreeMap;
const LABELS: [&str; 8] = [
    "Tool ID",
    "Command name",
    "Profiles (blank = base)",
    "Required tools",
    "Package manager",
    "Package",
    "Version (blank = default)",
    "npm runtime tool",
];
#[derive(Default)]
pub struct State {
    pub fields: [String; 8],
    pub focus: usize,
    pub failure: Option<String>,
    pub(super) change: Option<dort_core::tools::declaration::Change>,
    advanced: bool,
    provider_drafts: BTreeMap<(usize, String), [String; 4]>,
}
fn list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
impl State {
    pub(super) fn seed_installed(&mut self, seed: crate::services::installed_tools::Seed) {
        use dort_core::tools::inspection::Query;
        let i = 4;
        self.fields[i] = match seed.query {
            Query::BrewFormulae => "brew",
            Query::NpmGlobals => "npm",
        }
        .into();
        self.fields[i + 1] = seed.package;
        if seed.query == Query::NpmGlobals {
            self.fields[i + 2] = seed.version;
        }
        self.fields[i + 3] = seed.runtime.unwrap_or_default();
    }
    pub(super) fn seed_tool(
        &mut self,
        tool: &dort_core::model::Tool,
        change: dort_core::tools::declaration::Change,
    ) {
        self.change = Some(change);
        self.focus = 1;
        self.fields[0] = tool.id.clone();
        self.fields[1] = tool.binary.clone();
        self.fields[2] = tool.profiles.join(", ");
        self.fields[3] = tool.requires.join(", ");
        {
            let install = &tool.install;
            let i = 4;
            self.fields[i] = match install.provider {
                dort_core::model::Provider::Brew => "brew",
                dort_core::model::Provider::Npm => "npm",
            }
            .into();
            self.fields[i + 1] = install.package.clone();
            self.fields[i + 2] = install.version.clone().unwrap_or_default();
            self.fields[i + 3] = install.runtime_tool.clone().unwrap_or_default();
        }
    }
    pub fn controls(&self) -> Vec<usize> {
        let mut controls = if self.change.is_some() {
            vec![1, 2]
        } else {
            vec![0, 1, 2]
        };
        if self.advanced {
            controls.push(3);
        }
        controls.push(4);
        if self.fields[4..8].iter().any(|s| !s.is_empty()) {
            controls.extend([5, 6]);
            if self.fields[4] == "npm" || !self.fields[7].is_empty() {
                controls.push(7);
            }
        }
        controls
    }
    pub(super) fn choose_provider(&mut self, field: usize, value: &'static str) {
        if field != 4 || self.fields[field] == value {
            return;
        }
        let current = std::array::from_fn(|i| self.fields[field + i].clone());
        self.provider_drafts
            .insert((field, self.fields[field].clone()), current);
        let next = self
            .provider_drafts
            .get(&(field, value.into()))
            .cloned()
            .unwrap_or_else(|| [value.into(), String::new(), String::new(), String::new()]);
        for (i, value) in next.into_iter().enumerate() {
            self.fields[field + i] = value;
        }
        self.focus = field;
        self.failure = None;
    }
    pub fn request(&self) -> dort_core::Result<Request> {
        if self.fields[0].trim().is_empty() || self.fields[1].trim().is_empty() {
            return Err(dort_core::Error::Invalid(
                "Enter a tool ID and bare binary name before reviewing.".into(),
            ));
        }
        let f = &self.fields;
        let install = serde_json::json!({
            "provider": f[4],
            "package": f[5],
            "version": (!f[6].is_empty()).then_some(&f[6]),
            "runtime_tool": (!f[7].is_empty()).then_some(&f[7])
        });
        let profiles = if self.fields[2].is_empty() {
            vec!["base".into()]
        } else {
            list(&self.fields[2])
        };
        let mut request = Request::parse(&serde_json::json!({"id":self.fields[0], "binary":self.fields[1], "profiles":profiles, "requires":list(&self.fields[3]), "install":install}).to_string())?;
        request.change = self.change;
        Ok(request)
    }
    pub fn prepare(&mut self) -> Option<Task> {
        match self.request() {
            Ok(r) => {
                self.failure = None;
                Some(Task::ToolDeclaration(r))
            }
            Err(e) => {
                self.focus = [0, 1, 5]
                    .into_iter()
                    .find(|&i| {
                        self.fields[i].is_empty() && (i < 2 || !self.fields[i - 1].is_empty())
                    })
                    .unwrap_or(self.focus.min(7));
                self.failure = Some(super::actions::diagnostic(&e));
                None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        if self.change == Some(dort_core::tools::declaration::Change::Remove) {
            return if key.code == KeyCode::Enter {
                self.prepare()
            } else {
                None
            };
        }
        if key.code == KeyCode::F(4) {
            self.advanced = !self.advanced;
            if !self.advanced && self.focus == 3 {
                self.focus = 2;
            }
            return None;
        }
        let controls = self.controls();
        let position = controls.iter().position(|&i| i == self.focus).unwrap_or(0);
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.focus = controls[(position + 1) % controls.len()],
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = controls[(position + controls.len() - 1) % controls.len()]
            }
            KeyCode::Enter => return self.prepare(),
            _ if self.focus < 8
                && self.focus != 4
                && !(self.focus == 0 && self.change.is_some()) =>
            {
                edit_text(&mut self.fields[self.focus], key);
                self.failure = None;
            }
            _ => (),
        }
        None
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        use ratatui::crossterm::event::KeyModifiers;
        let key = |code| Target::Key(KeyEvent::new(code, KeyModifiers::NONE));
        if self.change == Some(dort_core::tools::declaration::Change::Remove) {
            let mut lines = vec![
                ui::metadata("Remove declaration", &self.fields[0]),
                Line::raw(
                    "Installed packages stay untouched. Review the exact manifest change before confirming.",
                ),
            ];
            if let Some(failure) = &self.failure {
                lines.push(ui::metadata("Problem", failure));
            }
            ui::report(frame, area, "Remove declaration", lines.into(), 0, false);
            hits.present(ui::ActionButton {
                label: "Review removal",
                key: "Enter",
                target: Target::ToolDeclarationControl(8, true),
            });
            hits.present(ui::ActionButton {
                label: "Back",
                key: "Esc",
                target: Target::Back,
            });
            return;
        }
        let controls = self.controls();
        let fields: Vec<_> = controls
            .iter()
            .map(|&i| {
                let value = if i == 4 {
                    match self.fields[i].as_str() {
                        "" => "Choose package manager",
                        "brew" => "Homebrew",
                        "npm" => "npm",
                        other => other,
                    }
                } else {
                    &self.fields[i]
                };
                (LABELS[i], value)
            })
            .collect();
        let choices: Vec<_> = controls
            .iter()
            .enumerate()
            .filter_map(|(position, i)| (*i == 4).then_some(position))
            .collect();
        let editing = self.change.map(|_| {
            format!(
                "Tool {}: ID fixed; packages stay installed.",
                ui::inline(&self.fields[0])
            )
        });
        let feedback = self
            .failure
            .as_deref()
            .map(|text| ui::Feedback {
                text,
                tone: ui::design::Tone::Error,
            })
            .or_else(|| {
                editing.as_deref().map(|text| ui::Feedback {
                    text,
                    tone: ui::design::Tone::Information,
                })
            })
            .or_else(|| {
                (!self.advanced && !self.fields[3].is_empty()).then_some(ui::Feedback {
                    text: "Dependencies are configured and included in review.",
                    tone: ui::design::Tone::Information,
                })
            });
        ui::field_page(
            frame,
            area,
            &fields,
            controls.iter().position(|&i| i == self.focus).unwrap_or(0),
            hits,
            |i| Target::ToolDeclarationControl(controls[i], false),
            ui::FormActions {
                choices: &choices,
                buttons: &[
                    ui::ActionButton {
                        label: "Review declaration",
                        key: "Enter",
                        target: Target::ToolDeclarationControl(8, true),
                    },
                    ui::ActionButton {
                        label: if self.advanced {
                            "Hide dependencies"
                        } else {
                            "Dependencies"
                        },
                        key: "F4",
                        target: key(KeyCode::F(4)),
                    },
                    ui::ActionButton {
                        label: "Back",
                        key: "Esc",
                        target: Target::Back,
                    },
                ],
                feedback,
            },
        );
    }
}
