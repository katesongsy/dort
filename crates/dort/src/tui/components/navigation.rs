//! Shared navigation presentation. Controllers supply only trusted labels and typed hits.
use super::{self as ui, design};
use crate::tui::input::{Hits, Target};
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

pub struct Destination {
    pub label: &'static str,
    pub target: Target,
    pub selected: bool,
}

/// A horizontally scrolling row; the selected destination is always visible.
fn destinations(
    frame: &mut Frame,
    area: Rect,
    items: &[Destination],
    hits: &Hits,
    reserve: u16,
) -> bool {
    let selected = items.iter().position(|item| item.selected).unwrap_or(0);
    let budget = area.width.saturating_sub(reserve);
    let cells = |item: &Destination| Line::raw(item.label).width() as u16 + 3;
    let mut start = 0;
    while start < selected
        && items[start..=selected].iter().map(cells).sum::<u16>() + if start > 0 { 2 } else { 0 }
            > budget
    {
        start += 1;
    }
    let mut x = area.x;
    if start > 0 && budget > 2 {
        frame.render_widget(Paragraph::new("<"), Rect::new(x, area.y, 2, 1));
        x += 2;
    }
    let mut end = start;
    for item in &items[start..] {
        let remaining = area.x + budget - x;
        if remaining < 4 {
            break;
        }
        let width = cells(item).min(remaining);
        if end > start && cells(item) > remaining {
            break;
        }
        let text = format!(" {} ", ui::chrome_clip(item.label, width.saturating_sub(2)));
        let rect = Rect::new(x, area.y, width, 1);
        frame.render_widget(
            Paragraph::new(text).style(if item.selected {
                design::active_tab()
            } else {
                ui::muted()
            }),
            rect,
        );
        hits.click(rect, item.target);
        x += width;
        end += 1;
    }
    start > 0 || end < items.len()
}

/// Persistent local destinations use the same selection and typed-click vocabulary.
pub fn local(
    frame: &mut Frame,
    area: Rect,
    items: &[Destination],
    overflow: Option<Target>,
    hits: &Hits,
) {
    if area.height == 0 || items.is_empty() {
        return;
    }
    let width: usize = items
        .iter()
        .map(|item| Line::raw(item.label).width() + 3)
        .sum();
    let reserve = if width > usize::from(area.width) {
        9
    } else {
        0
    };
    if destinations(frame, area, items, hits, reserve) && reserve > 0 {
        let rect = Rect::new(area.right().saturating_sub(reserve), area.y, reserve, 1);
        frame.render_widget(Paragraph::new(" More... ").style(ui::heading()), rect);
        if let Some(overflow) = overflow {
            hits.click(rect, overflow);
        }
    }
}
