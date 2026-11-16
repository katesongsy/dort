//! Shared visual vocabulary. Metadata is escaped before it reaches styled widgets.
pub mod choice;
pub mod chrome;
pub mod dashboard;
pub mod design;
pub mod glyphs;
pub mod help;
pub mod menu;
pub mod motion;
pub mod navigation;
pub(in crate::tui) mod surface;

pub fn short_field(label: &str, value: &str, width: u16) -> String {
    let escaped = inline(&format!("{value:?}"));
    let available = usize::from(width.saturating_sub(6))
        .saturating_sub(label.len())
        .max(4);
    let shown = if escaped.len() > available {
        format!("...{}", &escaped[escaped.len() - available + 3..])
    } else {
        escaped
    };
    format!("{label}: {shown}")
}

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
    design::selection().add_modifier(Modifier::BOLD)
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
        "INTEGRITY!" => danger(),
        "Blocked" | "Not eligible" => warning(),
        "Linked" => Style::default().fg(Color::Green),
        "Needs apply" | "Eligible hint" => warning(),
        _ => Style::default(),
    }
}
pub fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
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
#[derive(Clone, Copy)]
pub struct ActionButton {
    pub label: &'static str,
    pub key: &'static str,
    pub target: super::input::Target,
}
pub struct Feedback<'a> {
    pub text: &'a str,
    pub tone: design::Tone,
}
pub struct FormActions<'a> {
    pub choices: &'a [usize],
    pub buttons: &'a [ActionButton],
    pub feedback: Option<Feedback<'a>>,
}
/// Fields own input focus; the application shell owns the single action strip.
pub fn field_page(
    frame: &mut Frame,
    area: Rect,
    fields: &[(&str, &str)],
    focus: usize,
    hits: &super::input::Hits,
    field_target: impl Fn(usize) -> super::input::Target,
    controls: FormActions<'_>,
) {
    let choices = controls.choices;
    let fields_area = action_bar(frame, area, hits, controls);
    let count = usize::from(fields_area.height / 2).max(1);
    let focus = focus.min(fields.len().saturating_sub(1));
    let start = focus.saturating_sub(count.saturating_sub(1));
    for i in (start..fields.len()).take(count) {
        let y = fields_area.y + (i - start) as u16 * 2;
        if y + 1 >= fields_area.bottom() {
            break;
        }
        let active = i == focus && hits.input_focus_allowed();
        frame.render_widget(
            Paragraph::new(fields[i].0).style(if active { heading() } else { muted() }),
            Rect::new(fields_area.x, y, fields_area.width, 1),
        );
        let rect = Rect::new(fields_area.x, y + 1, fields_area.width, 1);
        if choices.contains(&i) {
            design::choice(frame, rect, fields[i].1, active);
        } else {
            design::filter(frame, rect, "", fields[i].1, active);
        }
        hits.click(
            Rect::new(fields_area.x, y, fields_area.width, 2),
            field_target(i),
        );
    }
}
/// Register foreground actions, without allocating another toolbar or hint row.
pub fn action_bar(
    frame: &mut Frame,
    area: Rect,
    hits: &super::input::Hits,
    controls: FormActions<'_>,
) -> Rect {
    for action in controls.buttons {
        hits.present(*action);
    }
    if let Some(feedback) = controls.feedback {
        let height = area.height.min(2);
        design::notice(
            frame,
            Rect::new(area.x, area.bottom() - height, area.width, height),
            match feedback.tone {
                design::Tone::Neutral | design::Tone::Information => "Information",
                design::Tone::Success => "Complete",
                design::Tone::Attention => "Needs attention",
                design::Tone::Error => "Invalid input",
            },
            feedback.text,
            feedback.tone,
        );
        Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(height),
        )
    } else {
        area
    }
}
fn metadata_lines(
    value: &serde_json::Value,
    label: &str,
    depth: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let title = inline(label);
    match value {
        serde_json::Value::Object(values) if depth < 24 => {
            if !title.is_empty() {
                lines.push(Line::styled(
                    format!("{}{title}", "  ".repeat(depth.min(6))),
                    heading(),
                ));
            }
            if values.is_empty() {
                lines.push(metadata(&title, "None recorded"));
            }
            for (key, value) in values {
                metadata_lines(value, key, depth + 1, lines);
            }
        }
        serde_json::Value::Array(values) if depth < 24 => {
            lines.push(Line::styled(
                format!("{}{title} ({})", "  ".repeat(depth.min(6)), values.len()),
                heading(),
            ));
            if values.is_empty() {
                lines.push(Line::raw("None recorded"));
            }
            for (index, value) in values.iter().enumerate() {
                metadata_lines(value, &format!("{}", index + 1), depth + 1, lines);
            }
        }
        serde_json::Value::String(value) => lines.push(metadata(&title, value)),
        serde_json::Value::Null => lines.push(metadata(&title, "Not recorded")),
        value => lines.push(metadata(&title, &value.to_string())),
    }
}
/// Visual hierarchy for an operation's existing human report. This presentation
/// never supplies identity, eligibility or approval data to the executor.
pub fn review_text(body: &str) -> Text<'static> {
    // Public metadata reports are presented as labeled facts, never used to reconstruct approval.
    if body.trim_start().starts_with(['{', '['])
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
    {
        let mut lines = Vec::new();
        metadata_lines(&value, "", 0, &mut lines);
        return Text::from(lines);
    }
    let escaped = super::views::safe(body);
    let lines = escaped.lines().enumerate().map(|(index, line)| {
        let trimmed = line.trim_start();
        if index == 0 || trimmed.ends_with(':') {
            Line::styled(line.to_owned(), heading())
        } else if trimmed.starts_with("BLOCKED") || trimmed.starts_with("Blocker") {
            Line::styled(line.to_owned(), danger())
        } else if let Some(effect) = trimmed.strip_prefix("- ") {
            Line::from(vec![
                Span::styled(format!("{} ", glyphs::PENDING), muted()),
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
    text_view_at(frame, inner, body, usize::from(offset));
}
pub fn text_view_at(frame: &mut Frame, inner: Rect, body: Text<'static>, offset: usize) {
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
pub fn metadata(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<13} ", inline(label)), muted()),
        Span::raw(inline(value)),
    ])
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
