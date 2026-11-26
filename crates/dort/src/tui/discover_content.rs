//! Temporary selected-file content, kept out of reports.
use super::{
    actions::Task,
    app::{App, Tab},
};
use dort_core::{discovery::SelectedContent, observation::Entry};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
enum Selection {
    Candidate(Entry, Option<PathBuf>),
    Path(PathBuf),
}
pub struct Read {
    token: u64,
    selection: Selection,
}
pub struct Report {
    token: u64,
    content: Result<SelectedContent, &'static str>,
}
impl Read {
    pub fn perform(self, host: &dort_core::context::HostContext) -> Report {
        let content = match self.selection {
            Selection::Candidate(entry, repository) => {
                dort_core::discovery::selected_content(host, repository.as_deref(), &entry)
            }
            Selection::Path(path) => dort_core::discovery::selected_content_path(host, None, &path),
        };
        Report {
            token: self.token,
            content,
        }
    }
}
pub fn draw(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, state: &State, focused: bool) {
    use ratatui::{
        text::{Line, Span, Text},
        widgets::Paragraph,
    };
    let block = super::components::pane("Selected content", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = match state.followed() {
        Some(target) => {
            let path = target.to_str().unwrap_or("unavailable");
            let line = Line::from(vec![
                Span::styled("Following link -> ", super::components::accent()),
                Span::styled(path.to_owned(), super::components::muted()),
            ]);
            frame.render_widget(
                Paragraph::new(line),
                ratatui::layout::Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
            );
            ratatui::layout::Rect::new(
                inner.x,
                inner.y.saturating_add(1),
                inner.width,
                inner.height.saturating_sub(1),
            )
        }
        None => inner,
    };
    let gutter = usize::from(inner.width.min(7));
    let width = usize::from(inner.width).saturating_sub(gutter);
    state.page.set(inner.height.max(1));
    if width == 0 {
        state.end.set(0);
        return;
    }
    // Escaped ASCII cell chunks: bound allocations to the visible viewport, not
    // one styled span per byte of the entire selected file on every frame.
    let escaped = super::views::safe(state.text());
    let rows = || {
        escaped.lines().enumerate().flat_map(|(number, line)| {
            let chunks = line.as_bytes().chunks(width);
            let empty = line.is_empty().then_some("");
            empty
                .into_iter()
                .chain(chunks.map(|part| std::str::from_utf8(part).expect("escaped ASCII")))
                .enumerate()
                .map(move |(part, text)| (number + 1, part, text))
        })
    };
    let end = rows()
        .count()
        .saturating_sub(usize::from(inner.height))
        .min(u16::MAX as usize) as u16;
    state.end.set(end);
    let start = usize::from(state.scroll.min(end));
    let visible: Vec<_> = rows()
        .skip(start)
        .take(usize::from(inner.height))
        .map(|(number, part, text)| {
            let label = if part == 0 {
                format!("{number:>5} │")
            } else {
                "      │".into()
            };
            let mut spans = vec![Span::styled(label, super::components::muted())];
            if part == 0 && !text.trim_start().starts_with('#') {
                if let Some((key, value)) = text.split_once('=') {
                    spans.push(Span::styled(key.to_owned(), super::components::heading()));
                    spans.push(Span::styled("=", super::components::accent()));
                    spans.push(Span::raw(value.to_owned()));
                } else {
                    spans.push(Span::raw(text.to_owned()));
                }
            } else {
                spans.push(Span::raw(text.to_owned()));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(visible)), inner);
}

#[derive(Default)]
pub struct State {
    token: u64,
    selected: Option<Selection>,
    requested: bool,
    report: Option<Report>,
    pub scroll: u16,
    pub focused: bool,
    pub page: std::cell::Cell<u16>,
    pub end: std::cell::Cell<u16>,
}
impl State {
    pub fn scroll_by(&mut self, backwards: bool, rows: u16) {
        let start = self.scroll.min(self.end.get());
        self.scroll = if backwards {
            start.saturating_sub(rows)
        } else {
            start.saturating_add(rows).min(self.end.get())
        };
    }
    pub fn available(&self) -> bool {
        self.selected.is_some()
    }
    pub fn accept(&mut self, report: Report) {
        if self.selected.is_some() && report.token == self.token {
            self.report = Some(report);
        }
    }
    pub fn followed(&self) -> Option<&std::path::Path> {
        match self.report.as_ref().map(|r| &r.content) {
            Some(Ok(content)) => content.followed(),
            _ => None,
        }
    }
    pub fn text(&self) -> &str {
        match self.report.as_ref().map(|r| &r.content) {
            Some(Ok(content)) => content.text(),
            Some(Err(reason)) => reason,
            None if self.selected.is_some() => "Reading selected file...",
            None => "Select a file or link to preview its content.",
        }
    }
}
/// Invalidate on selection/scope/view changes before accepting a late worker result.
pub fn sync(app: &mut App, width: u16, height: u16) {
    let quiet = app.tab == Tab::Files
        && !app.dry_run
        && width >= 48
        && height >= 14
        && !app.quit
        && !app.exit_requested
        && !app.busy
        && app.error.is_none()
        && !app.show_result
        && app.preview.is_none()
        && app.help.is_none()
        && app.more.is_none()
        && app.choice.is_none()
        && app.shell.overlay.is_none()
        && !app.batch_editor.as_ref().is_some_and(|e| e.open);
    let selected = if quiet
        && app.discovering
        && app.inventory.group.is_none()
        && width >= 48
        && height >= 14
        && !app.quit
        && !app.exit_requested
        && !app.busy
        && app.error.is_none()
        && !app.show_result
        && app.preview.is_none()
        && app.help.is_none()
        && app.more.is_none()
        && app.choice.is_none()
        && app.shell.overlay.is_none()
        && !app.batch_editor.as_ref().is_some_and(|e| e.open)
        && app.inventory.group.is_none()
    {
        app.discovery.as_ref().and_then(|d| {
            d.report
                .candidates
                .get(app.candidate_selected)
                .filter(|c| {
                    matches!(
                        c.entry.kind,
                        dort_core::observation::Kind::Regular
                            | dort_core::observation::Kind::Symlink
                    ) && super::discovery_tree::contains(app, app.candidate_selected)
                })
                .map(|c| Selection::Candidate(c.entry.clone(), d.repository.clone()))
        })
    } else if quiet && !app.discovering && (app.inventory.details || (width >= 110 && height >= 14))
    {
        app.data
            .as_ref()
            .and_then(|data| {
                data.managed.get(app.selected).filter(|row| {
                    !data.file_declarations.iter().any(|file| {
                        file.id == row.id && file.mode == dort_core::model::FileMode::Template
                    })
                })
            })
            .map(|row| Selection::Path(row.target.clone()))
    } else {
        None
    };
    if selected != app.content.selected {
        app.content.token = app.content.token.wrapping_add(1);
        app.content.selected = selected;
        app.content.requested = false;
        app.content.report = None;
        app.content.scroll = 0;
        app.content.focused = false;
        app.content.end.set(0);
    }
    if app.content.available()
        && app.discovering
        && app.inventory.details
        && !app.inventory.search(app.discovering).editing
    {
        app.content.focused = true;
    }
    if app.inventory.search(app.discovering).editing {
        app.content.focused = false;
    }
}
pub fn next(app: &mut App) -> Option<Task> {
    if app.content.requested {
        return None;
    }
    let selection = app.content.selected.clone()?;
    app.content.requested = true;
    Some(Task::DiscoverContent(Read {
        token: app.content.token,
        selection,
    }))
}
