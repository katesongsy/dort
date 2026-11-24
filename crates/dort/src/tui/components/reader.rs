//! Report readers share wrapping, browse layout and input defaults.
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use super::pane;
use crate::tui::input::{Hits, Wheel};

pub fn wide_details(area: Rect, header_rows: u16) -> bool {
    area.width >= 110 && area.height.saturating_sub(header_rows) >= 14
}

pub fn detail_panes(area: Rect, wide: bool) -> (Rect, Option<Rect>) {
    if !wide {
        return (area, None);
    }
    let [list, details] =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .spacing(2)
            .areas(area);
    (list, Some(details))
}

pub struct Reader<'a> {
    hits: &'a Hits,
    wheel: Wheel,
}
impl<'a> Reader<'a> {
    pub fn new(hits: &'a Hits) -> Self {
        Self {
            hits,
            wheel: Wheel::Details,
        }
    }

    pub fn with_wheel(mut self, wheel: Wheel) -> Self {
        self.wheel = wheel;
        self
    }

    pub fn report(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        body: Text<'static>,
        offset: u16,
        focused: bool,
    ) {
        self.hits.wheel(area, self.wheel);
        report(frame, area, title, body, offset, focused);
    }

    pub fn wrapped(&self, frame: &mut Frame, area: Rect, body: Text<'static>, offset: u16) {
        self.hits.wheel(area, self.wheel);
        text_view(frame, area, body, offset);
    }

    pub fn pannable(&self, frame: &mut Frame, area: Rect, body: Text<'static>, scroll: (u16, u16)) {
        self.hits.wheel(area, self.wheel);
        frame.render_widget(Paragraph::new(body).scroll(scroll), area);
    }
}

pub struct ScrollKeys {
    line: u16,
    page: u16,
    end: bool,
}
impl ScrollKeys {
    pub const DETAILS: Self = Self::new(1, 2);

    pub const fn new(line: u16, page: u16) -> Self {
        Self {
            line,
            page,
            end: false,
        }
    }

    pub fn with_end(mut self) -> Self {
        self.end = true;
        self
    }

    pub fn apply(self, offset: &mut u16, code: KeyCode) -> bool {
        *offset = match code {
            KeyCode::Down => offset.saturating_add(self.line),
            KeyCode::Up => offset.saturating_sub(self.line),
            KeyCode::PageDown => offset.saturating_add(self.page),
            KeyCode::PageUp => offset.saturating_sub(self.page),
            KeyCode::Home => 0,
            KeyCode::End if self.end => u16::MAX,
            _ => return false,
        };
        true
    }
}

/// Scroll an ASCII-safe report by rendered cells, not unbounded paragraph offsets.
/// Splitting long lines at cells preserves all metadata and avoids silently clipped paths.
pub fn report(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    body: Text<'static>,
    offset: u16,
    focused: bool,
) {
    let block = pane(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    text_view(frame, inner, body, offset);
}
pub fn text_view(frame: &mut Frame, inner: Rect, body: Text<'static>, offset: u16) {
    text_view_at(frame, inner, body, usize::from(offset));
}
pub fn text_view_at(frame: &mut Frame, inner: Rect, body: Text<'static>, offset: usize) {
    let width = usize::from(inner.width).max(1);
    let mut lines = Vec::new();
    for line in body.lines {
        let mut row = Vec::new();
        let mut used = 0;
        let line_style = body.style.patch(line.style);
        for span in line.spans {
            let style = line_style.patch(span.style);
            // Callers supply escaped ASCII text, trusted labels or decoration only.
            for c in span.content.chars() {
                if used == width {
                    lines.push(Line::from(std::mem::take(&mut row)));
                    used = 0;
                }
                row.push(Span::styled(c.to_string(), style));
                used += 1;
            }
        }
        lines.push(Line::from(row));
    }
    let start = offset.min(lines.len().saturating_sub(usize::from(inner.height)));
    frame.render_widget(
        Paragraph::new(Text::from(
            lines
                .into_iter()
                .skip(start)
                .take(usize::from(inner.height))
                .collect::<Vec<_>>(),
        )),
        inner,
    );
}
