//! Combined adoption is a page; editing one draft never approves or reviews a write.
use super::{actions::Task, app::App, components as ui, input::Target, shell::AdoptionDraft};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Cell, Clear, Row, Table, TableState},
};
pub struct Editor {
    pub drafts: Vec<AdoptionDraft>,
    paths: Vec<std::path::PathBuf>,
    pub item: usize,
    pub focused: usize,
    pub open: bool,
    pub item_form: bool,
    pub details: bool,
    pub scroll: u16,
    offset: std::cell::Cell<usize>,
    page_rows: std::cell::Cell<u16>,
}
impl Editor {
    pub fn new(
        paths: &std::collections::BTreeSet<std::path::PathBuf>,
        home: &std::path::Path,
    ) -> Self {
        let drafts: Vec<_> = paths
            .iter()
            .map(|p| {
                let mut d = AdoptionDraft::new();
                d.suggest(p, home);
                d
            })
            .collect();
        Self {
            focused: drafts.len() * 3,
            paths: paths.iter().cloned().collect(),
            drafts,
            item: 0,
            open: true,
            item_form: false,
            details: false,
            scroll: 0,
            offset: Default::default(),
            page_rows: std::cell::Cell::new(2),
        }
    }
    pub fn sync(
        &mut self,
        paths: &std::collections::BTreeSet<std::path::PathBuf>,
        home: &std::path::Path,
    ) {
        let mut retained: std::collections::BTreeMap<_, _> = std::mem::take(&mut self.paths)
            .into_iter()
            .zip(std::mem::take(&mut self.drafts))
            .collect();
        self.paths = paths.iter().cloned().collect();
        self.drafts = self
            .paths
            .iter()
            .map(|p| {
                retained.remove(p).unwrap_or_else(|| {
                    let mut d = AdoptionDraft::new();
                    d.suggest(p, home);
                    d
                })
            })
            .collect();
        self.item = self.item.min(self.drafts.len().saturating_sub(1));
        self.focused = self.drafts.len() * 3;
        self.scroll = 0;
        self.open = true;
        self.item_form = false;
        self.details = false;
    }
    pub fn edit_item(&mut self) {
        if !self.drafts.is_empty() {
            self.item_form = true;
            self.details = false;
            self.focused = self.item * 3;
            self.scroll = 0;
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        if key.code == KeyCode::F(2) {
            self.details = !self.details;
            self.scroll = 0;
            return None;
        }
        if self.details {
            match key.code {
                KeyCode::Esc => self.details = false,
                KeyCode::PageDown | KeyCode::Down => {
                    self.scroll = self.scroll.saturating_add(self.page_rows.get().max(1))
                }
                KeyCode::PageUp | KeyCode::Up => {
                    self.scroll = self.scroll.saturating_sub(self.page_rows.get().max(1))
                }
                KeyCode::Home => self.scroll = 0,
                _ => (),
            }
            return None;
        }
        let review = self.drafts.len() * 3;
        if self.item_form {
            let field = self.focused.saturating_sub(self.item * 3).min(2);
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.item_form = false;
                    self.focused = review;
                    self.scroll = 0;
                }
                KeyCode::Tab => self.focused = self.item * 3 + (field + 1) % 3,
                KeyCode::BackTab => self.focused = self.item * 3 + (field + 2) % 3,
                _ => {
                    let d = &mut self.drafts[self.item];
                    super::shell::edit_text(
                        match field {
                            0 => &mut d.target,
                            1 => &mut d.id,
                            _ => &mut d.source,
                        },
                        key,
                    );
                }
            }
            return None;
        }
        self.focused = review; // List selection is distinct from editable field focus.
        match key.code {
            KeyCode::Char('e') => self.edit_item(),
            KeyCode::Esc => self.open = false,
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
                self.item = (self.item + 1).min(self.drafts.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                self.item = self.item.saturating_sub(1)
            }
            KeyCode::PageDown => {
                self.item = (self.item + usize::from(self.page_rows.get()))
                    .min(self.drafts.len().saturating_sub(1))
            }
            KeyCode::PageUp => {
                self.item = self.item.saturating_sub(usize::from(self.page_rows.get()))
            }
            KeyCode::Home => self.item = 0,
            KeyCode::End => self.item = self.drafts.len().saturating_sub(1),
            KeyCode::Enter if !self.drafts.is_empty() => {
                let mappings = self
                    .drafts
                    .iter()
                    .map(|d| {
                        match crate::services::adopt_request(
                            std::path::Path::new(&d.target),
                            &d.id,
                            std::path::Path::new(&d.source),
                        ) {
                            dort_core::planner::Request::Adopt { mapping } => mapping,
                            _ => unreachable!("typed adoption"),
                        }
                    })
                    .collect();
                return Some(Task::Batch(dort_core::batch::Request::Adopt(mappings)));
            }
            _ => (),
        }
        None
    }
    fn metadata(&self) -> Text<'static> {
        let Some(d) = self.drafts.get(self.item) else {
            return Text::raw("No selected draft.");
        };
        Text::from(vec![
            Line::styled("Selected draft metadata", ui::heading()),
            ui::metadata("TARGET: ", &d.target),
            ui::metadata("ID: ", &d.id),
            ui::metadata("SOURCE: ", &d.source),
        ])
    }
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, editor: &Editor) {
    frame.render_widget(Clear, area);
    app.hits.wheel(area, super::input::Wheel::Overlay);
    if editor.details {
        let content = ui::action_bar(
            frame,
            area,
            &app.hits,
            ui::FormActions {
                buttons: &[ui::ActionButton {
                    label: "Back to draft",
                    key: "F2",
                    target: Target::BatchDetails,
                }],
                hint: "F2/Esc: back · PgUp/PgDn: inspect escaped metadata",
            },
        );
        editor.page_rows.set(content.height.max(1));
        ui::text_view(frame, content, editor.metadata(), editor.scroll);
        return;
    }
    if editor.item_form {
        let Some(d) = editor.drafts.get(editor.item) else {
            return;
        };
        let values = [
            ("Target · home-relative", d.target.as_str()),
            ("Mapping ID", d.id.as_str()),
            ("Repository source", d.source.as_str()),
        ];
        ui::field_page(
            frame,
            area,
            &values,
            editor.focused.saturating_sub(editor.item * 3).min(2),
            &app.hits,
            |i| Target::BatchControl(editor.item * 3 + i, false),
            ui::FormActions {
                buttons: &[
                    ui::ActionButton {
                        label: "Done editing",
                        key: "Enter",
                        target: Target::BatchControl(editor.item * 3, true),
                    },
                    ui::ActionButton {
                        label: "Metadata",
                        key: "F2",
                        target: Target::BatchDetails,
                    },
                ],
                hint: "Tab: fields · Enter/Esc: retain and return · no write review",
            },
        );
        return;
    }
    let content = ui::action_bar(
        frame,
        area,
        &app.hits,
        ui::FormActions {
            buttons: &[
                ui::ActionButton {
                    label: "Review all",
                    key: "Enter",
                    target: Target::BatchControl(editor.drafts.len() * 3, true),
                },
                ui::ActionButton {
                    label: "Edit item",
                    key: "e",
                    target: Target::BatchEdit,
                },
                ui::ActionButton {
                    label: "Metadata",
                    key: "F2",
                    target: Target::BatchDetails,
                },
            ],
            hint: "Up/Down: draft · Enter: combined preview · e: edit · Esc: Back",
        },
    );
    let sections = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(if content.height >= 12 { 5 } else { 0 }),
    ])
    .split(content);
    let rows = editor.drafts.iter().map(|d| {
        Row::new(vec![
            Cell::from(ui::inline(&d.target)),
            Cell::from(ui::inline(&d.id)),
            Cell::from(ui::inline(&d.source)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(45),
            Constraint::Percentage(20),
            Constraint::Percentage(35),
        ],
    )
    .header(Row::new(["TARGET", "ID", "SOURCE"]).style(ui::heading()))
    .block(ui::pane("All editable defaults", false))
    .row_highlight_style(ui::selected());
    let mut state = TableState::default()
        .with_selected(Some(editor.item))
        .with_offset(editor.offset.get());
    frame.render_stateful_widget(table, sections[0], &mut state);
    editor.offset.set(state.offset());
    editor
        .page_rows
        .set(sections[0].height.saturating_sub(3).max(1));
    for (line, index) in (state.offset()..editor.drafts.len())
        .take(usize::from(sections[0].height.saturating_sub(3)))
        .enumerate()
    {
        app.hits.click(
            Rect::new(
                sections[0].x,
                sections[0].y + 2 + line as u16,
                sections[0].width,
                1,
            ),
            Target::BatchControl(index * 3, false),
        );
    }
    if sections[1].height > 0 {
        ui::text_view(frame, sections[1], editor.metadata(), 0);
    }
}
