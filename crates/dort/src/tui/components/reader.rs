use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use super::{
    design::{PaneFocus, Tone, pane},
    glyphs, inline,
};
use crate::tui::{
    appearance::TextRole,
    input::{Hits, Wheel},
};

pub fn section(title: &str) -> Line<'static> {
    Line::styled(inline(title), TextRole::Heading.style())
}

pub fn field(label: &str, value: &str, tone: Tone) -> Line<'static> {
    field_line(format!("{}: ", inline(label)), value, tone)
}

pub fn metadata(label: &str, value: &str) -> Line<'static> {
    field_line(format!("{:<13} ", inline(label)), value, Tone::Neutral)
}

fn field_line(label: String, value: &str, tone: Tone) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, TextRole::Label.style()),
        Span::styled(inline(value), tone.style()),
    ])
}

fn metadata_lines(
    value: &serde_json::Value,
    label: &str,
    depth: usize,
    lines: &mut Vec<Line<'static>>,
) {
    match value {
        serde_json::Value::Object(values) if depth < 24 => {
            if !label.is_empty() {
                lines.push(section(&format!("{}{label}", "  ".repeat(depth.min(6)))));
            }
            if values.is_empty() {
                lines.push(metadata(label, "None recorded"));
            }
            for (key, value) in values {
                metadata_lines(value, key, depth + 1, lines);
            }
        }
        serde_json::Value::Array(values) if depth < 24 => {
            lines.push(section(&format!(
                "{}{label} ({})",
                "  ".repeat(depth.min(6)),
                values.len()
            )));
            if values.is_empty() {
                lines.push(Line::raw("None recorded"));
            }
            for (index, value) in values.iter().enumerate() {
                metadata_lines(value, &(index + 1).to_string(), depth + 1, lines);
            }
        }
        serde_json::Value::String(value) => lines.push(metadata(label, value)),
        serde_json::Value::Null => lines.push(metadata(label, "Not recorded")),
        value => lines.push(metadata(label, &value.to_string())),
    }
}

/// Display an existing text or JSON report. Use section and field for typed data.
pub fn review_text(body: &str) -> Text<'static> {
    if body.trim_start().starts_with(['{', '['])
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
    {
        let mut lines = Vec::new();
        metadata_lines(&value, "", 0, &mut lines);
        return Text::from(lines);
    }
    let lines = body
        .split_terminator('\n')
        .enumerate()
        .map(|(index, line)| {
            let trimmed = line.trim_start_matches(' ');
            if matches!(
                trimmed.split_once(':').map_or(trimmed, |(label, _)| label),
                "BLOCKED" | "Blocker"
            ) {
                Line::styled(inline(line), TextRole::Blocker.style())
            } else if index != 0
                && let Some(effect) = trimmed.strip_prefix("- ")
            {
                Line::from(vec![
                    Span::raw(format!("{} ", glyphs::PENDING)),
                    Span::raw(inline(effect)),
                ])
            } else if let Some((label, value)) = line.split_once(": ") {
                field(label, value, Tone::Neutral)
            } else if trimmed.ends_with(':') {
                section(line)
            } else if index == 0 {
                Line::styled(inline(line), TextRole::Label.style())
            } else {
                Line::raw(inline(line))
            }
        });
    Text::from(lines.collect::<Vec<_>>())
}

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
        focus: impl Into<PaneFocus>,
    ) {
        self.hits.wheel(area, self.wheel);
        report(frame, area, title, body, offset, focus);
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

/// Scroll a report by wrapped terminal rows.
pub fn report(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    body: Text<'static>,
    offset: u16,
    focus: impl Into<PaneFocus>,
) {
    let block = pane(title, focus);
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
    for line in &body.lines {
        let mut row = Vec::new();
        let mut used = 0;
        for grapheme in line.styled_graphemes(body.style) {
            let span = Span::styled(grapheme.symbol, grapheme.style);
            let cells = span.width();
            if used > 0 && used + cells > width {
                lines.push(Line::from(std::mem::take(&mut row)));
                used = 0;
            }
            used += cells;
            row.push(span);
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
