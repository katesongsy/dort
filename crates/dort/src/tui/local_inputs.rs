use super::{
    app::App,
    components as ui,
    forms::Command,
    input::{Target, Wheel},
};
use crate::services::presentation::local_input_value;
use dort_core::{
    Error, Result,
    generated::{VALUE_LIMIT, local_inputs::Row},
};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    text::Text,
};

pub(super) struct Panel {
    id: String,
    rows: Vec<Row>,
    selected: usize,
    scroll: u16,
    edit: Option<String>,
}

pub(super) struct Change {
    pub id: String,
    pub name: String,
    pub value: Option<String>,
}

impl Panel {
    pub fn new(id: String, rows: Vec<Row>) -> Self {
        Self {
            id,
            rows,
            selected: 0,
            scroll: 0,
            edit: None,
        }
    }

    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }

    pub fn focus_at(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self.scroll = 0;
    }

    pub fn begin_set(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.edit = Some(row.value.clone().unwrap_or_default());
        }
    }

    pub fn back(&mut self) -> bool {
        self.edit.take().is_some()
    }

    pub fn change(&self, clear: bool) -> Option<Change> {
        let row = self.rows.get(self.selected)?;
        let value = if clear {
            if self.editing() || row.value.is_none() {
                return None;
            }
            None
        } else {
            Some(self.edit.clone()?)
        };
        Some(Change {
            id: self.id.clone(),
            name: row.name.clone(),
            value,
        })
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(edit) = &mut self.edit {
            match key.code {
                KeyCode::Backspace => {
                    edit.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => edit.clear(),
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.paste(c.encode_utf8(&mut [0; 4]))?;
                }
                _ => (),
            }
        } else {
            match key.code {
                KeyCode::Down | KeyCode::Tab => self.focus_at(self.selected.saturating_add(1)),
                KeyCode::Up | KeyCode::BackTab => self.focus_at(self.selected.saturating_sub(1)),
                KeyCode::Home => self.focus_at(0),
                KeyCode::End => self.focus_at(usize::MAX),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
                _ => (),
            }
        }
        Ok(())
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        let Some(edit) = &mut self.edit else {
            return Ok(());
        };
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        {
            return Err(Error::Invalid("Use plain text for local inputs.".into()));
        }
        if text.len() > VALUE_LIMIT.saturating_sub(edit.len()) {
            return Err(Error::Invalid("Input exceeds 16 KiB.".into()));
        }
        edit.push_str(text);
        Ok(())
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        let [heading, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        ui::prose(
            frame,
            heading,
            &format!("Local inputs: {}", self.id),
            ui::heading(),
        );
        let Some(row) = self.rows.get(self.selected) else {
            ui::prose(
                frame,
                body,
                "No local inputs declared. Edit the generated file to declare local names.",
                ui::muted(),
            );
            return;
        };
        let feedback = app
            .shell
            .form_feedback
            .as_ref()
            .map(|(_, text)| ui::Feedback {
                text,
                tone: ui::design::Tone::Attention,
            });
        if let Some(edit) = &self.edit {
            let [note, field] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(body);
            ui::prose(
                frame,
                note,
                "Blank sets an empty value. Stored on this machine. No secrets.",
                ui::muted(),
            );
            ui::field_page(
                frame,
                field,
                &[(&format!("local.{}", row.name), edit)],
                0,
                &app.hits,
                |i| Target::FormControl(i, false),
                ui::FormActions {
                    choices: &[],
                    toggles: &[],
                    buttons: &[ui::ActionButton {
                        label: "Review value",
                        key: "Enter",
                        target: Target::FormAction(Command::Submit),
                    }],
                    feedback,
                },
            );
        } else {
            app.presented("Set value", "Enter", Target::FormAction(Command::Submit));
            if row.value.is_some() {
                app.presented("Clear", "F6", Target::FormAction(Command::ClearLocalInput));
            }
            let list_height =
                (self.rows.len().saturating_add(1)).min(usize::from(body.height / 2).max(2)) as u16;
            let [list, details] =
                Layout::vertical([Constraint::Length(list_height), Constraint::Min(0)]).areas(body);
            let capacity = ui::design::pair_capacity(list);
            let start = (self.selected + 1).saturating_sub(capacity);
            let names: Vec<_> = self
                .rows
                .iter()
                .skip(start)
                .take(capacity)
                .map(|row| format!("local.{}", row.name))
                .collect();
            let rows: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let i = start + index;
                    let status = match self.rows[i].value.as_deref() {
                        None => "Unset",
                        Some("") => "Empty",
                        Some(_) => "Set",
                    };
                    ui::design::PairRow {
                        identity: i,
                        values: [name, status],
                    }
                })
                .collect();
            app.hits.wheel(list, Wheel::Choices);
            ui::design::pairs(
                frame,
                list,
                ui::design::PairTable {
                    headers: ["Declared name", "Value"],
                    rows: &rows,
                    selected: self.selected,
                },
                &app.hits,
                |i| Target::FormControl(i, false),
            );
            let value = local_input_value(row.value.as_deref());
            let lines = Text::from(vec![
                ui::reader::section(&format!("local.{} on this machine", row.name)),
                ui::reader::field("Value", &value, ui::design::Tone::Neutral),
            ]);
            ui::reader::Reader::new(&app.hits)
                .with_wheel(Wheel::Overlay)
                .wrapped(frame, details, lines, self.scroll);
        }
    }
}
