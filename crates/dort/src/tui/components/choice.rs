//! One bounded selection surface for providers and explicit inspection bindings.
use super::{
    self as ui,
    design::{self, PairRow, PairTable},
};
use crate::tui::input::{Hits, Target};
use ratatui::{Frame, layout::Rect};
pub struct View<'a> {
    pub title: &'static str,
    pub rows: &'a [(&'a str, &'a str)],
    pub selected: usize,
    pub details: bool,
    pub scroll: u16,
    pub feedback: Option<&'static str>,
}
pub fn draw(frame: &mut Frame, hits: &Hits, view: View<'_>) {
    let area = design::choice_dialog(frame, view.title, hits);
    if view.details {
        let text = view
            .rows
            .get(view.selected)
            .map(|(name, scope)| format!("{name}\n\n{scope}"))
            .unwrap_or_else(|| "No choice available.".into());
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
    let extra = u16::from(view.feedback.is_some()).min(area.height);
    let table = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(extra),
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
            headers: ["CHOICE", "SCOPE"],
            rows: &rows,
            selected: view.selected,
        },
        hits,
        Target::ChoiceRow,
    );
    if let Some(feedback) = view.feedback {
        ui::chrome_prose(
            frame,
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            feedback,
            ui::warning(),
        );
    }
}
