//! One combined editable adoption review; defaults need no per-file acceptance.
use super::{actions::Task, app::App, components as ui, input::Target, shell::AdoptionDraft};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Cell, Clear, Paragraph, Row, Table, TableState},
};
pub struct Editor {
    pub drafts: Vec<AdoptionDraft>,
    paths: Vec<std::path::PathBuf>,
    pub item: usize,
    pub focused: usize,
    pub open: bool,
    pub item_form: bool,
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
    }
    pub fn edit_item(&mut self) {
        if !self.drafts.is_empty() {
            self.item_form = true;
            self.focused = self.item * 3;
            self.scroll = 0;
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        let review = self.drafts.len() * 3;
        if self.item_form {
            let field = self.focused.saturating_sub(self.item * 3).min(3);
            match key.code {
                KeyCode::Esc => {
                    self.item_form = false;
                    self.focused = review;
                }
                KeyCode::Enter if field == 3 => {
                    self.item_form = false;
                    self.focused = review;
                }
                KeyCode::Enter | KeyCode::Tab => {
                    self.focused = self.item * 3 + (field + 1) % 4;
                }
                KeyCode::BackTab => self.focused = self.item * 3 + (field + 3) % 4,
                _ if field < 3 => {
                    let d = &mut self.drafts[self.item];
                    let text = match field {
                        0 => &mut d.target,
                        1 => &mut d.id,
                        _ => &mut d.source,
                    };
                    super::shell::edit_text(text, key);
                }
                _ => (),
            }
            return None;
        }
        match key.code {
            KeyCode::Char('e') if self.focused == review => self.edit_item(),
            KeyCode::Char('j' | 'k') if self.focused == review => {
                self.item = if key.code == KeyCode::Char('j') {
                    (self.item + 1).min(self.drafts.len().saturating_sub(1))
                } else {
                    self.item.saturating_sub(1)
                };
            }
            KeyCode::Esc => self.open = false,
            KeyCode::Tab => {
                self.focused = (self.focused + 1) % (review + 1);
                self.scroll = 0;
            }
            KeyCode::BackTab => {
                self.focused = (self.focused + review) % (review + 1);
                self.scroll = 0;
            }
            KeyCode::Down | KeyCode::Up => {
                self.item = if key.code == KeyCode::Down {
                    (self.item + 1).min(self.drafts.len().saturating_sub(1))
                } else {
                    self.item.saturating_sub(1)
                };
                if self.focused < review {
                    self.focused = self.item * 3 + self.focused % 3;
                }
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.page_rows.get().max(1))
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.page_rows.get().max(1))
            }
            KeyCode::Home => self.scroll = 0,
            KeyCode::Enter if self.focused < review => {
                self.focused += 1;
                self.scroll = 0;
            }
            KeyCode::Enter => {
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
            _ if self.focused < review => {
                if let Some(d) = self.drafts.get_mut(self.focused / 3) {
                    let text = match self.focused % 3 {
                        0 => &mut d.target,
                        1 => &mut d.id,
                        _ => &mut d.source,
                    };
                    super::shell::edit_text(text, key);
                }
            }
            _ => (),
        }
        if self.focused < review {
            self.item = self.focused / 3;
        }
        None
    }
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, editor: &Editor) {
    // This is an exclusive review screen, not a transparent overlay on Discover.
    frame.render_widget(Clear, area);
    app.hits.clear();
    app.hits.wheel(area, super::input::Wheel::Overlay);
    if editor.item_form {
        draw_item(frame, area, app, editor);
        return;
    }
    let block = ui::pane("Adopt marked · All editable defaults · Esc retains", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 4 {
        return;
    }
    let review = editor.drafts.len() * 3;
    let editing = editor.focused < review;
    let sections = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(if editing { 3 } else { 0 }),
        Constraint::Length(1),
        Constraint::Length(if inner.height >= 14 { 6 } else { 2 }),
    ])
    .split(inner);
    {
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
        .row_highlight_style(ui::selected());
        let mut state = TableState::default()
            .with_selected(Some(editor.item))
            .with_offset(editor.offset.get());
        frame.render_stateful_widget(table, sections[0], &mut state);
        editor.offset.set(state.offset());
        for (line, index) in (state.offset()..editor.drafts.len())
            .take(sections[0].height.saturating_sub(1) as usize)
            .enumerate()
        {
            let columns = Layout::horizontal([
                Constraint::Percentage(45),
                Constraint::Percentage(20),
                Constraint::Percentage(35),
            ])
            .split(Rect::new(
                sections[0].x,
                sections[0].y + 1 + line as u16,
                sections[0].width,
                1,
            ));
            for (field, rect) in columns.iter().enumerate() {
                app.hits
                    .click(*rect, Target::BatchControl(index * 3 + field, false));
            }
        }
    }
    if editing && let Some(d) = editor.drafts.get(editor.focused / 3) {
        let (label, value) = match editor.focused % 3 {
            0 => ("TARGET", &d.target),
            1 => ("ID", &d.id),
            _ => ("SOURCE", &d.source),
        };
        ui::input(frame, sections[1], label, value, true);
    }
    let edit_width = ui::button("Edit item", "e", false).width() as u16;
    let controls =
        Layout::horizontal([Constraint::Length(edit_width), Constraint::Min(1)]).split(sections[2]);
    frame.render_widget(
        Paragraph::new(Line::from(ui::button("Edit item", "e", false))),
        controls[0],
    );
    app.hits.click(controls[0], Target::BatchEdit);
    let button = ui::button("Review all", "Enter", !editing);
    frame.render_widget(Paragraph::new(Line::from(button)), controls[1]);
    app.hits
        .click(controls[1], Target::BatchControl(review, true));
    editor.page_rows.set(sections[3].height.max(1));
    if let Some(draft) = editor.drafts.get(editor.item) {
        let text = Text::from(vec![
            Line::styled("Selected paths", ui::heading()),
            ui::metadata("TARGET: ", &draft.target),
            ui::metadata("ID: ", &draft.id),
            ui::metadata("SOURCE: ", &draft.source),
        ]);
        ui::text_view(frame, sections[3], text, editor.scroll);
    }
}

fn draw_item(frame: &mut Frame, area: Rect, app: &App, editor: &Editor) {
    let block = ui::pane("Edit item · Esc retains and returns", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(d) = editor.drafts.get(editor.item) else {
        return;
    };
    let field = editor.focused.saturating_sub(editor.item * 3).min(3);
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(format!(
            "Item {} of {} · Enter advances",
            editor.item + 1,
            editor.drafts.len()
        )),
        sections[0],
    );
    let values = [("TARGET", &d.target), ("ID", &d.id), ("SOURCE", &d.source)];
    if sections[1].height >= 9 {
        let fields = Layout::vertical([Constraint::Length(3); 3]).split(sections[1]);
        for (index, (label, value)) in values.iter().enumerate() {
            ui::input(frame, fields[index], label, value, field == index);
            app.hits.click(
                fields[index],
                Target::BatchControl(editor.item * 3 + index, false),
            );
        }
    } else {
        let index = field.min(2);
        let (label, value) = values[index];
        ui::input(
            frame,
            sections[1],
            &format!("{} / 3 · {label}", index + 1),
            value,
            field < 3,
        );
        app.hits.click(
            sections[1],
            Target::BatchControl(editor.item * 3 + index, false),
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(ui::button("Done", "Enter", field == 3))),
        sections[2],
    );
    app.hits
        .click(sections[2], Target::BatchControl(editor.item * 3 + 3, true));
}
