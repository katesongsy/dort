//! File reviews rendered from typed plans, never parsed from a human report.
use super::{app::Preview, components as ui, views::safe};
use dort_core::planner::{Effect, Outcome, Plan};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};
fn outcome(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Change => "Change",
        Outcome::NoOp => "Unchanged",
        Outcome::Blocked => "Blocked",
    }
}
fn effect(effect: &Effect) -> &'static str {
    match effect {
        Effect::Backup { .. } => "Back up destination",
        Effect::EnsureParents { .. } => "Create parents",
        Effect::CopySource { .. } => "Copy source",
        Effect::AddMapping { .. } => "Add mapping",
        Effect::RemoveMapping { .. } => "Remove mapping",
        Effect::Link { .. } => "Create link",
        Effect::Materialize { .. } => "Keep ordinary file",
        Effect::RemoveDestination { .. } => "Delete managed link",
    }
}
fn path(label: &str, path: &std::path::Path) -> Line<'static> {
    ui::metadata(label, path.to_str().unwrap_or("unavailable"))
}
fn detail(plan: &Plan, selected: usize) -> Text<'static> {
    let Some(file) = plan.files.get(selected) else {
        return Text::from("Everything is up to date.");
    };
    let mut lines = vec![Line::styled(
        format!("{}  [{}]", safe(&file.id), outcome(&file.outcome)),
        ui::heading(),
    )];
    if let Some(input) = plan.inputs.files.get(selected) {
        lines.push(path("Source", &input.source_path));
        lines.push(path("Destination", &input.target_path));
    }
    if file.destructive {
        lines.push(Line::styled(
            "Destructive destination effect; inspect all effects below.",
            ui::warning(),
        ));
    }
    if file.requires_removal_acknowledgement {
        lines.push(Line::styled(
            "Separate exact-ID acknowledgement required before literal yes.",
            ui::danger(),
        ));
    }
    for blocker in &file.blockers {
        lines.push(Line::styled(
            format!("BLOCKED: {}", safe(blocker)),
            ui::danger(),
        ));
    }
    for (index, e) in file.effects.iter().enumerate() {
        lines.push(Line::default());
        lines.push(Line::styled(
            format!("{}. {}", index + 1, effect(e)),
            ui::accent(),
        ));
        match e {
            Effect::Backup { path: p } => lines.push(path("Backup", p)),
            Effect::EnsureParents {
                path: p,
                permissions,
            } => {
                lines.push(path("Parents", p));
                lines.push(ui::metadata("Permissions", permissions));
            }
            Effect::CopySource {
                from,
                to,
                permissions,
            } => {
                lines.push(path("From", from));
                lines.push(path("To", to));
                lines.push(ui::metadata("Permissions", permissions));
            }
            Effect::AddMapping { manifest, mapping } => {
                lines.push(path("Manifest", manifest));
                lines.push(ui::metadata("Mapping ID", &mapping.id));
            }
            Effect::RemoveMapping { manifest, id } => {
                lines.push(path("Manifest", manifest));
                lines.push(ui::metadata("Mapping ID", id));
            }
            Effect::Link { target, source } => {
                lines.push(path("Link", target));
                lines.push(path("Points to", source));
            }
            Effect::Materialize {
                source,
                target,
                permissions,
            } => {
                lines.push(path("From", source));
                lines.push(path("To", target));
                lines.push(ui::metadata("Permissions", permissions));
            }
            Effect::RemoveDestination { target } => {
                lines.push(path("Delete link", target));
                lines.push(Line::raw("Repository source is retained."));
            }
        }
    }
    lines.push(Line::styled(
        "i: full report, permissions, warnings and guards",
        ui::muted(),
    ));
    Text::from(lines)
}
pub fn draw_batch(
    frame: &mut Frame,
    area: Rect,
    batch: &dort_core::batch::Plan,
    preview: &Preview,
    hits: &super::input::Hits,
) {
    // Render-only aggregation; approval always consumes the untouched real Batch plan.
    let Some(first) = batch.items.first() else {
        return;
    };
    let mut display = first.clone();
    display.files = batch
        .items
        .iter()
        .flat_map(|p| p.files.iter().cloned())
        .collect();
    display.inputs.files = batch
        .items
        .iter()
        .flat_map(|p| p.inputs.files.iter().cloned())
        .collect();
    // Combined conflicts block every item, even if its isolated plan could change.
    // Keep the reasons in the scrollable selected-effects view as well as the full report.
    if !batch.blockers.is_empty() {
        for file in &mut display.files {
            file.outcome = Outcome::Blocked;
            file.blockers
                .extend(batch.blockers.iter().map(|b| format!("Batch: {b}")));
        }
    }
    draw_inner(
        frame,
        area,
        &display,
        preview,
        hits,
        "Exact marked set · one transaction · failure may be partial",
    );
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    plan: &Plan,
    preview: &Preview,
    hits: &super::input::Hits,
) {
    draw_inner(
        frame,
        area,
        plan,
        preview,
        hits,
        "Scope: all mappings; display filters never narrow it.",
    );
}
fn draw_inner(
    frame: &mut Frame,
    area: Rect,
    plan: &Plan,
    preview: &Preview,
    hits: &super::input::Hits,
    scope: &str,
) {
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(if area.height >= 12 { 5 } else { 3 }),
        Constraint::Min(1),
    ])
    .split(area);
    let count = |predicate: fn(&Outcome) -> bool| {
        plan.files.iter().filter(|f| predicate(&f.outcome)).count()
    };
    let change = count(|o| matches!(o, Outcome::Change));
    let unchanged = count(|o| matches!(o, Outcome::NoOp));
    let blocked = count(|o| matches!(o, Outcome::Blocked));
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("Review · {change} changes  {unchanged} unchanged  {blocked} blocked"),
                if blocked > 0 {
                    ui::danger()
                } else {
                    ui::heading()
                },
            ),
            Line::styled(scope, ui::muted()),
        ]),
        parts[0],
    );
    let rows: Vec<_> = plan
        .files
        .iter()
        .map(|f| {
            Row::new(vec![
                Cell::from(ui::inline(&f.id)),
                Cell::from(outcome(&f.outcome)).style(ui::state(outcome(&f.outcome))),
                Cell::from(f.effects.first().map(effect).unwrap_or("None")),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Length(11),
            Constraint::Min(8),
        ],
    )
    .header(Row::new(["FILE", "STATUS", "PLANNED CHANGE"]).style(ui::heading()))
    .row_highlight_style(ui::selected())
    .highlight_symbol("> ");
    let mut state =
        TableState::default().with_selected((!plan.files.is_empty()).then_some(preview.selected));
    frame.render_stateful_widget(table, parts[1], &mut state);
    hits.wheel(parts[1], super::input::Wheel::PreviewRows);
    for (y, index) in (state.offset()..plan.files.len())
        .take(usize::from(parts[1].height.saturating_sub(1)))
        .enumerate()
    {
        hits.click(
            Rect::new(parts[1].x, parts[1].y + 1 + y as u16, parts[1].width, 1),
            super::input::Target::PreviewRow(index),
        );
    }
    if parts[2].height >= 3 {
        ui::report(
            frame,
            parts[2],
            "Change details · i full report",
            detail(plan, preview.selected),
            preview.effect_scroll,
            true,
        );
    } else {
        ui::prose(
            frame,
            parts[2],
            "i: inspect complete effects, paths and blockers",
            ui::accent(),
        );
    }
}
