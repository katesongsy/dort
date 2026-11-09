//! Retained desired-metadata draft. Review and typed mutation approval are separate.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::declaration::Request;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::Line,
    widgets::Paragraph,
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
                self.failure = Some(super::actions::diagnostic(&e));
                None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % 14,
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + 13) % 14,
            KeyCode::Enter if self.focus == 12 => return self.prepare(),
            KeyCode::Enter => self.focus = (self.focus + 1) % 14,
            _ if self.focus < 12 => {
                edit_text(&mut self.fields[self.focus], key);
                self.failure = None;
            }
            _ => (),
        }
        None
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let header = Rect::new(area.x, area.y, area.width, 2.min(area.height));
        ui::prose(
            frame,
            header,
            "Add tool declaration · selected registered repository/session profiles\nDesired metadata only · NOT installed-binary adoption or a package install",
            ui::heading(),
        );
        let area = Rect::new(
            area.x,
            area.y + header.height,
            area.width,
            area.height.saturating_sub(header.height),
        );
        let budget = usize::from(area.height.saturating_sub(2));
        let height = |i: usize| if i < 12 { 3 } else { 1 };
        let mut start = 0;
        let mut used = 0;
        for i in (0..=self.focus).rev() {
            if used + height(i) > budget {
                break;
            }
            used += height(i);
            start = i;
        }
        if (0..14).map(height).sum::<usize>() <= budget {
            start = 0;
        }
        let mut y = area.y;
        for (i, label) in LABELS
            .iter()
            .chain(["Review declaration · Enter", "Back · Enter"].iter())
            .enumerate()
            .skip(start)
        {
            let h = height(i) as u16;
            if y + h > area.bottom().saturating_sub(2) {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, h);
            hits.click(rect, Target::ToolDeclarationControl(i, i >= 12));
            if i < 12 {
                ui::input(frame, rect, label, &self.fields[i], self.focus == i);
            } else {
                frame.render_widget(
                    Paragraph::new(Line::from(ui::button(label, "", self.focus == i))),
                    rect,
                );
            }
            y += h;
        }
        ui::prose(frame, Rect::new(area.x, area.bottom().saturating_sub(2), area.width, 2.min(area.height)), self.failure.as_deref().unwrap_or("Tab/Shift-Tab: fields · Enter: next/review · Esc: back (draft retained)\nAdd-only; identical = no-op, conflicting ID refused. Review, then Confirm · Enter."), if self.failure.is_some() { ui::warning() } else { ui::muted() });
    }
}
