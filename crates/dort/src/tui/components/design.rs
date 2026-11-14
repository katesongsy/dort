//! Semantic, reusable Calm Workbench components. No execution or approval policy.
use super::{self as ui, glyphs};
use crate::tui::{
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
            Self::Neutral => Style::default(),
            Self::Information => Style::default().fg(Color::Blue),
            Self::Success => Style::default().fg(Color::Green),
            Self::Attention => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Self::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
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
pub fn selection() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}
pub fn table_heading() -> Style {
    Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}
pub fn primary() -> Style {
    // Inverse preserves the primary surface when monochrome resets both colors.
    Style::default()
        .fg(Color::Cyan)
        .bg(Color::Black)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
}
pub fn backdrop(style: Style) -> Style {
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

/// Bounded foreground choice surface; the caller renders the one global action strip.
pub fn choice_dialog(frame: &mut Frame, title: &'static str, hits: &Hits) -> Rect {
    let area = frame.area();
    for cell in &mut frame.buffer_mut().content {
        cell.set_style(backdrop(cell.style()));
    }
    hits.clear();
    hits.input_focus(false);
    hits.click(area, Target::NavigationBlocked);
    let width = area.width.saturating_sub(2).min(72);
    let height = area.height.saturating_sub(3).min(16);
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height.saturating_sub(1) - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(format!(" {title} "), ui::heading()));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    inner
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
        Paragraph::new(label),
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
    let visible = &escaped[escaped.len().saturating_sub(width)..];
    let boundary = if focused { ui::accent() } else { ui::muted() };
    let line = Line::from(vec![
        Span::styled("[", boundary),
        Span::raw(format!("{visible:width$}")),
        Span::styled("]", boundary),
    ]);
    frame.render_widget(Paragraph::new(line), input);
    if focused {
        frame.set_cursor_position((
            input.x + 1 + visible.len().min(width.saturating_sub(1)) as u16,
            input.y,
        ));
    }
    input
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
            Row::new([
                Cell::from(if current { glyphs::SELECTED } else { " " }).style(ui::accent()),
                Cell::from(ui::clip(row.values[0], left)),
                Cell::from(ui::clip(row.values[1], right)),
            ])
            .style(if current {
                selection()
            } else {
                Tone::Neutral.style()
            })
        });
    let header = Row::new(["", table.headers[0], table.headers[1]]).style(table_heading());
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
                Span::styled(
                    format!(" {} ", action.key),
                    Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED),
                ),
                Span::raw(format!("{} ", action.label)),
            ])
        };
        frame.render_widget(Paragraph::new(line), rect);
        hits.click(rect, action.target);
        x += width + 1;
    }
}
