//! Bounded metadata-only choices for More and explicit form choices.
use super::{
    self as ui,
    design::{self, PairRow, PairTable},
};
use crate::tui::input::{Hits, Target};
use ratatui::{Frame, layout::Rect};

#[derive(Clone, Copy)]
pub struct Item {
    pub action: ui::ActionButton,
    pub reason: Option<&'static str>,
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    items: &[Item],
    selected: usize,
    details: bool,
    feedback: Option<&'static str>,
    hits: &Hits,
) {
    let reason = items
        .get(selected)
        .and_then(|item| item.reason)
        .or(feedback);
    if details {
        let item = items.get(selected);
        let body = item
            .map(|item| {
                format!(
                    "{}\n\n{}",
                    item.action.label,
                    item.reason
                        .unwrap_or("Available here. Choosing it runs the same action.")
                )
            })
            .unwrap_or_else(|| "No additional actions.".into());
        ui::report(
            frame,
            area,
            "Action details",
            ui::review_text(&body),
            0,
            true,
        );
        return;
    }
    let extra = u16::from(reason.is_some()).min(area.height);
    let table = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(extra),
    );
    let capacity = design::pair_capacity(table).max(1);
    let offset = selected.saturating_sub(capacity - 1);
    let rows: Vec<_> = items
        .iter()
        .enumerate()
        .skip(offset)
        .take(capacity)
        .map(|(identity, item)| PairRow {
            identity,
            values: [
                item.action.label,
                if item.reason.is_some() {
                    "Unavailable"
                } else {
                    item.action.key
                },
            ],
        })
        .collect();
    design::pairs(
        frame,
        table,
        PairTable {
            headers: ["ACTION", "KEY"],
            rows: &rows,
            selected,
        },
        hits,
        Target::MoreRow,
    );
    if items.is_empty() {
        ui::prose(frame, area, "No additional actions here.", ui::muted());
    }
    if let Some(reason) = reason {
        ui::prose(
            frame,
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            reason,
            ui::warning(),
        );
    }
}
