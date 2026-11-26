use super::{app::App, components as ui, input::Wheel};
use dort_core::generated::{DestinationStatus, Preview};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
};
use ui::{
    design::Tone,
    reader::{ReviewScroll, ReviewStage, field, section},
};

pub struct Report {
    document: Text<'static>,
    pub scroll: ReviewScroll,
}

impl Report {
    pub fn new(preview: &Preview) -> Self {
        let tone = match preview.destination_status {
            DestinationStatus::Blocked => Tone::Error,
            DestinationStatus::Existing => Tone::Attention,
            DestinationStatus::Missing => Tone::Neutral,
        };
        let mut lines = vec![
            section(&preview.id),
            Line::raw("Read-only preview. No files written."),
            field(
                "Inputs from dort.toml",
                &crate::services::presentation::generated_inputs(preview),
                Tone::Neutral,
            ),
            Line::default(),
            field(
                "Source",
                &crate::services::presentation::generated_source(preview),
                Tone::Neutral,
            ),
            field(
                "Destination",
                &preview.destination.display().to_string(),
                Tone::Neutral,
            ),
            field(
                "Destination status",
                preview.destination_status.label(),
                tone,
            ),
        ];
        if let Some(mode) = &preview.permissions {
            lines.push(field("Declared permissions", mode, Tone::Neutral));
        }
        lines.push(field(
            "Final newline",
            if preview.rendered.ends_with('\n') {
                "Yes"
            } else {
                "No"
            },
            Tone::Neutral,
        ));
        lines.extend([Line::default(), section("Rendered output")]);
        if preview.rendered.is_empty() {
            lines.push(Line::styled("Empty output", ui::muted()));
        } else {
            lines.extend(
                super::views::safe(&preview.rendered)
                    .split('\n')
                    .map(|line| Line::raw(line.to_owned())),
            );
        }
        lines.push(Line::styled("End of rendered output", ui::muted()));
        Self {
            document: Text::from(lines),
            scroll: ReviewScroll::default(),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .review(
                frame,
                area,
                None,
                &self.document,
                ReviewStage::Generated,
                &self.scroll,
            );
    }
}

use dort_core::{
    Error, Result,
    generated::{RECIPE_LIMIT, VALUE_LIMIT, input_names},
    manifest_edit::{Change, FileChange},
    model::{FileMode, ManagedFile, base_profiles},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

pub(super) struct Draft {
    before: Option<ManagedFile>,
    id: String,
    target: String,
    lines: Vec<String>,
    values: BTreeMap<String, String>,
    step: Step,
    recipe_focus: usize,
}

enum Step {
    Recipe,
    Inputs { names: Vec<String>, focus: usize },
}

impl Draft {
    pub fn new(before: Option<ManagedFile>) -> Self {
        Self {
            id: before.as_ref().map_or_else(String::new, |f| f.id.clone()),
            target: before
                .as_ref()
                .map_or_else(String::new, |f| f.target.display().to_string()),
            lines: before
                .as_ref()
                .and_then(|f| f.recipe.as_deref())
                .unwrap_or_default()
                .split('\n')
                .map(str::to_owned)
                .collect(),
            values: before
                .as_ref()
                .map_or_else(BTreeMap::new, |f| f.inputs.clone()),
            before,
            step: Step::Recipe,
            recipe_focus: 0,
        }
    }

    pub fn title(&self) -> String {
        if self.before.is_some() {
            format!("Edit generated file: {}", self.id)
        } else {
            "New generated file".into()
        }
    }

    pub fn instructions(&self) -> &'static str {
        if self.entering_inputs() {
            "Enter literal values. Blanks and spaces are kept. Never enter secrets."
        } else {
            "Use {{ name }} for inputs. Add lines with F4. Del removes a line. Never enter secrets."
        }
    }

    fn first_line(&self) -> usize {
        if self.before.is_some() { 1 } else { 2 }
    }

    pub fn entering_inputs(&self) -> bool {
        matches!(self.step, Step::Inputs { .. })
    }

    pub fn fields(&self) -> Vec<(String, &str)> {
        match &self.step {
            Step::Recipe => {
                let mut fields = Vec::new();
                if self.before.is_none() {
                    fields.push(("File ID".into(), self.id.as_str()));
                }
                fields.push(("Home destination (~/path)".into(), self.target.as_str()));
                fields.extend(
                    self.lines
                        .iter()
                        .enumerate()
                        .map(|(i, line)| (format!("Recipe line {}", i + 1), line.as_str())),
                );
                fields
            }
            Step::Inputs { names, .. } => names
                .iter()
                .map(|name| {
                    (
                        format!("Input: {name}"),
                        self.values.get(name).map_or("", String::as_str),
                    )
                })
                .collect(),
        }
    }

    pub fn focus(&self) -> usize {
        match &self.step {
            Step::Recipe => self.recipe_focus,
            Step::Inputs { focus, .. } => *focus,
        }
    }

    pub fn focus_at(&mut self, index: usize) {
        let last = self.first_line() + self.lines.len() - 1;
        match &mut self.step {
            Step::Recipe => self.recipe_focus = index.min(last),
            Step::Inputs { names, focus } => *focus = index.min(names.len() - 1),
        }
    }

    pub fn back(&mut self) -> bool {
        if self.entering_inputs() {
            self.step = Step::Recipe;
            true
        } else {
            false
        }
    }

    pub fn submit(&mut self) -> Result<Option<Change>> {
        let recipe = self.lines.join("\n");
        let names = if let Step::Inputs { names, .. } = &self.step {
            names.clone()
        } else {
            if self.id.trim().is_empty() || self.target.trim().is_empty() {
                return Err(Error::Invalid(
                    "Enter a file ID and Home destination.".into(),
                ));
            }
            let names = input_names(&recipe)?
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !names.is_empty() {
                self.step = Step::Inputs { names, focus: 0 };
                return Ok(None);
            }
            names
        };
        let mut mapping = self.before.clone().unwrap_or_else(|| ManagedFile {
            id: self.id.trim().into(),
            mode: FileMode::Template,
            target: Default::default(),
            source: None,
            recipe: None,
            profiles: base_profiles(),
            requires: vec![],
            overrides: Default::default(),
            inputs: BTreeMap::new(),
            permissions: None,
        });
        mapping.target = self.target.trim().into();
        mapping.recipe = Some(recipe);
        mapping.inputs = names
            .into_iter()
            .map(|name| {
                let value = self.values.get(&name).cloned().unwrap_or_default();
                (name, value)
            })
            .collect();
        Ok(Some(Change::File(Box::new(FileChange {
            mapping,
            before: self.before.clone(),
        }))))
    }

    fn recipe_size(&self) -> usize {
        self.lines.iter().map(String::len).sum::<usize>() + self.lines.len() - 1
    }

    fn recipe_line(&self) -> Option<usize> {
        if self.entering_inputs() {
            None
        } else {
            self.recipe_focus.checked_sub(self.first_line())
        }
    }

    pub fn insert_line(&mut self) -> Result<()> {
        if self.entering_inputs() {
            return Ok(());
        }
        if self.recipe_size() >= RECIPE_LIMIT {
            return Err(Error::Invalid("Template recipe exceeds 64 KiB.".into()));
        }
        let index = self.recipe_line().map_or(0, |line| line + 1);
        self.lines.insert(index, String::new());
        self.recipe_focus = self.first_line() + index;
        Ok(())
    }

    fn field(&mut self) -> (&mut String, usize) {
        if let Some(line) = self.recipe_line() {
            let limit = RECIPE_LIMIT.saturating_sub(self.recipe_size() - self.lines[line].len());
            return (&mut self.lines[line], limit);
        }
        if let Step::Inputs { names, focus } = &self.step {
            return (
                self.values.entry(names[*focus].clone()).or_default(),
                VALUE_LIMIT,
            );
        }
        if self.before.is_none() && self.recipe_focus == 0 {
            (&mut self.id, 4096)
        } else {
            (&mut self.target, 4096)
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.focus_at(self.focus() + 1),
            KeyCode::BackTab | KeyCode::Up => self.focus_at(self.focus().saturating_sub(1)),
            KeyCode::Home => self.focus_at(0),
            KeyCode::End => self.focus_at(usize::MAX),
            KeyCode::PageDown => self.focus_at(self.focus() + 5),
            KeyCode::PageUp => self.focus_at(self.focus().saturating_sub(5)),
            KeyCode::Delete if self.recipe_line().is_some() => {
                let line = self.recipe_line().unwrap();
                if self.lines.len() == 1 {
                    self.lines[0].clear();
                } else {
                    self.lines.remove(line);
                    self.focus_at(self.recipe_focus);
                }
            }
            KeyCode::Backspace => {
                self.field().0.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.field().0.clear()
            }
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
        Ok(())
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        let line = self.recipe_line();
        let multiline = line.is_some() || self.entering_inputs();
        if text
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
        {
            return Err(Error::Invalid("Paste plain text into this field.".into()));
        }
        let (field, limit) = self.field();
        if text.len() > limit.saturating_sub(field.len()) {
            return Err(Error::Invalid(
                match (line, multiline) {
                    (Some(_), _) => "Template recipe exceeds 64 KiB.",
                    (None, true) => "Input exceeds 16 KiB.",
                    (None, false) => "This field exceeds 4 KiB.",
                }
                .into(),
            ));
        }
        if let Some(line) = line {
            let mut parts = text.split('\n');
            field.push_str(parts.next().unwrap_or_default());
            let added = parts.map(str::to_owned).collect::<Vec<_>>();
            self.recipe_focus += added.len();
            self.lines.splice(line + 1..line + 1, added);
        } else {
            field.push_str(text);
        }
        Ok(())
    }
}
