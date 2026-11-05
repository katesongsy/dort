//! Retained batch-adoption form; preview and approval remain real core objects.
use super::{actions::Task, app::App, components as ui, input::Target, shell::AdoptionDraft};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::Line,
    widgets::Paragraph,
};
pub struct Editor {
    pub drafts: Vec<AdoptionDraft>,
    paths: Vec<std::path::PathBuf>,
    pub item: usize,
    pub focused: usize,
    pub open: bool,
    pub scroll: u16,
}
impl Editor {
    pub fn new(paths: &std::collections::BTreeSet<std::path::PathBuf>) -> Self {
        let drafts = paths
            .iter()
            .map(|p| {
                let mut d = AdoptionDraft::new();
                d.suggest(p);
                d
            })
            .collect();
        Self {
            paths: paths.iter().cloned().collect(),
            drafts,
            item: 0,
            focused: 0,
            open: true,
            scroll: 0,
        }
    }
    pub fn sync(&mut self, paths: &std::collections::BTreeSet<std::path::PathBuf>) {
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
                    d.suggest(p);
                    d
                })
            })
            .collect();
        self.item = self.item.min(self.drafts.len().saturating_sub(1));
        self.open = true;
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Task> {
        match key.code {
            KeyCode::Esc => self.open = false,
            KeyCode::Tab => {
                self.focused = (self.focused + 1) % 6;
                self.scroll = 0;
            }
            KeyCode::BackTab => {
                self.focused = (self.focused + 5) % 6;
                self.scroll = 0;
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Enter if self.focused < 3 => {
                self.focused += 1;
                self.scroll = 0;
            }
            KeyCode::Enter if self.focused == 3 => {
                self.item = self.item.saturating_sub(1);
                self.focused = 0;
                self.scroll = 0;
            }
            KeyCode::Enter if self.focused == 4 => {
                self.item = (self.item + 1).min(self.drafts.len().saturating_sub(1));
                self.focused = 0;
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
                            _ => unreachable!("adopt_request always constructs one typed adoption"),
                        }
                    })
                    .collect();
                return Some(Task::Batch(dort_core::batch::Request::Adopt(mappings)));
            }
            _ if self.focused < 3 => {
                if let Some(d) = self.drafts.get_mut(self.item) {
                    let text = match self.focused {
                        0 => &mut d.target,
                        1 => &mut d.id,
                        _ => &mut d.source,
                    };
                    super::shell::edit_text(text, key);
                }
            }
            _ => (),
        }
        None
    }
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, editor: &Editor) {
    app.hits.clear();
    app.hits.wheel(area, super::input::Wheel::Overlay);
    let block = ui::pane("Adopt marked · Editable batch · Esc retains", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(d) = editor.drafts.get(editor.item) else {
        return;
    };
    if inner.height < 4 {
        return;
    }
    let labels = ["TARGET", "ID", "SOURCE"];
    let field = editor.focused.min(2);
    let value = match field {
        0 => &d.target,
        1 => &d.id,
        _ => &d.source,
    };
    frame.render_widget(
        Paragraph::new(Line::raw(format!(
            "File {}/{} · Tab fields/buttons · PgDn full draft",
            editor.item + 1,
            editor.drafts.len()
        )))
        .style(ui::muted()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let input = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(2),
    );
    if editor.scroll == 0 {
        ui::input(frame, input, labels[field], value, editor.focused < 3);
        app.hits.click(input, Target::BatchControl(field, false));
    } else {
        ui::text_view(
            frame,
            input,
            ratatui::text::Text::from(format!(
                "TARGET: {}\nID: {}\nSOURCE: {}\nEvery marked file is included. Conflicts block the whole batch; no skipping.",
                ui::inline(&d.target),
                ui::inline(&d.id),
                ui::inline(&d.source)
            )),
            editor.scroll.saturating_sub(2),
        );
    }
    let mut x = inner.x;
    for (i, label) in [(3, "Prev"), (4, "Next"), (5, "Review batch")] {
        let button = ui::button(label, "", editor.focused == i);
        let width = (button.width() as u16).min(inner.right().saturating_sub(x));
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, inner.bottom() - 1, width, 1);
        frame.render_widget(Paragraph::new(Line::from(button)), rect);
        app.hits.click(rect, Target::BatchControl(i, true));
        x += width;
    }
}
