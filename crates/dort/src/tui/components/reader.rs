use std::cell::Cell;

use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::Rect,
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

    pub fn review(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: Option<&str>,
        body: &Text<'static>,
        stage: ReviewStage,
        scroll: &ReviewScroll,
    ) {
        self.hits.wheel(area, self.wheel);
        let inner = if let Some(title) = title {
            let block = pane(title, false);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            inner
        } else {
            area
        };
        let rows = text_view_at(frame, inner, body, scroll.offset.get());
        scroll.offset.set(rows.start);
        self.hits.observe_review(stage, rows);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReviewStage {
    Changes,
    Generated,
    DeclaredCheck,
    InstallMissing,
    InstalledInspect,
    CloseRecord,
}

#[derive(Clone, Copy)]
pub struct VisibleRows {
    pub area: Rect,
    pub start: usize,
    pub end: usize,
}
impl VisibleRows {
    pub fn shows_tail(self) -> bool {
        !self.area.is_empty() && self.start == self.end
    }
}

#[derive(Default)]
pub struct ReviewScroll {
    offset: Cell<usize>,
}
impl ReviewScroll {
    pub fn key(&self, hits: &Hits, stage: ReviewStage, code: KeyCode) {
        let old = self.offset.get();
        let mut offset = old;
        if !ScrollKeys::new(1, 4).with_end().apply_at(&mut offset, code) {
            return;
        }
        if code != KeyCode::End
            && let Some(end) = hits.review_end(stage)
        {
            offset = offset.min(end);
        }
        if offset != old {
            self.offset.set(offset);
            hits.hide_review_tail();
        }
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
        let mut rows = usize::from(*offset);
        let handled = self.apply_at(&mut rows, code);
        *offset = rows.min(usize::from(u16::MAX)) as u16;
        handled
    }
    fn apply_at(self, offset: &mut usize, code: KeyCode) -> bool {
        *offset = match code {
            KeyCode::Down => offset.saturating_add(usize::from(self.line)),
            KeyCode::Up => offset.saturating_sub(usize::from(self.line)),
            KeyCode::PageDown => offset.saturating_add(usize::from(self.page)),
            KeyCode::PageUp => offset.saturating_sub(usize::from(self.page)),
            KeyCode::Home => 0,
            KeyCode::End if self.end => usize::MAX,
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
    text_view_at(frame, inner, &body, usize::from(offset));
}
pub fn text_view_at(
    frame: &mut Frame,
    inner: Rect,
    body: &Text<'static>,
    offset: usize,
) -> VisibleRows {
    let width = usize::from(inner.width).max(1);
    let mut lines = Vec::new();
    for line in &body.lines {
        if let [span] = line.spans.as_slice()
            && span.content.bytes().all(|byte| matches!(byte, b' '..=b'~'))
            && !span.content.is_empty()
        {
            let style = body.style.patch(line.style).patch(span.style);
            lines.extend((0..span.content.len()).step_by(width).map(|start| {
                let end = (start + width).min(span.content.len());
                Line::styled(&span.content[start..end], style)
            }));
            continue;
        }
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
    let end = lines.len().saturating_sub(usize::from(inner.height));
    let start = offset.min(end);
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
    VisibleRows {
        area: inner,
        start,
        end,
    }
}
