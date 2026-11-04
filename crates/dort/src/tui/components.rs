//! Shared visual vocabulary. Metadata is escaped before it reaches styled widgets.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn accent() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}
pub fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}
pub fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD)
}
pub fn heading() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
pub fn warning() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}
pub fn danger() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
pub fn state(label: &str) -> Style {
    match label {
        "Blocked" | "Not eligible" | "INTEGRITY!" => danger(),
        "Linked" => Style::default().fg(Color::Green),
        "Needs apply" | "Eligible hint" => warning(),
        _ => Style::default(),
    }
}
pub fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {}{} ", title, if focused { " · Focus" } else { "" }),
            if focused { accent() } else { heading() },
        ))
        .border_style(if focused { accent() } else { muted() })
}
pub fn inline(value: &str) -> String {
    super::views::safe(value).replace('\n', "\\n")
}
pub fn clip(value: &str, width: u16) -> String {
    let value = inline(value);
    if value.len() <= usize::from(width) {
        value
    } else if width < 4 {
        value.chars().take(usize::from(width)).collect()
    } else {
        format!("{}...", &value[..usize::from(width) - 3])
    }
}
pub fn button(label: &str, key: &str, focused: bool) -> Span<'static> {
    Span::styled(
        format!(
            " {}[{}{}] ",
            if focused { "> " } else { "" },
            super::views::safe(label),
            if key.is_empty() {
                String::new()
            } else {
                format!(" · {key}")
            }
        ),
        if focused { selected() } else { accent() },
    )
}
pub fn keys(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![];
    for (key, label) in items {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(format!(" {key} "), selected()));
        spans.push(Span::raw(format!(" {label}")));
    }
    Line::from(spans)
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
    let width = usize::from(inner.width).max(1);
    let mut lines = Vec::new();
    for line in body.lines {
        let mut row = Vec::new();
        let mut used = 0;
        for span in line.spans {
            // Callers supply escaped ASCII text, trusted labels or decoration only.
            for c in span.content.chars() {
                if used == width {
                    lines.push(Line::from(std::mem::take(&mut row)));
                    used = 0;
                }
                row.push(Span::styled(c.to_string(), span.style));
                used += 1;
            }
        }
        lines.push(Line::from(row));
    }
    let start = usize::from(offset).min(lines.len().saturating_sub(usize::from(inner.height)));
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
pub fn metadata(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<13}"), muted()),
        Span::raw(inline(value)),
    ])
}
pub fn input(frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let block = pane(label, focused);
    let inner = block.inner(area);
    let escaped = inline(value);
    let max = usize::from(inner.width.saturating_sub(1));
    let shown = if escaped.len() > max && max > 3 {
        format!("...{}", &escaped[escaped.len() - max + 3..])
    } else {
        escaped.chars().take(max).collect()
    };
    let cursor = inner.x + shown.len() as u16;
    frame.render_widget(Paragraph::new(shown).block(block), area);
    if focused && inner.width > 0 && inner.height > 0 {
        frame.set_cursor_position((cursor, inner.y));
    }
}
pub fn prose(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(super::views::safe(text))
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
}
