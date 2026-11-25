//! Shared visual components. Metadata is escaped before styling.
pub mod approval;
pub mod choice;
pub mod chrome;
pub mod collection;
pub mod dashboard;
pub mod design;
pub mod glyphs;
pub mod help;
pub mod menu;
pub mod motion;
pub mod navigation;
pub mod reader;
pub use design::pane;
pub use reader::{metadata, report, review_text, text_view};

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

use super::appearance::TextRole;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
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
    TextRole::Heading.style()
}
/// Subdued secondary text, such as a directory name in a mixed list.
pub fn note() -> Style {
    crate::tui::appearance::TextRole::Note.style()
}
pub fn muted() -> Style {
    TextRole::Value.style()
}
pub fn selected() -> Style {
    TextRole::Label.style().add_modifier(Modifier::REVERSED)
}
pub fn heading() -> Style {
    TextRole::Label.style()
}
pub fn warning() -> Style {
    TextRole::Warning.style()
}
pub fn danger() -> Style {
    TextRole::Blocker.style()
}
pub fn success() -> Style {
    design::Tone::Success.style()
}
pub fn state(label: &str) -> Style {
    match label {
        "Damaged" => danger(),
        "Blocked" => warning(),
        "Not eligible" | "Ineligible" => muted(),
        "Eligible" | "Linked" => success(),
        "Managed" => accent(),
        "Needs apply" => warning(),
        _ => Style::default(),
    }
}
pub fn inline(value: &str) -> String {
    super::views::safe(value).replace('\n', "\\n")
}
/// Chrome text: escape control characters, keep printable Unicode such as
/// spinner frames and box drawing. Titles are app-composed, never provider output.
pub fn chrome_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
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
    /// Field index and on/off state for rows without a text input.
    pub toggles: &'a [(usize, bool)],
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
    let toggles = controls.toggles;
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
            Paragraph::new(inline(fields[i].0)).style(TextRole::Label.style()),
            Rect::new(fields_area.x, y, fields_area.width, 1),
        );
        let rect = Rect::new(fields_area.x, y + 1, fields_area.width, 1);
        if let Some(on) = toggles
            .iter()
            .find(|(index, _)| *index == i)
            .map(|(_, on)| *on)
        {
            design::toggle(frame, rect, on, active);
        } else if choices.contains(&i) {
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
