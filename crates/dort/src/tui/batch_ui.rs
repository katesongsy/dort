//! Batch adoption pages. Editing a draft does not approve a write.
use super::{
    actions::Task,
    app::App,
    components as ui,
    input::{Target, Wheel},
    shell::AdoptionDraft,
};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Cell, Clear, Row},
};
use ui::collection::{Collection, Header, RowTargets, Selection};
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
            focused: drafts.len() * AdoptionDraft::FIELDS,
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
        self.focused = self.drafts.len() * AdoptionDraft::FIELDS;
        self.scroll = 0;
        self.open = true;
        self.item_form = false;
        self.details = false;
    }
    pub fn edit_item(&mut self) {
        if !self.drafts.is_empty() {
            self.item_form = true;
            self.details = false;
            self.focused = self.item * AdoptionDraft::FIELDS;
            self.scroll = 0;
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
        let review = self.drafts.len() * AdoptionDraft::FIELDS;
        if self.item_form {
            let start = self.item * AdoptionDraft::FIELDS;
            let field = self
                .focused
                .saturating_sub(start)
                .min(AdoptionDraft::FIELDS - 1);
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.item_form = false;
                    self.focused = review;
                    self.scroll = 0;
                }
                KeyCode::Tab => self.focused = start + (field + 1) % AdoptionDraft::FIELDS,
                KeyCode::BackTab => {
                    self.focused =
                        start + (field + AdoptionDraft::FIELDS - 1) % AdoptionDraft::FIELDS
                }
                _ => super::shell::edit_text(self.drafts[self.item].field(field), key),
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
                let mappings = self.drafts.iter().map(AdoptionDraft::mapping).collect();
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
            Line::styled("Selected draft", ui::heading()),
            ui::metadata("TARGET: ", &d.target),
            ui::metadata("ID: ", &d.id),
            ui::metadata("SOURCE: ", &d.source),
            ui::metadata("Profiles", &d.profiles),
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
                choices: &[],
                toggles: &[],
                buttons: &[ui::ActionButton {
                    label: "Back to draft",
                    key: "F3",
                    target: Target::BatchDetails,
                }],
                feedback: None,
            },
        );
        editor.page_rows.set(content.height.max(1));
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .wrapped(frame, content, editor.metadata(), editor.scroll);
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
            ("Profiles (blank uses base)", d.profiles.as_str()),
        ];
        ui::field_page(
            frame,
            area,
            &values,
            editor
                .focused
                .saturating_sub(editor.item * AdoptionDraft::FIELDS)
                .min(AdoptionDraft::FIELDS - 1),
            &app.hits,
            |i| Target::BatchControl(editor.item * AdoptionDraft::FIELDS + i, false),
            ui::FormActions {
                choices: &[],
                toggles: &[],
                buttons: &[
                    ui::ActionButton {
                        label: "Done editing",
                        key: "Enter",
                        target: Target::BatchControl(editor.item * AdoptionDraft::FIELDS, true),
                    },
                    ui::ActionButton {
                        label: "Details",
                        key: "F3",
                        target: Target::BatchDetails,
                    },
                ],
                feedback: None,
            },
        );
        return;
    }
    let content = ui::action_bar(
        frame,
        area,
        &app.hits,
        ui::FormActions {
            choices: &[],
            toggles: &[],
            buttons: &[
                ui::ActionButton {
                    label: "Review all",
                    key: "Enter",
                    target: Target::BatchControl(editor.drafts.len() * AdoptionDraft::FIELDS, true),
                },
                ui::ActionButton {
                    label: "Edit item",
                    key: "e",
                    target: Target::BatchEdit,
                },
                ui::ActionButton {
                    label: "Details",
                    key: "F3",
                    target: Target::BatchDetails,
                },
            ],
            feedback: None,
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
    let collection = Collection::new(
        [
            Constraint::Percentage(45),
            Constraint::Percentage(20),
            Constraint::Percentage(35),
        ],
        Header::Heading(["TARGET", "ID", "SOURCE"]),
    )
    .titled("All editable defaults", false)
    .highlight_only();
    editor
        .page_rows
        .set(collection.capacity(sections[0]).max(1) as u16);
    let rendered = collection.draw(
        frame,
        sections[0],
        rows.enumerate(),
        Selection {
            selected: Some(editor.item),
            offset: editor.offset.get(),
        },
        RowTargets {
            hits: &app.hits,
            wheel: None,
            target: |index| Target::BatchControl(index * AdoptionDraft::FIELDS, false),
        },
    );
    editor.offset.set(rendered.offset);
    if sections[1].height > 0 {
        ui::text_view(frame, sections[1], editor.metadata(), 0);
    }
}
