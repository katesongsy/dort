use super::{app::App, components as ui, input::Wheel};
use dort_core::generated::{DriftStatus, Preview};
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
    pub id: String,
    drift_status: Option<DriftStatus>,
    document: Text<'static>,
    pub scroll: ReviewScroll,
}

impl Report {
    pub fn new(preview: &Preview) -> Self {
        let mut lines = vec![
            section(&preview.id),
            Line::raw("Read-only preview. No files written."),
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
        ];
        if let Some(status) = preview.drift_status {
            let tone = match status {
                DriftStatus::Unsafe | DriftStatus::Unverifiable => Tone::Error,
                DriftStatus::UpToDate => Tone::Success,
                DriftStatus::Missing => Tone::Neutral,
                _ => Tone::Attention,
            };
            lines.push(field("Status", status.label(), tone));
            lines.push(Line::raw(status.guidance()));
        }
        lines.extend([
            Line::default(),
            field(
                "Inputs from dort.toml",
                &crate::services::presentation::generated_inputs(preview),
                Tone::Neutral,
            ),
            field(
                "Proposed permissions",
                &preview.proposed_permissions(),
                Tone::Neutral,
            ),
            field(
                "Final newline",
                if preview.rendered.ends_with('\n') {
                    "Yes"
                } else {
                    "No"
                },
                Tone::Neutral,
            ),
        ]);
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
            id: preview.id.clone(),
            drift_status: preview.drift_status,
            document: Text::from(lines),
            scroll: ReviewScroll::default(),
        }
    }

    pub fn can_review_deployment(&self) -> bool {
        self.drift_status == Some(DriftStatus::Missing)
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

use super::template_buffer::TemplateBuffer;
use dort_core::{
    Error, Result,
    generated::{VALUE_LIMIT, input_names},
    manifest_edit::{Change, FileChange},
    model::{FileMode, ManagedFile, base_profiles},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

pub(super) struct Draft {
    before: Option<ManagedFile>,
    id: String,
    target: String,
    pub template: TemplateBuffer,
    values: BTreeMap<String, String>,
    step: Step,
    template_focus: usize,
}

enum Step {
    Template,
    Inputs { names: Vec<String>, focus: usize },
}

impl Draft {
    pub fn new(before: Option<ManagedFile>) -> Self {
        Self {
            id: before.as_ref().map_or_else(String::new, |f| f.id.clone()),
            target: before
                .as_ref()
                .map_or_else(String::new, |f| f.target.display().to_string()),
            template: TemplateBuffer::new(
                before
                    .as_ref()
                    .and_then(|f| f.template.clone())
                    .unwrap_or_default(),
            ),
            values: before
                .as_ref()
                .map_or_else(BTreeMap::new, |f| f.inputs.clone()),
            step: Step::Template,
            template_focus: if before.is_some() { 1 } else { 0 },
            before,
        }
    }

    pub fn id(&self) -> &str {
        self.id.trim()
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
            "Use {{ name }} for inputs. Enter adds a line. Tab moves fields. Never enter secrets."
        }
    }

    pub fn template_field(&self) -> usize {
        if self.before.is_some() { 1 } else { 2 }
    }

    pub fn entering_inputs(&self) -> bool {
        matches!(self.step, Step::Inputs { .. })
    }

    pub fn input_fields(&self) -> Vec<(String, &str)> {
        let Step::Inputs { names, .. } = &self.step else {
            return Vec::new();
        };
        names
            .iter()
            .map(|name| {
                (
                    format!("Input: {name}"),
                    self.values.get(name).map_or("", String::as_str),
                )
            })
            .collect()
    }

    pub fn focus(&self) -> usize {
        match &self.step {
            Step::Template => self.template_focus,
            Step::Inputs { focus, .. } => *focus,
        }
    }

    pub fn focus_at(&mut self, index: usize) {
        let last = self.template_field();
        match &mut self.step {
            Step::Template => self.template_focus = index.min(last),
            Step::Inputs { names, focus } => *focus = index.min(names.len() - 1),
        }
    }

    pub fn back(&mut self) -> bool {
        if self.entering_inputs() {
            self.step = Step::Template;
            true
        } else {
            false
        }
    }

    pub fn submit(&mut self) -> Result<Option<Change>> {
        let template = self.template.text();
        let names = if let Step::Inputs { names, .. } = &self.step {
            names.clone()
        } else {
            if self.id.trim().is_empty() || self.target.trim().is_empty() {
                return Err(Error::Invalid(
                    "Enter a file ID and Home destination.".into(),
                ));
            }
            let names = input_names(template)?
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
            id: self.id().into(),
            mode: FileMode::Template,
            target: Default::default(),
            source: None,
            template: None,
            profiles: base_profiles(),
            requires: vec![],
            overrides: Default::default(),
            inputs: BTreeMap::new(),
            permissions: None,
        });
        mapping.target = self.target.trim().into();
        mapping.template = Some(template.into());
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

    pub fn draw_template(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        use super::input::Target;
        let fields: &[(&str, &str)] = if self.before.is_some() {
            &[("Home destination (~/path)", &self.target)]
        } else {
            &[
                ("File ID", &self.id),
                ("Home destination (~/path)", &self.target),
            ]
        };
        let focus = hits.input_focus_allowed();
        let mut y = area.y;
        let limit = usize::from(area.height.saturating_sub(2));
        for (index, &(label, value)) in fields.iter().enumerate() {
            if index != self.focus() && limit < fields.len() {
                continue;
            }
            if y == area.bottom() {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, 1);
            ui::design::filter(frame, rect, label, value, focus && index == self.focus());
            hits.click(rect, Target::FormControl(index, false));
            y += 1;
        }
        let outer = Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y));
        if outer.is_empty() {
            return;
        }
        let active = focus && self.template_focused();
        let inner = if outer.height >= 3 {
            let block = ui::design::pane("Template", active);
            let inner = block.inner(outer);
            frame.render_widget(block, outer);
            inner
        } else {
            ui::prose(
                frame,
                Rect::new(outer.x, outer.y, outer.width, 1),
                "Template",
                ui::heading(),
            );
            Rect::new(outer.x, outer.y + 1, outer.width, outer.height - 1)
        };
        self.template.draw(frame, inner, active);
        hits.click(outer, Target::FormControl(self.template_field(), false));
    }

    pub fn template_focused(&self) -> bool {
        !self.entering_inputs() && self.template_focus == self.template_field()
    }

    pub fn focus_template(&mut self) {
        self.focus_at(self.template_field());
    }

    fn step_focus(&mut self, backwards: bool) {
        let count = match &self.step {
            Step::Template => self.template_field() + 1,
            Step::Inputs { names, .. } => names.len(),
        };
        self.focus_at((self.focus() + if backwards { count - 1 } else { 1 }) % count);
    }

    fn field(&mut self) -> (&mut String, usize) {
        if let Step::Inputs { names, focus } = &self.step {
            return (
                self.values.entry(names[*focus].clone()).or_default(),
                VALUE_LIMIT,
            );
        }
        if self.before.is_none() && self.template_focus == 0 {
            (&mut self.id, 4096)
        } else {
            (&mut self.target, 4096)
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.step_focus(
                key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT),
            );
            return Ok(());
        }
        if self.template_focused() {
            return self.template.key(key);
        }
        match key.code {
            KeyCode::Down => self.focus_at(self.focus() + 1),
            KeyCode::Up => self.focus_at(self.focus().saturating_sub(1)),
            KeyCode::Home => self.focus_at(0),
            KeyCode::End => self.focus_at(usize::MAX),
            KeyCode::PageDown => self.focus_at(self.focus() + 5),
            KeyCode::PageUp => self.focus_at(self.focus().saturating_sub(5)),
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
        if self.template_focused() {
            return self.template.paste(text);
        }
        let multiline = self.entering_inputs();
        if text
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
        {
            return Err(Error::Invalid("Paste plain text into this field.".into()));
        }
        let (field, limit) = self.field();
        if text.len() > limit.saturating_sub(field.len()) {
            return Err(Error::Invalid(
                if multiline {
                    "Input exceeds 16 KiB."
                } else {
                    "This field exceeds 4 KiB."
                }
                .into(),
            ));
        }
        field.push_str(text);
        Ok(())
    }
}
