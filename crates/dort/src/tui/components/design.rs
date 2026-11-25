//! Shared Calm Workbench components. Approval logic stays in App.
use super::{self as ui, glyphs};
use crate::tui::{
    appearance::TextRole,
    input::{Hits, Target},
    views,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};

#[derive(Clone, Copy)]
pub enum Tone {
    Neutral,
    Information,
    Success,
    Attention,
    Error,
}
impl Tone {
    pub fn style(self) -> Style {
        match self {
            Self::Neutral => TextRole::Value,
            Self::Information => TextRole::Information,
            Self::Success => TextRole::Success,
            Self::Attention => TextRole::Warning,
            Self::Error => TextRole::Blocker,
        }
        .style()
    }
    fn symbol(self) -> &'static str {
        match self {
            Self::Neutral => glyphs::PENDING,
            Self::Information => "i",
            Self::Success => glyphs::COMPLETE,
            Self::Attention => "!",
            Self::Error => glyphs::FAILED,
        }
    }
}
const SLATE: Color = Color::Rgb(0x16, 0x24, 0x2e);
const TEXT: Color = Color::Rgb(0xe3, 0xed, 0xf4);
const TEAL: Color = Color::Rgb(0x77, 0xe2, 0xcd);

pub(super) fn chrome() -> Style {
    Style::default().fg(TEXT).bg(SLATE)
}
pub(super) fn brand() -> Style {
    chrome().fg(TEAL).add_modifier(Modifier::BOLD)
}
pub(super) fn muted_chrome() -> Style {
    chrome().fg(Color::Rgb(0xa3, 0xb7, 0xc7))
}
pub(super) fn active_rail() -> Style {
    inverse_pair(TEXT, SLATE)
}
pub(super) fn active_tab() -> Style {
    selection().add_modifier(Modifier::BOLD)
}
fn selection() -> Style {
    inverse_pair(TEXT, Color::Rgb(0x17, 0x36, 0x3c))
}
pub(super) fn focus_marker(inverse: Style) -> Style {
    inverse.bg(TEAL).add_modifier(Modifier::BOLD)
}
fn primary() -> Style {
    inverse_pair(Color::Rgb(0x0c, 0x22, 0x25), TEAL).add_modifier(Modifier::BOLD)
}
pub(super) fn secondary_key() -> Style {
    chrome()
        .fg(Color::Rgb(0xb9, 0xcb, 0xd8))
        .add_modifier(Modifier::BOLD)
}
fn inverse_pair(ink: Color, fill: Color) -> Style {
    Style::default()
        .fg(fill)
        .bg(ink)
        .add_modifier(Modifier::REVERSED)
}
#[derive(Clone, Copy)]
pub enum PaneFocus {
    Active,
    Inactive,
}

impl From<bool> for PaneFocus {
    fn from(focused: bool) -> Self {
        if focused {
            Self::Active
        } else {
            Self::Inactive
        }
    }
}

pub fn pane(title: &str, focus: impl Into<PaneFocus>) -> Block<'static> {
    let heading = TextRole::Heading.style();
    match focus.into() {
        PaneFocus::Active => titled_block(
            &format!("{title} [Active]"),
            Borders::TOP,
            heading.add_modifier(Modifier::UNDERLINED),
        ),
        PaneFocus::Inactive => titled_block(title, Borders::TOP, heading),
    }
}

pub(super) fn titled_block(title: &str, borders: Borders, style: Style) -> Block<'static> {
    Block::default()
        .borders(borders)
        .border_style(Style::default().add_modifier(Modifier::DIM))
        .title(Span::styled(
            format!(" {} ", ui::chrome_text(title)),
            style.remove_modifier(Modifier::DIM),
        ))
}
fn backdrop(mut style: Style) -> Style {
    if style.add_modifier.contains(Modifier::REVERSED)
        && matches!((style.fg, style.bg), (Some(fg), Some(bg)) if fg != Color::Reset && bg != Color::Reset)
    {
        style = style.fg(Color::Reset).bg(Color::Reset);
    }
    style
        .remove_modifier(Modifier::BOLD | Modifier::REVERSED)
        .add_modifier(Modifier::DIM)
}
pub fn badge(label: &'static str, tone: Tone) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{} ", tone.symbol()), tone.style()),
        Span::styled(label, tone.style()),
    ])
}
pub fn notice(frame: &mut Frame, area: Rect, title: &'static str, body: &str, tone: Tone) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(badge(title, tone)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(Text::from(views::safe(body))).wrap(Wrap { trim: false }),
            Rect::new(area.x, area.y + 1, area.width, area.height - 1),
        );
    }
}

pub struct DialogAreas {
    pub body: Rect,
    pub footer: Rect,
}

pub fn dialog_inner_width(width: u16) -> u16 {
    dialog_width(width).saturating_sub(2)
}

fn dialog_width(width: u16) -> u16 {
    width.saturating_sub(2).min(144)
}

pub fn choice_dialog(frame: &mut Frame, title: &'static str, hits: &Hits) -> DialogAreas {
    let area = frame.area();
    for cell in &mut frame.buffer_mut().content {
        cell.set_style(backdrop(cell.style()));
    }
    hits.clear();
    hits.input_focus(false);
    let width = dialog_width(area.width);
    let height = area.height.saturating_sub(2).min(32);
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    for edge in [
        Rect::new(area.x, area.y, area.width, modal.y - area.y),
        Rect::new(
            area.x,
            modal.bottom(),
            area.width,
            area.bottom() - modal.bottom(),
        ),
        Rect::new(area.x, modal.y, modal.x - area.x, modal.height),
        Rect::new(
            modal.right(),
            modal.y,
            area.right() - modal.right(),
            modal.height,
        ),
    ] {
        hits.click(edge, Target::DismissDialog);
    }
    frame.render_widget(Clear, modal);
    let block = titled_block(title, Borders::ALL, TextRole::Heading.style());
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );
    let footer = Rect::new(inner.x, body.bottom(), inner.width, inner.height.min(1));
    DialogAreas { body, footer }
}

/// One-row filter. Raw metadata is escaped; only the caller registers its typed hit.
pub fn filter(
    frame: &mut Frame,
    area: Rect,
    label: &'static str,
    value: &str,
    focused: bool,
) -> Rect {
    let label_width = (label.len() as u16 + 1).min(area.width);
    frame.render_widget(
        Paragraph::new(label).style(TextRole::Label.style()),
        Rect::new(area.x, area.y, label_width, area.height.min(1)),
    );
    let input = Rect::new(
        area.x + label_width,
        area.y,
        area.width.saturating_sub(label_width),
        area.height.min(1),
    );
    if input.width < 3 || input.height == 0 {
        return input;
    }
    let width = usize::from(input.width - 2);
    let escaped = ui::inline(value);
    // Escaping produces ASCII, so a tail cannot split an untrusted terminal cell.
    let value_width = width.saturating_sub(usize::from(focused));
    let visible = &escaped[escaped.len().saturating_sub(value_width)..];
    let boundary = if focused { ui::accent() } else { ui::muted() };
    let line = Line::from(vec![
        Span::styled("[", boundary),
        Span::raw(format!("{visible:width$}")),
        Span::styled("]", boundary),
    ]);
    frame.render_widget(Paragraph::new(line), input);
    if focused {
        frame.set_cursor_position((input.x + 1 + visible.len() as u16, input.y));
    }
    input
}

pub fn choice(frame: &mut Frame, area: Rect, value: &str, focused: bool) {
    if area.width < 5 || area.height == 0 {
        return;
    }
    let hint = " Enter/Space";
    let hint = if area.width >= 5 + hint.len() as u16 {
        hint
    } else {
        ""
    };
    let visible = ui::clip(value, area.width - 4 - hint.len() as u16);
    let boundary = if focused { ui::accent() } else { ui::muted() };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[", boundary),
            Span::raw(format!("{visible} ")),
            Span::styled(glyphs::MENU, boundary),
            Span::styled("]", boundary),
            Span::styled(hint, ui::muted()),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

/// On/off row. Space flips it; Enter still reviews the whole form.
pub fn toggle(frame: &mut Frame, area: Rect, on: bool, focused: bool) {
    if area.width < 5 || area.height == 0 {
        return;
    }
    let boundary = if focused { ui::accent() } else { ui::muted() };
    let hint = if focused && area.width >= 12 {
        "  Space"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(if on { "[x]" } else { "[ ]" }, boundary),
            Span::raw(if on { " On" } else { " Off" }),
            Span::styled(hint, ui::muted()),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

pub struct PairRow<'a> {
    pub identity: usize,
    pub values: [&'a str; 2],
}
pub struct PairTable<'a> {
    pub headers: [&'static str; 2],
    pub rows: &'a [PairRow<'a>],
    pub selected: usize,
}
pub fn pair_capacity(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(1))
}
pub fn pairs(
    frame: &mut Frame,
    area: Rect,
    table: PairTable<'_>,
    hits: &Hits,
    target: impl Fn(usize) -> Target,
) {
    pairs_with_disclosure(frame, area, table, hits, target, false);
}

/// Metadata stays escaped; the optional disclosure glyph is trusted decoration.
pub fn pairs_with_disclosure(
    frame: &mut Frame,
    area: Rect,
    table: PairTable<'_>,
    hits: &Hits,
    target: impl Fn(usize) -> Target,
    disclosure: bool,
) {
    if area.height < 2 {
        return;
    }
    let right = (area.width / 3).clamp(6, 24).min(area.width);
    let left = area.width.saturating_sub(right + 4);
    let rows = table
        .rows
        .iter()
        .take(pair_capacity(area))
        .enumerate()
        .map(|(offset, row)| {
            hits.click(
                Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1),
                target(row.identity),
            );
            let current = row.identity == table.selected;
            let marker = if current {
                Cell::from(glyphs::SELECTED).style(focus_marker(selection()))
            } else {
                Cell::from(" ")
            };
            Row::new([
                marker,
                Cell::from(Line::from(vec![
                    Span::raw(ui::clip(
                        row.values[0],
                        left.saturating_sub(if disclosure { 2 } else { 0 }),
                    )),
                    Span::raw(if disclosure {
                        format!(" {}", glyphs::EXPAND)
                    } else {
                        String::new()
                    }),
                ])),
                Cell::from(ui::clip(row.values[1], right)),
            ])
            .style(if current {
                selection()
            } else {
                Tone::Neutral.style()
            })
        });
    let header = Row::new(["", table.headers[0], table.headers[1]]).style(TextRole::Label.style());
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(right),
            ],
        )
        .column_spacing(1)
        .header(header),
        area,
    );
}

/// A single foreground strip. Labels are trusted; actions are render-owned typed hits.
pub struct StripAction {
    pub key: &'static str,
    pub label: &'static str,
    pub target: Target,
    pub primary: bool,
}
pub fn action_strip(frame: &mut Frame, area: Rect, actions: &[StripAction], hits: &Hits) {
    debug_assert!(
        actions.iter().filter(|action| action.primary).count() <= 1,
        "one foreground primary action"
    );
    frame.render_widget(
        Paragraph::default().style(muted_chrome().remove_modifier(Modifier::DIM)),
        area,
    );
    let mut x = area.x;
    for action in actions {
        let label = format!(" {} {} ", action.key, action.label);
        let width = Line::raw(&label).width() as u16;
        if x + width > area.right() {
            break;
        }
        let rect = Rect::new(x, area.y, width, area.height.min(1));
        let line = if action.primary {
            Line::styled(label, primary())
        } else {
            Line::from(vec![
                Span::styled(format!(" {} ", action.key), secondary_key()),
                Span::raw(format!("{} ", action.label)),
            ])
        };
        frame.render_widget(Paragraph::new(line), rect);
        hits.click(rect, action.target);
        x += width + 1;
    }
}
