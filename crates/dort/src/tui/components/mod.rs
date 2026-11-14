//! Shared visual vocabulary. Metadata is escaped before it reaches styled widgets.
pub mod design;
pub mod glyphs;
pub mod motion;
pub(in crate::tui) mod surface;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Status messages distinguish controlled UI decoration from runtime metadata.
/// Owned strings are escaped by default; explicit chrome composition must escape
/// each dynamic metadata fragment first. No global unescape is performed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusText(String);
impl StatusText {
    pub fn chrome(text: String) -> Self {
        Self(text)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl From<String> for StatusText {
    fn from(text: String) -> Self {
        Self(super::views::safe(&text))
    }
}
impl From<&'static str> for StatusText {
    fn from(text: &'static str) -> Self {
        Self(text.into())
    }
}
impl std::ops::Deref for StatusText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl PartialEq<&str> for StatusText {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
impl std::fmt::Display for StatusText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
pub fn accent() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}
pub fn muted() -> Style {
    Style::default()
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
    design::Tone::Error.style()
}
pub fn success() -> Style {
    design::Tone::Success.style()
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
            format!(" {title} "),
            if focused { accent() } else { heading() },
        ))
        .border_style(if focused { accent() } else { muted() })
}
pub fn inline(value: &str) -> String {
    super::views::safe(value).replace('\n', "\\n")
}
pub fn clip(value: &str, width: u16) -> String {
    chrome_clip(&inline(value), width)
}
/// Trusted decoration plus already-escaped metadata only. Never pass raw paths,
/// provider output or user input here. Clip by terminal cells, not UTF-8 bytes.
pub fn chrome_clip(value: &str, width: u16) -> String {
    let max = usize::from(width);
    if Line::raw(value).width() <= max {
        return value.to_owned();
    }
    let budget = max.saturating_sub(if max >= 4 { 3 } else { 0 });
    let mut out = String::new();
    let mut used = 0;
    for c in value.chars() {
        let w = Span::raw(c.to_string()).width();
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    if max >= 4 {
        out.push_str("...");
    }
    out
}
pub fn button(label: &'static str, key: &'static str, focused: bool) -> Span<'static> {
    Span::styled(
        format!(
            " {}[{}{}] ",
            if focused { "> " } else { "" },
            label,
            if key.is_empty() {
                String::new()
            } else {
                format!(" · {key}")
            }
        ),
        if focused { selected() } else { accent() },
    )
}
pub struct ActionButton {
    pub label: &'static str,
    pub key: &'static str,
    pub target: super::input::Target,
}
pub struct FormActions<'a> {
    pub buttons: &'a [ActionButton],
    pub hint: &'a str,
}
/// Shared page-form layout: only fields have focus. Actions are direct keys or
/// clicks, never additional traversal stops. Every raw field value is escaped.
pub fn field_page(
    frame: &mut Frame,
    area: Rect,
    fields: &[(&str, &str)],
    focus: usize,
    hits: &super::input::Hits,
    field_target: impl Fn(usize) -> super::input::Target,
    controls: FormActions<'_>,
) {
    let fields_area = action_bar(frame, area, hits, controls);
    let count = usize::from(fields_area.height / 3);
    let focus = focus.min(fields.len().saturating_sub(1));
    let start = focus.saturating_sub(count.saturating_sub(1));
    for i in (start..fields.len()).take(count) {
        let rect = Rect::new(
            fields_area.x,
            fields_area.y + (i - start) as u16 * 3,
            fields_area.width,
            3,
        );
        input(
            frame,
            rect,
            fields[i].0,
            fields[i].1,
            i == focus && hits.input_focus_allowed(),
        );
        hits.click(rect, field_target(i));
    }
}
/// Direct-action buttons wrap independently of row/field focus. Returns the
/// remaining content area; hints must contain trusted chrome/escaped metadata.
pub fn action_bar(
    frame: &mut Frame,
    area: Rect,
    hits: &super::input::Hits,
    controls: FormActions<'_>,
) -> Rect {
    let FormActions {
        buttons: actions,
        hint,
    } = controls;
    let mut rows = vec![Vec::<(usize, u16)>::new()];
    let mut used = 0;
    for (i, action) in actions.iter().enumerate() {
        let width = (button(action.label, action.key, false).width() as u16).min(area.width);
        if used > 0 && used + width > area.width {
            rows.push(vec![]);
            used = 0;
        }
        rows.last_mut().expect("action row").push((i, width));
        used += width;
    }
    let action_height = rows.len() as u16;
    let content_height = area.height.saturating_sub(action_height + 1);
    let top = area.y + content_height;
    for (row, buttons) in rows.iter().enumerate() {
        let mut x = area.x;
        for &(i, width) in buttons {
            let rect = Rect::new(x, top + row as u16, width, 1);
            if rect.y < area.bottom() {
                frame.render_widget(
                    Paragraph::new(Line::from(button(actions[i].label, actions[i].key, false))),
                    rect,
                );
                hits.click(rect, actions[i].target);
            }
            x += width;
        }
    }
    chrome_prose(
        frame,
        Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
        &chrome_clip(hint, area.width),
        muted(),
    );
    Rect::new(area.x, area.y, area.width, content_height)
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
/// Visual hierarchy for an operation's existing human report. This presentation
/// never supplies identity, eligibility or approval data to the executor.
pub fn review_text(body: &str) -> Text<'static> {
    let escaped = super::views::safe(body);
    let lines = escaped.lines().enumerate().map(|(index, line)| {
        let trimmed = line.trim_start();
        if index == 0 || trimmed.ends_with(':') {
            Line::styled(line.to_owned(), heading())
        } else if trimmed.starts_with("BLOCKED") || trimmed.starts_with("Blocker") {
            Line::styled(line.to_owned(), danger())
        } else if let Some(effect) = trimmed.strip_prefix("- ") {
            Line::from(vec![
                Span::styled(format!("{} ", glyphs::EXPAND), accent()),
                Span::raw(effect.to_owned()),
            ])
        } else if let Some((label, value)) = line.split_once(": ") {
            Line::from(vec![
                Span::styled(format!("{label}: "), heading()),
                Span::raw(value.to_owned()),
            ])
        } else {
            Line::raw(line.to_owned())
        }
    });
    Text::from(lines.collect::<Vec<_>>())
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
    let block = pane(
        &format!("{label}{}", if focused { " · Focus" } else { "" }),
        focused,
    );
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
/// Trusted UI prose composed only with escaped metadata at dynamic boundaries.
pub fn chrome_prose(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(text).style(style).wrap(Wrap { trim: false }),
        area,
    );
}
pub fn prose(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(super::views::safe(text))
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
}
