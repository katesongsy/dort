use super::{
    actions::Task,
    app::App,
    components as ui,
    forms::Command,
    input::{Target, Wheel},
    template_buffer::TemplateBuffer,
};
use crate::services::template_files as report;
use dort_core::{
    Error, Result,
    template_files::{Change, Row, Source},
};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    text::Text,
    widgets::Row as TableRow,
};
use std::path::{Path, PathBuf};
use ui::{
    collection::{Collection, Header, RowTargets},
    reader::{Reader, ReviewScroll, ReviewStage},
};

#[derive(Default)]
pub(super) struct Inventory {
    rows: Vec<Row>,
    selected: usize,
    root: Option<PathBuf>,
}

impl Inventory {
    pub fn open(&mut self, root: &Path) {
        if self.root.as_deref() != Some(root) {
            self.replace(Vec::new());
            self.root = Some(root.to_owned());
        }
    }

    pub fn replace(&mut self, rows: Vec<Row>) {
        self.selected = self
            .rows
            .get(self.selected)
            .and_then(|selected| rows.iter().position(|row| row.path == selected.path))
            .unwrap_or(0);
        self.rows = rows;
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
    }

    pub fn key(&mut self, key: KeyEvent, root: PathBuf, page: usize) -> Option<Task> {
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        let index = match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.selected.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => self.selected.saturating_sub(1),
            KeyCode::PageDown => self.selected.saturating_add(page),
            KeyCode::PageUp => self.selected.saturating_sub(page),
            KeyCode::Home => 0,
            KeyCode::End | KeyCode::Char('G') => usize::MAX,
            KeyCode::Enter => {
                return self
                    .rows
                    .get(self.selected)
                    .map(|r| Task::ShowTemplate(root, r.path.clone()));
            }
            _ => return None,
        };
        self.select(index);
        None
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        if !app.registered_repository() {
            ui::prose(
                frame,
                area,
                "Choose a registered repository to open templates.",
                ui::muted(),
            );
            return;
        }
        app.presented("New template file", "s", key_target(KeyCode::Char('s')));
        app.presented("Refresh", "r", key_target(KeyCode::Char('r')));
        if self.rows.is_empty() {
            ui::prose(
                frame,
                area,
                "No template files found. Create a .jinja file under templates/.",
                ui::muted(),
            );
            return;
        }
        app.presented("View source", "Enter", key_target(KeyCode::Enter));
        let collection = Collection::new(
            [
                Constraint::Min(12),
                Constraint::Length(8),
                Constraint::Length(10),
            ],
            Header::Heading(["Template path", "Mappings", "Access"]),
        );
        let window = collection.window(
            area,
            app.inventory.viewport(app.files_page),
            self.rows.len(),
            Some(self.selected),
        );
        let rows = window.range.map(|i| {
            let row = &self.rows[i];
            (
                i,
                TableRow::new([
                    ui::inline(&row.path.display().to_string()),
                    row.mappings.to_string(),
                    if row.editable {
                        "Editable"
                    } else {
                        "Read only"
                    }
                    .into(),
                ]),
            )
        });
        collection.draw(
            frame,
            area,
            rows,
            window.selection,
            RowTargets {
                hits: &app.hits,
                wheel: Some(Wheel::Choices),
                target: Target::Row,
            },
        );
    }
}

fn key_target(code: KeyCode) -> Target {
    Target::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub(super) enum Screen {
    Source {
        source: Source,
        text: Text<'static>,
        scroll: ReviewScroll,
    },
    Draft(Draft),
}

impl Screen {
    pub fn view(source: Source) -> Self {
        let text = Text::raw(report::show(&source));
        Self::Source {
            source,
            text,
            scroll: ReviewScroll::default(),
        }
    }

    pub fn draft(&self) -> Option<&Draft> {
        match self {
            Self::Draft(draft) => Some(draft),
            _ => None,
        }
    }

    pub fn submit(&mut self, root: PathBuf) -> Option<Task> {
        match self {
            Self::Source { source, .. } if source.row().editable => {
                *self = Self::Draft(Draft::new(Change::Edit(source.clone())));
                None
            }
            Self::Draft(draft) => Some(Task::ReviewTemplate(
                root,
                Box::new(match &draft.change {
                    Change::Create(path) => Change::Create(path.clone()),
                    Change::Edit(source) => Change::Edit(source.clone()),
                }),
                draft.buffer.text().to_owned(),
            )),
            _ => None,
        }
    }

    pub fn key(&mut self, key: KeyEvent, focused: bool, hits: &super::input::Hits) -> Result<()> {
        match self {
            Self::Source { scroll, .. } => scroll.key(hits, ReviewStage::TemplateSource, key.code),
            Self::Draft(draft) if focused => draft.key(key)?,
            _ => (),
        }
        Ok(())
    }

    pub fn focus_at(&mut self, index: usize) {
        if let Self::Draft(draft) = self {
            draft.content = index != 0 || matches!(draft.change, Change::Edit(_));
        }
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        match self {
            Self::Draft(draft) if draft.content => draft.buffer.paste(text),
            Self::Draft(draft) => {
                if text.chars().any(char::is_control) {
                    return Err(Error::Invalid(
                        "Use a single line for the template path.".into(),
                    ));
                }
                draft.append_path(text)
            }
            _ => Ok(()),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        match self {
            Self::Source {
                source,
                text,
                scroll,
            } => {
                if source.row().editable {
                    app.presented(
                        "Edit template file",
                        "Enter",
                        Target::FormAction(Command::Submit),
                    );
                }
                Reader::new(&app.hits).with_wheel(Wheel::Overlay).review(
                    frame,
                    area,
                    Some("Template source"),
                    text,
                    ReviewStage::TemplateSource,
                    scroll,
                );
            }
            Self::Draft(draft) => draft.draw(frame, area, app),
        }
    }
}

const NEW_PATH: &str = "templates/";

pub(super) struct Draft {
    change: Change,
    buffer: TemplateBuffer,
    content: bool,
}

impl Draft {
    pub fn create() -> Self {
        Self::new(Change::Create(NEW_PATH.into()))
    }

    fn new(change: Change) -> Self {
        let (text, content) = match &change {
            Change::Create(_) => (String::new(), false),
            Change::Edit(source) => (source.text().to_owned(), true),
        };
        Self {
            change,
            buffer: TemplateBuffer::new(text),
            content,
        }
    }

    pub fn dirty(&self) -> bool {
        match &self.change {
            Change::Create(path) => path.as_os_str() != NEW_PATH || !self.buffer.text().is_empty(),
            Change::Edit(source) => source.text() != self.buffer.text(),
        }
    }

    fn append_path(&mut self, suffix: &str) -> Result<()> {
        if let Change::Create(path) = &mut self.change {
            let mut value = path.to_string_lossy().into_owned();
            if value.len() + suffix.len() > 4096 {
                return Err(Error::Invalid("Template path is too long.".into()));
            }
            value.push_str(suffix);
            *path = value.into();
        }
        Ok(())
    }

    fn key(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            if matches!(self.change, Change::Create(_)) {
                self.content = !self.content;
            }
        } else if self.content {
            self.buffer.key(key)?;
        } else if let Change::Create(path) = &mut self.change {
            let mut value = path.to_string_lossy().into_owned();
            let old_len = value.len();
            super::shell::edit_text(&mut value, key);
            if value.len() > old_len {
                return self.append_path(&value[old_len..]);
            }
            *path = value.into();
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        let controls = ui::FormActions {
            choices: &[],
            toggles: &[],
            buttons: &[ui::ActionButton {
                label: "Review template file",
                key: app.draft_continue_key(),
                target: Target::FormAction(Command::Submit),
            }],
            feedback: app
                .shell
                .form_feedback
                .as_ref()
                .map(|(_, text)| ui::Feedback {
                    text,
                    tone: ui::design::Tone::Attention,
                }),
        };
        let area = ui::action_bar(frame, area, &app.hits, controls);
        let [path_area, content] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let focused = app.form_has_focus();
        match &self.change {
            Change::Create(path) => {
                ui::design::filter(
                    frame,
                    path_area,
                    "Path",
                    &ui::inline(&path.display().to_string()),
                    focused && !self.content,
                );
                app.hits.click(path_area, Target::FormControl(0, false));
            }
            Change::Edit(source) => ui::prose(
                frame,
                path_area,
                &format!(
                    "Edit {}",
                    ui::inline(&source.row().path.display().to_string())
                ),
                ui::heading(),
            ),
        }
        let block = ui::pane("Template", focused && self.content);
        let inner = if content.height >= 3 {
            let inner = block.inner(content);
            frame.render_widget(block, content);
            inner
        } else {
            content
        };
        if !content.is_empty() {
            app.hits.click(content, Target::FormControl(1, false));
            self.buffer.draw(frame, inner, focused && self.content);
        }
    }
}
