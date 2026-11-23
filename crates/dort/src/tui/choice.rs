//! Choice selection for explicit options. Selecting a value runs nothing.
use super::{
    app::{App, Tab},
    components as ui, tools_install,
};
use ratatui::{Frame, crossterm::event::KeyCode};
#[derive(Clone)]
enum Value {
    ClosureProvider(usize),
    DeclarationProvider {
        field: usize,
        provider: &'static str,
    },
}
struct Item {
    label: String,
    scope: String,
    value: Value,
}
pub(super) struct Choice {
    pub title: &'static str,
    pub selected: usize,
    pub details: bool,
    scroll: u16,
    feedback: Option<&'static str>,
    items: Vec<Item>,
}
impl Choice {
    fn new(title: &'static str, items: Vec<Item>, selected: usize) -> Self {
        Self {
            title,
            selected,
            details: false,
            scroll: 0,
            feedback: None,
            items,
        }
    }
    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.items.len().saturating_sub(1));
    }
    pub fn draw(
        &self,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        hits: &super::input::Hits,
        feedback: Option<&'static str>,
    ) {
        let rows: Vec<_> = self
            .items
            .iter()
            .map(|item| (item.label.as_str(), item.scope.as_str()))
            .collect();
        ui::choice::draw(
            frame,
            area,
            hits,
            ui::choice::View {
                rows: &rows,
                selected: self.selected,
                details: self.details,
                scroll: self.scroll,
                feedback: self.feedback.or(feedback),
            },
        );
    }
}
impl App {
    pub(super) fn open_control_choice(&mut self, code: KeyCode) -> bool {
        if self.tab != Tab::Tools || self.busy || self.shell.overlay.is_some() {
            return false;
        }
        let tools = &self.tools_view;
        let opening = matches!(code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right);
        let choice = if tools.install_open
            && tools.installation.mode == tools_install::Mode::CloseInputs
            && code == KeyCode::F(4)
        {
            Some(Choice::new(
                "Choose package manager",
                ["npm", "Homebrew"]
                    .iter()
                    .enumerate()
                    .map(|(index, label)| Item {
                        label: (*label).into(),
                        scope: "Saved inputs; nothing is retried.".into(),
                        value: Value::ClosureProvider(index),
                    })
                    .collect(),
                tools.installation.closure_provider(),
            ))
        } else if tools.declaration_open && tools.declaration.focus == 4 && opening {
            let field = tools.declaration.focus;
            let options = [("Homebrew", "brew"), ("npm", "npm")];
            let selected = options
                .iter()
                .position(|(_, value)| *value == tools.declaration.fields[field])
                .unwrap_or(0);
            Some(Choice::new(
                "Package manager",
                options
                    .into_iter()
                    .map(|(label, provider)| Item {
                        label: label.into(),
                        scope: "Declaration only; no install".into(),
                        value: Value::DeclarationProvider { field, provider },
                    })
                    .collect(),
                selected,
            ))
        } else {
            None
        };
        if let Some(choice) = choice {
            self.choice = Some(choice);
            self.hits.clear();
            true
        } else {
            false
        }
    }
    pub(super) fn choice_key(&mut self, code: KeyCode) {
        let Some(choice) = &mut self.choice else {
            return;
        };
        if code == KeyCode::F(3) {
            choice.details = !choice.details;
            choice.scroll = 0;
            return;
        }
        if choice.details {
            match code {
                KeyCode::Esc => choice.details = false,
                KeyCode::Down | KeyCode::PageDown => {
                    choice.scroll = choice.scroll.saturating_add(2)
                }
                KeyCode::Up | KeyCode::PageUp => choice.scroll = choice.scroll.saturating_sub(2),
                KeyCode::Home => choice.scroll = 0,
                _ => (),
            }
            return;
        }
        match code {
            KeyCode::Esc => self.choice = None,
            KeyCode::Down | KeyCode::Tab => {
                choice.selected = (choice.selected + 1) % choice.items.len().max(1)
            }
            KeyCode::Up | KeyCode::BackTab => {
                choice.selected = (choice.selected + choice.items.len().saturating_sub(1))
                    % choice.items.len().max(1)
            }
            KeyCode::Home => choice.selected = 0,
            KeyCode::End => choice.selected = choice.items.len().saturating_sub(1),
            KeyCode::PageDown => {
                choice.selected = (choice.selected + 3).min(choice.items.len().saturating_sub(1))
            }
            KeyCode::PageUp => choice.selected = choice.selected.saturating_sub(3),
            KeyCode::F(2) | KeyCode::Char('k') => {
                choice.feedback = Some("Choose a value or cancel before opening another menu.")
            }
            KeyCode::Enter => {
                let Some(value) = choice
                    .items
                    .get(choice.selected)
                    .map(|item| item.value.clone())
                else {
                    return;
                };
                self.choice = None;
                self.hits.clear();
                match value {
                    Value::ClosureProvider(index) => {
                        self.tools_view.installation.choose_closure_provider(index)
                    }
                    Value::DeclarationProvider { field, provider }
                        if self.tools_view.declaration_open =>
                    {
                        self.tools_view.declaration.choose_provider(field, provider)
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }
}
