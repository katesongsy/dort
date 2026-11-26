//! Choice lists and details.
use super::{
    self as ui,
    design::{self, PairRow, PairTable},
};
use crate::tui::input::{Hits, Target, Wheel};
use ratatui::{Frame, layout::Rect};
pub struct View<'a> {
    pub rows: &'a [(&'a str, &'a str)],
    pub column: &'static str,
    pub selected: usize,
    pub details: bool,
    pub scroll: u16,
    pub note: Option<&'static str>,
    pub feedback: Option<&'static str>,
}
pub fn draw(frame: &mut Frame, area: Rect, hits: &Hits, view: View<'_>) {
    hits.wheel(
        area,
        if view.details {
            Wheel::Overlay
        } else {
            Wheel::Choices
        },
    );
    if view.details {
        let text = view
            .rows
            .get(view.selected)
            .map(|(name, scope)| format!("{name}\n\n{scope}"))
            .unwrap_or_else(|| view.note.unwrap_or("No choice available.").into());
        ui::report(
            frame,
            area,
            "Choice details",
            ui::review_text(&text),
            view.scroll,
            false,
        );
        return;
    }
    let feedback_height = u16::from(view.feedback.is_some()).min(area.height);
    let note_height = view
        .note
        .map_or(0, |note| {
            1 + u16::from(note.len() > usize::from(area.width))
        })
        .min(area.height - feedback_height);
    let table = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height - feedback_height - note_height,
    );
    let capacity = design::pair_capacity(table).max(1);
    let offset = view.selected.saturating_sub(capacity - 1);
    let rows: Vec<_> = view
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(capacity)
        .map(|(identity, (name, scope))| PairRow {
            identity,
            values: [name, scope],
        })
        .collect();
    design::pairs(
        frame,
        table,
        PairTable {
            headers: ["CHOICE", view.column],
            rows: &rows,
            selected: view.selected,
        },
        hits,
        Target::ChoiceRow,
    );
    if let Some(note) = view.note {
        ui::chrome_prose(
            frame,
            Rect::new(area.x, table.bottom(), area.width, note_height),
            note,
            ui::muted(),
        );
    }
    if let Some(feedback) = view.feedback {
        ui::chrome_prose(
            frame,
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            feedback,
            ui::warning(),
        );
    }
}
