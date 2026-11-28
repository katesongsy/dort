use super::{app::App, components as ui, input::Wheel};
use crate::services::presentation::{generated_input_lines, generated_source};
use dort_core::generated::{DriftStatus, Preview, UpdateAction};
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
            field("Source", &generated_source(preview), Tone::Neutral),
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
            lines.push(Line::raw(if status == DriftStatus::UpdateAvailable {
                "Review the new contents and permissions. The deployed file stays in place until approval."
            } else {
                status.guidance()
            }));
        }
        lines.extend([
            Line::default(),
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
        lines.extend([Line::default(), section("Resolved inputs")]);
        lines.extend(generated_input_lines(&preview.resolved_inputs).map(Line::raw));
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

    pub fn review_action(&self) -> Option<UpdateAction> {
        self.drift_status.and_then(UpdateAction::for_drift)
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
    generated::{HostInput, HostInputs, VALUE_LIMIT, input_names_for},
    manifest_edit::{Change, FileChange},
    model::{FileMode, ManagedFile, base_profiles},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Draft {
    before: Option<ManagedFile>,
    id: String,
    target: String,
    pub template: TemplateBuffer,
    content: Content,
    /// Text read from the chosen template file. Finds the inputs it may use.
    source_text: Option<String>,
    host_inputs: HostInputs,
    local_inputs: Vec<String>,
    values: BTreeMap<String, String>,
    step: Step,
    template_focus: usize,
}

/// Where the generated text comes from.
#[derive(Clone, PartialEq, Eq)]
enum Content {
    /// Jinja text in dort.toml.
    Inline,
    /// An existing repository template file, by repository-relative path.
    File(String),
}

impl Content {
    fn of(file: &ManagedFile) -> Self {
        if file.template.is_some() {
            return Self::Inline;
        }
        Self::File(
            file.source
                .as_ref()
                .map_or_else(String::new, |path| path.display().to_string()),
        )
    }
}

enum Step {
    Template,
    Inputs {
        fields: Vec<InputField>,
        focus: usize,
    },
}

enum InputField {
    Value(String),
    Host { key: HostInput, referenced: bool },
    Local { name: String, referenced: bool },
}

impl Draft {
    pub fn new(before: Option<ManagedFile>) -> Self {
        let content = before.as_ref().map_or(Content::Inline, Content::of);
        let rows = if before.is_some() { 1 } else { 2 };
        let template_focus = match &content {
            Content::Inline if before.is_some() => rows + 1,
            _ => 0,
        };
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
            content,
            source_text: None,
            host_inputs: before
                .as_ref()
                .and_then(|f| f.host_inputs)
                .unwrap_or_default(),
            local_inputs: before
                .as_ref()
                .and_then(|f| f.local_inputs.clone())
                .unwrap_or_default(),
            values: before
                .as_ref()
                .map_or_else(BTreeMap::new, |f| f.inputs.clone()),
            step: Step::Template,
            template_focus,
            before,
        }
    }

    pub fn dirty(&self) -> bool {
        let before = self.before.as_ref();
        self.id != before.map_or("", |file| &file.id)
            || self.target != before.and_then(|file| file.target.to_str()).unwrap_or("")
            || self.content != before.map_or(Content::Inline, Content::of)
            || self.template.text()
                != before
                    .and_then(|file| file.template.as_deref())
                    .unwrap_or("")
            || self.host_inputs != before.and_then(|file| file.host_inputs).unwrap_or_default()
            || before.map_or(!self.values.is_empty(), |file| self.values != file.inputs)
            || self.local_inputs_changed(before.and_then(|file| file.local_inputs.as_deref()))
    }

    fn local_inputs_changed(&self, before: Option<&[String]>) -> bool {
        let before = before.unwrap_or(&[]);
        self.local_inputs.len() != before.len()
            || !self.local_inputs.iter().all(|name| before.contains(name))
    }

    pub fn has_local_inputs(&self) -> bool {
        !self.local_inputs.is_empty()
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
            "Declare local names. Set their values after saving. Leave unused inputs unset. No secrets."
        } else if self.content_is_file() {
            "Enter or Space chooses a template file. Dort saves the path and writes dort.toml only."
        } else {
            "Use {{ name }}, {{ host.os }} or {{ local.name }}. In Template, Enter adds a line."
        }
    }

    /// Field rows above the content choice. A new draft also names the file.
    fn field_rows(&self) -> usize {
        if self.before.is_some() { 1 } else { 2 }
    }

    pub fn content_row(&self) -> usize {
        self.field_rows()
    }

    /// Index of the template editor or the read-only template file pane.
    pub fn template_field(&self) -> usize {
        self.content_row() + 1
    }

    pub fn on_content_row(&self) -> bool {
        matches!(self.step, Step::Template) && self.template_focus == self.content_row()
    }

    pub fn content_is_inline(&self) -> bool {
        matches!(self.content, Content::Inline)
    }

    pub fn content_is_file(&self) -> bool {
        matches!(self.content, Content::File(_))
    }

    pub fn content_value(&self) -> String {
        match &self.content {
            Content::Inline => "In dort.toml".into(),
            Content::File(path) if path.is_empty() => "Choose a file".into(),
            Content::File(path) => path.clone(),
        }
    }

    pub fn source_path(&self) -> Option<&str> {
        match &self.content {
            Content::File(path) if !path.is_empty() => Some(path),
            _ => None,
        }
    }

    /// Space and Enter on the content row. True when a template file is still needed.
    pub fn content_action(&mut self, toggle: bool) -> bool {
        if self.entering_inputs() || !self.on_content_row() {
            return false;
        }
        if toggle && self.content_is_file() {
            self.content = Content::Inline;
        } else if self.content_is_inline() {
            self.content = Content::File(String::new());
        }
        self.content_is_file()
    }

    pub fn choose_source(&mut self, path: String) {
        if self.content != Content::File(path.clone()) {
            self.source_text = None;
        }
        self.content = Content::File(path);
    }

    /// Keep the text read for the current choice. False when it was replaced.
    pub fn set_source_text(&mut self, path: &str, text: String) -> bool {
        if self.source_path() != Some(path) {
            return false;
        }
        self.source_text = Some(text);
        true
    }

    /// Text that declares the inputs a render may use.
    fn reference_text(&self) -> &str {
        match (&self.content, &self.source_text) {
            (Content::File(_), Some(text)) => text,
            _ => self.template.text(),
        }
    }

    pub fn entering_inputs(&self) -> bool {
        matches!(self.step, Step::Inputs { .. })
    }

    pub fn input_fields(&self) -> Vec<(String, &str)> {
        let Step::Inputs { fields, .. } = &self.step else {
            return Vec::new();
        };
        fields
            .iter()
            .map(|field| match field {
                InputField::Value(name) => (
                    format!("Input: {name}"),
                    self.values.get(name).map_or("(not set)", String::as_str),
                ),
                InputField::Local { name, referenced } => (
                    format!(
                        "local.{name} ({}, {})",
                        if self.local_inputs.contains(name) {
                            "declared"
                        } else {
                            "undeclared"
                        },
                        if *referenced {
                            "possible reference"
                        } else {
                            "unused"
                        },
                    ),
                    "",
                ),
                InputField::Host { key, referenced } => (
                    format!(
                        "host.{}{}",
                        key.key(),
                        if *referenced { " (referenced)" } else { "" }
                    ),
                    "",
                ),
            })
            .collect()
    }

    pub fn toggle_rows(&self) -> Vec<(usize, bool)> {
        let Step::Inputs { fields, .. } = &self.step else {
            return Vec::new();
        };
        fields
            .iter()
            .enumerate()
            .filter_map(|(i, field)| match field {
                InputField::Host { key, .. } => Some((i, self.host_inputs.contains(*key))),
                InputField::Local { name, .. } => Some((i, self.local_inputs.contains(name))),
                InputField::Value(_) => None,
            })
            .collect()
    }

    pub fn value_action(&self) -> Option<&'static str> {
        let Step::Inputs { fields, focus } = &self.step else {
            return None;
        };
        match &fields[*focus] {
            InputField::Value(name) => Some(if self.values.contains_key(name) {
                "Unset input"
            } else {
                "Set empty input"
            }),
            _ => None,
        }
    }

    pub fn toggle_value(&mut self) {
        if self.value_action().is_none() {
            return;
        }
        if let Step::Inputs { fields, focus } = &self.step
            && let InputField::Value(name) = &fields[*focus]
            && self.values.remove(name).is_none()
        {
            self.values.insert(name.clone(), String::new());
        }
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
            Step::Inputs { fields, focus } => *focus = index.min(fields.len() - 1),
        }
    }

    pub fn toggle_declaration(&mut self) -> bool {
        let Step::Inputs { fields, focus } = &self.step else {
            return false;
        };
        match &fields[*focus] {
            InputField::Host { key, .. } => {
                self.host_inputs = HostInput::ALL
                    .into_iter()
                    .filter(|&k| self.host_inputs.contains(k) != (k == *key))
                    .collect();
            }
            InputField::Local { name, .. } => {
                if let Some(index) = self.local_inputs.iter().position(|key| key == name) {
                    self.local_inputs.remove(index);
                } else {
                    self.local_inputs.push(name.clone());
                }
            }
            InputField::Value(_) => return false,
        }
        true
    }

    pub fn back(&mut self) -> bool {
        if self.entering_inputs() {
            self.step = Step::Template;
            true
        } else {
            false
        }
    }

    fn discover(&self) -> Result<Vec<InputField>> {
        let references = input_names_for(self.id(), self.reference_text())?;
        let names: BTreeSet<_> = references
            .iter()
            .filter_map(|name| match name.strip_prefix("inputs.") {
                Some(name) => Some(name),
                None => Some(name.as_str()).filter(|name| !name.contains('.')),
            })
            .chain(self.values.keys().map(String::as_str))
            .collect();
        let local_names: BTreeSet<_> = references
            .iter()
            .filter_map(|name| name.strip_prefix("local."))
            .chain(self.local_inputs.iter().map(String::as_str))
            .collect();
        Ok(local_names
            .into_iter()
            .map(|name| InputField::Local {
                name: name.to_owned(),
                referenced: references.contains(&format!("local.{name}")),
            })
            .chain(
                names
                    .into_iter()
                    .map(|name| InputField::Value(name.to_owned())),
            )
            .chain(
                HostInput::ALL
                    .into_iter()
                    .filter(|key| {
                        let name = format!("host.{}", key.key());
                        references.contains(&name) || self.host_inputs.contains(*key)
                    })
                    .map(|key| InputField::Host {
                        key,
                        referenced: references.contains(&format!("host.{}", key.key())),
                    }),
            )
            .collect())
    }

    pub fn submit(&mut self) -> Result<Option<Change>> {
        let inputs = if let Step::Inputs { fields, .. } = &self.step {
            fields
                .iter()
                .filter_map(|field| {
                    let InputField::Value(name) = field else {
                        return None;
                    };
                    let value = self.values.get(name).cloned()?;
                    Some((name.clone(), value))
                })
                .collect()
        } else {
            if self.id.trim().is_empty() || self.target.trim().is_empty() {
                return Err(Error::Invalid(
                    "Enter a file ID and Home destination.".into(),
                ));
            }
            if self.content_is_file() && self.source_path().is_none() {
                return Err(Error::Invalid(
                    "Choose a template file for this generated file.".into(),
                ));
            }
            let fields = self.discover()?;
            if !fields.is_empty() {
                self.step = Step::Inputs { fields, focus: 0 };
                return Ok(None);
            }
            BTreeMap::new()
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
            syntax: None,
            host_inputs: None,
            local_inputs: None,
            permissions: None,
        });
        mapping.target = self.target.trim().into();
        mapping.template = self
            .content_is_inline()
            .then(|| self.template.text().into());
        mapping.source = self.source_path().map(Into::into);
        mapping.inputs = inputs;
        mapping.host_inputs = Some(self.host_inputs).filter(|keys| *keys != HostInputs::default());
        if self.local_inputs_changed(mapping.local_inputs.as_deref()) {
            mapping.local_inputs =
                Some(self.local_inputs.clone()).filter(|names| !names.is_empty());
        }
        Ok(Some(Change::File(Box::new(FileChange {
            mapping,
            before: self.before.clone(),
        }))))
    }

    pub fn draw_template(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        use super::input::Target;
        let content = self.content_value();
        let mut fields: Vec<(&str, &str)> = if self.before.is_some() {
            vec![("Home destination (~/path)", self.target.as_str())]
        } else {
            vec![
                ("File ID", self.id.as_str()),
                ("Home destination (~/path)", self.target.as_str()),
            ]
        };
        fields.push(("Template content", content.as_str()));
        let focus = hits.input_focus_allowed();
        let body = area;
        let mut y = area.y;
        let limit = usize::from(body.height.saturating_sub(2));
        for (index, &(label, value)) in fields.iter().enumerate() {
            if index != self.focus() && limit < fields.len() {
                continue;
            }
            if y == body.bottom() {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, 1);
            ui::design::filter(frame, rect, label, value, focus && index == self.focus());
            hits.click(rect, Target::FormControl(index, false));
            y += 1;
        }
        let outer = Rect::new(area.x, y, area.width, body.bottom().saturating_sub(y));
        if outer.is_empty() {
            return;
        }
        if self.content_is_file() {
            self.draw_source(frame, outer);
        } else {
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
        }
        hits.click(outer, Target::FormControl(self.template_field(), false));
    }

    /// The chosen file answers for the text. The draft only saves its path.
    fn draw_source(&self, frame: &mut Frame, area: Rect) {
        let inner = if area.height >= 3 {
            let block = ui::design::pane("Template file", false);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            inner
        } else {
            ui::prose(
                frame,
                Rect::new(area.x, area.y, area.width, 1),
                "Template file",
                ui::heading(),
            );
            Rect::new(area.x, area.y + 1, area.width, area.height - 1)
        };
        let text = match self.source_path() {
            Some(path) => {
                format!("{path}\nRead only. Dort saves the path and writes dort.toml only.")
            }
            None => {
                "Choose a file with Enter. Nothing is written until you review and save.".into()
            }
        };
        ui::prose(frame, inner, &text, ui::muted());
    }

    pub fn template_focused(&self) -> bool {
        self.content_is_inline()
            && !self.entering_inputs()
            && self.template_focus == self.template_field()
    }

    pub fn focus_template(&mut self) {
        self.focus_at(self.template_field());
    }

    fn step_focus(&mut self, backwards: bool) {
        let count = match &self.step {
            Step::Template => self.template_field() + 1,
            Step::Inputs { fields, .. } => fields.len(),
        };
        self.focus_at((self.focus() + if backwards { count - 1 } else { 1 }) % count);
    }

    fn field(&mut self, create: bool) -> Option<&mut String> {
        if let Step::Inputs { fields, focus } = &self.step {
            let InputField::Value(name) = &fields[*focus] else {
                return None;
            };
            return if create {
                Some(self.values.entry(name.clone()).or_default())
            } else {
                self.values.get_mut(name)
            };
        }
        if self.template_focus >= self.template_field() || self.on_content_row() {
            return None;
        }
        match self.template_focus {
            0 if self.before.is_none() => Some(&mut self.id),
            _ => Some(&mut self.target),
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
            KeyCode::Char(' ') if key.modifiers.is_empty() && self.toggle_declaration() => (),
            KeyCode::Down => self.focus_at(self.focus() + 1),
            KeyCode::Up => self.focus_at(self.focus().saturating_sub(1)),
            KeyCode::Home => self.focus_at(0),
            KeyCode::End => self.focus_at(usize::MAX),
            KeyCode::PageDown => self.focus_at(self.focus() + 5),
            KeyCode::PageUp => self.focus_at(self.focus().saturating_sub(5)),
            KeyCode::Backspace => {
                if let Some(value) = self.field(false) {
                    value.pop();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(value) = self.field(false) {
                    value.clear();
                }
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
        let limit = if multiline { VALUE_LIMIT } else { 4096 };
        let used = self.field(false).map_or(0, |value| value.len());
        if text.len() > limit.saturating_sub(used) {
            return Err(Error::Invalid(
                if multiline {
                    "Input exceeds 16 KiB."
                } else {
                    "This field exceeds 4 KiB."
                }
                .into(),
            ));
        }
        if !text.is_empty()
            && let Some(field) = self.field(true)
        {
            field.push_str(text);
        }
        Ok(())
    }
}
