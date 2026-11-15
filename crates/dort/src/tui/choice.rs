//! Choice ownership and application of explicit metadata selections. Never executes a provider.
use super::{
    app::{App, Tab},
    components as ui, tools_install, tools_reconcile,
};
use ratatui::{Frame, crossterm::event::KeyCode};
#[derive(Clone)]
enum Value {
    InspectionProvider(usize),
    ClosureProvider(usize),
    DeclarationProvider {
        field: usize,
        provider: &'static str,
    },
    Binding {
        tool: String,
        snapshot: Option<String>,
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
            hits,
            ui::choice::View {
                title: self.title,
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
        let choice = if tools.configuring && !tools.config_details && tools.focus == 0 && opening {
            Some(Choice::new(
                "Choose inspection provider",
                ["Homebrew", "APT", "mise", "npm"]
                    .iter()
                    .enumerate()
                    .map(|(index, label)| Item {
                        label: (*label).into(),
                        scope: "Explicit query review required".into(),
                        value: Value::InspectionProvider(index),
                    })
                    .collect(),
                tools.query,
            ))
        } else if tools.install_open
            && tools.installation.mode == tools_install::Mode::CloseInputs
            && !tools.installation.draft_metadata()
            && code == KeyCode::F(4)
        {
            Some(Choice::new(
                "Choose historical provider",
                ["mise", "npm", "Homebrew", "APT"]
                    .iter()
                    .enumerate()
                    .map(|(index, label)| Item {
                        label: (*label).into(),
                        scope: "Retained closure inputs; no replay".into(),
                        value: Value::ClosureProvider(index),
                    })
                    .collect(),
                tools.installation.closure_provider(),
            ))
        } else if tools.declaration_open
            && !tools.declaration.details
            && [4, 8].contains(&tools.declaration.focus)
            && opening
        {
            let field = tools.declaration.focus;
            let options = if field == 4 {
                [
                    ("Unsupported", ""),
                    ("Homebrew", "brew"),
                    ("mise", "mise"),
                    ("npm", "npm"),
                ]
            } else {
                [
                    ("Unsupported", ""),
                    ("APT", "apt"),
                    ("mise", "mise"),
                    ("npm", "npm"),
                ]
            };
            let selected = options
                .iter()
                .position(|(_, value)| *value == tools.declaration.fields[field])
                .unwrap_or(0);
            Some(Choice::new(
                if field == 4 {
                    "macOS declaration provider"
                } else {
                    "Ubuntu declaration provider"
                },
                options
                    .into_iter()
                    .map(|(label, provider)| Item {
                        label: label.into(),
                        scope: if provider.is_empty() {
                            "No declaration for this platform"
                        } else {
                            "Desired metadata only; no installation"
                        }
                        .into(),
                        value: Value::DeclarationProvider { field, provider },
                    })
                    .collect(),
                selected,
            ))
        } else if tools.recon_open
            && tools.reconciliation.mode == tools_reconcile::Mode::Bindings
            && opening
        {
            let Some((tool, provider)) = tools.reconciliation.selected_binding_tool() else {
                return false;
            };
            let current = tools.reconciliation.bindings.get(tool);
            let mut items = vec![Item {
                label: "Not inspected".into(),
                scope: "No inspection assigned".into(),
                value: Value::Binding {
                    tool: tool.into(),
                    snapshot: None,
                },
            }];
            let mut selected = 0;
            for snapshot in tools.snapshots.iter().filter(|snapshot| {
                Some(snapshot.report.provider.description.query.provider()) == provider
            }) {
                if current == Some(&snapshot.id) {
                    selected = items.len();
                }
                items.push(Item {
                    label: format!("Snapshot {}", snapshot.id),
                    scope: format!(
                        "{:?} / {}",
                        snapshot.report.provider.description.query,
                        snapshot
                            .report
                            .provider
                            .description
                            .executable
                            .to_str()
                            .unwrap_or("Unavailable path")
                    ),
                    value: Value::Binding {
                        tool: tool.into(),
                        snapshot: Some(snapshot.id.clone()),
                    },
                });
            }
            Some(Choice::new("Choose inspection source", items, selected))
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
                    Value::InspectionProvider(index) if self.tools_view.configuring => {
                        if index != self.tools_view.query {
                            self.tools_view.default_focus = index;
                            self.tools_view.select_advanced();
                        }
                        self.tools_view.focus = 0;
                    }
                    Value::ClosureProvider(index) => {
                        self.tools_view.installation.choose_closure_provider(index)
                    }
                    Value::DeclarationProvider { field, provider }
                        if self.tools_view.declaration_open =>
                    {
                        self.tools_view.declaration.choose_provider(field, provider)
                    }
                    Value::Binding { tool, snapshot }
                        if snapshot.as_ref().is_none_or(|id| {
                            self.tools_view.snapshots.iter().any(|s| &s.id == id)
                        }) =>
                    {
                        self.tools_view
                            .reconciliation
                            .choose_binding(&tool, snapshot.as_deref())
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }
}
