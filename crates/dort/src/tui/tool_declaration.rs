//! Retained desired metadata. Progressive presentation never changes declaration authority.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::declaration::Request;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::{Line, Text},
};
use std::collections::BTreeMap;
const LABELS: [&str; 12] = [
    "Tool ID",
    "Bare binary name",
    "Profiles (blank = base)",
    "Required tool IDs",
    "macOS provider",
    "macOS package",
    "macOS version (blank = provider default)",
    "macOS npm runtime tool ID",
    "Ubuntu provider",
    "Ubuntu package",
    "Ubuntu version (blank = provider default)",
    "Ubuntu npm runtime tool ID",
];
#[derive(Default)]
pub struct State {
    pub fields: [String; 12],
    pub focus: usize,
    pub failure: Option<String>,
    pub details: bool,
    pub(super) from_inspection: bool,
    advanced: bool,
    scroll: u16,
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
    pub(super) fn seed_inspection(
        &mut self,
        platform: dort_core::platform::Platform,
        query: dort_core::tools::inspection::Query,
        package: String,
        version: String,
    ) {
        use dort_core::{platform::Platform, tools::inspection::Query};
        let i = match platform {
            Platform::Macos => 4,
            Platform::Ubuntu => 8,
        };
        self.from_inspection = true;
        self.fields[i] = match query {
            Query::BrewFormulae => "brew",
            Query::DpkgPackages => "apt",
            Query::MiseInstalled => "mise",
            Query::NpmGlobals => "npm",
        }
        .into();
        self.fields[i + 1] = package;
        if matches!(query, Query::MiseInstalled | Query::NpmGlobals) {
            self.fields[i + 2] = version;
        }
    }
    pub fn controls(&self) -> Vec<usize> {
        let mut controls = vec![0, 1, 2];
        if self.advanced {
            controls.push(3);
        }
        for index in [4, 8] {
            controls.push(index);
            if !self.fields[index].is_empty()
                || self.fields[index + 1..index + 4]
                    .iter()
                    .any(|s| !s.is_empty())
            {
                controls.extend([index + 1, index + 2]);
                if self.fields[index] == "npm" || !self.fields[index + 3].is_empty() {
                    controls.push(index + 3);
                }
            }
        }
        controls
    }
    pub(super) fn choose_provider(&mut self, field: usize, value: &'static str) {
        if ![4, 8].contains(&field) || self.fields[field] == value {
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
        let mut install = serde_json::Map::new();
        for (platform, i) in [("macos", 4), ("ubuntu", 8)] {
            let f = &self.fields;
            if f[i].is_empty() {
                if f[i + 1..i + 4].iter().any(|s| !s.is_empty()) {
                    return Err(dort_core::Error::Invalid(
                        "Select a provider or clear all fields for the unsupported platform."
                            .into(),
                    ));
                }
                continue;
            }
            install.insert(platform.into(), serde_json::json!({"provider":f[i], "package":f[i+1], "version":(!f[i+2].is_empty()).then_some(&f[i+2]), "runtime_tool":(!f[i+3].is_empty()).then_some(&f[i+3])}));
        }
        let profiles = if self.fields[2].is_empty() {
            vec!["base".into()]
        } else {
            list(&self.fields[2])
        };
        Request::parse(&serde_json::json!({"id":self.fields[0], "binary":self.fields[1], "profiles":profiles, "requires":list(&self.fields[3]), "install":install}).to_string())
    }
    pub fn prepare(&mut self) -> Option<Task> {
        match self.request() {
            Ok(r) => {
                self.failure = None;
                Some(Task::ToolDeclaration(r))
            }
            Err(e) => {
                self.focus = [0, 1, 5, 9]
                    .into_iter()
                    .find(|&i| {
                        self.fields[i].is_empty() && (i < 2 || !self.fields[i - 1].is_empty())
                    })
                    .unwrap_or(self.focus.min(11));
                self.failure = Some(super::actions::diagnostic(&e));
                None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        if key.code == KeyCode::F(3) {
            self.details = !self.details;
            self.scroll = 0;
            return None;
        }
        if self.details {
            match key.code {
                KeyCode::Esc => self.details = false,
                KeyCode::Down | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
                KeyCode::Up | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
                KeyCode::Home => self.scroll = 0,
                _ => (),
            }
            return None;
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
            _ if self.focus < 12 && ![4, 8].contains(&self.focus) => {
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
        if self.details {
            let lines = LABELS
                .iter()
                .zip(&self.fields)
                .map(|(label, value)| ui::metadata(label, value))
                .collect::<Vec<Line<'static>>>();
            ui::report(
                frame,
                area,
                "Declaration metadata",
                Text::from(lines),
                self.scroll,
                false,
            );
            hits.present(ui::ActionButton {
                label: "Fields",
                key: "Esc",
                target: Target::Back,
            });
            return;
        }
        let controls = self.controls();
        let fields: Vec<_> = controls
            .iter()
            .map(|&i| {
                let value = if [4, 8].contains(&i) {
                    match self.fields[i].as_str() {
                        "" => "Unsupported",
                        "brew" => "Homebrew",
                        "apt" => "APT",
                        "mise" => "mise",
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
            .filter_map(|(position, i)| [4, 8].contains(i).then_some(position))
            .collect();
        let feedback = self
            .failure
            .as_deref()
            .map(|text| ui::Feedback {
                text,
                tone: ui::design::Tone::Error,
            })
            .or_else(|| {
                self.from_inspection.then_some(ui::Feedback {
                    text: "Inspection is a past provider claim, not ownership. Enter a tool ID and command; select the npm runtime if needed.",
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
                        target: Target::ToolDeclarationControl(12, true),
                    },
                    ui::ActionButton {
                        label: "Metadata",
                        key: "F3",
                        target: key(KeyCode::F(3)),
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
