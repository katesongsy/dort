//! Retained desired-metadata draft. Review and explicit mutation approval are separate.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::declaration::Request;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
};
const LABELS: [&str; 12] = [
    "Tool ID",
    "Bare binary name",
    "Profiles · comma-separated (blank = base)",
    "Requires · comma-separated tool IDs",
    "macOS provider · brew/mise/npm (blank = unsupported)",
    "macOS package",
    "macOS version · blank/provider default, latest, exact, mise prefix",
    "macOS npm runtime tool ID · otherwise blank",
    "Ubuntu provider · apt/mise/npm (blank = unsupported)",
    "Ubuntu package",
    "Ubuntu version · blank/provider default, latest, exact, mise prefix",
    "Ubuntu npm runtime tool ID · otherwise blank",
];
#[derive(Default)]
pub struct State {
    pub fields: [String; 12],
    pub focus: usize,
    pub failure: Option<String>,
}
fn list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
impl State {
    pub fn request(&self) -> dort_core::Result<Request> {
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
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % 12,
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + 11) % 12,
            KeyCode::Enter => return self.prepare(),
            _ if self.focus < 12 => {
                edit_text(&mut self.fields[self.focus], key);
                self.failure = None;
            }
            _ => (),
        }
        None
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let fields: Vec<_> = LABELS
            .iter()
            .zip(&self.fields)
            .map(|(label, value)| (*label, value.as_str()))
            .collect();
        let hint = self
            .failure
            .as_ref()
            .map(|e| ui::inline(e))
            .unwrap_or_else(|| "Tab: fields · Enter: review declaration · Esc: Back".into());
        ui::field_page(
            frame,
            area,
            &fields,
            self.focus,
            hits,
            |i| Target::ToolDeclarationControl(i, false),
            ui::FormActions {
                buttons: &[ui::ActionButton {
                    label: "Review declaration",
                    key: "Enter",
                    target: Target::ToolDeclarationControl(12, true),
                }],
                hint: &hint,
            },
        );
    }
}
