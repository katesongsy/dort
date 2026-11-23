//! Content-free history presentation. Recovery previews make the final decision.
use super::{app::App, components as ui};
use dort_core::journal::{Diagnosis, Phase};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};

pub fn rollback(row: &Diagnosis) -> bool {
    row.problem.is_none()
        && matches!(row.journal_schema, Some(1 | 2 | 4 | 5 | 6))
        && row.phase.is_some_and(|p| p != Phase::RolledBack)
}
pub fn abandon(row: &Diagnosis) -> bool {
    row.problem.is_none()
        && row.journal_schema == Some(3)
        && row.phase.is_some_and(|p| !p.terminal())
}
pub fn resume(row: &Diagnosis) -> bool {
    abandon(row) && row.phase == Some(Phase::Running)
}
pub fn matches(search: &super::inventory::Search, row: &Diagnosis) -> bool {
    search.matches(&row.repository_id)
        || search.matches(&row.transaction_id)
        || search.matches(&format!(
            "{} {} steps {} {} {}",
            kind(row),
            phase(row),
            row.valid_checkpoints,
            if row.blocks_writes() {
                "blocks writes"
            } else {
                "finished"
            },
            row.problem.as_deref().unwrap_or("valid")
        ))
}
fn kind(row: &Diagnosis) -> &'static str {
    match row.journal_schema {
        Some(1) => "File adoption",
        Some(2) => "File change",
        Some(3) => "Repository setup",
        Some(4) => "File batch",
        Some(5 | 6) => "Tool declaration",
        _ => "Unknown",
    }
}
fn phase(row: &Diagnosis) -> &'static str {
    match row.phase {
        Some(Phase::Preparing) => "Preparing",
        Some(Phase::Running) => "Running",
        Some(Phase::Verifying) => "Verifying",
        Some(Phase::Completed) => "Completed",
        Some(Phase::RollingBack) => "Rolling back",
        Some(Phase::RolledBack) => "Rolled back",
        None => "Unknown",
    }
}
fn state(row: &Diagnosis) -> (&'static str, Style) {
    if row.problem.is_some() {
        ("Damaged", ui::danger())
    } else if row.blocks_writes() {
        ("Blocks writes", ui::warning())
    } else {
        ("Finished", ui::muted())
    }
}
fn field(label: &str, value: impl ToString) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), ui::muted()),
        Span::raw(ui::inline(&value.to_string())),
    ])
}
fn note(value: &str, style: Style) -> Line<'static> {
    Line::styled(ui::inline(value), style)
}
fn details(app: &App) -> Text<'static> {
    let Some(row) = app.history_selection() else {
        return Text::raw("No operation record selected. Clear the filter.");
    };
    let (status, style) = state(row);
    let mut lines = vec![
        Line::styled(format!("[{status}]"), style),
        Line::styled("Operation", ui::heading()),
        field("Repository ID", &row.repository_id),
        field("Operation ID", &row.transaction_id),
        field("Type", kind(row)),
        field("State", phase(row)),
        field("Saved steps", row.valid_checkpoints),
        field("Integrity", row.problem.as_deref().unwrap_or("Valid")),
        field("Blocks writes", row.blocks_writes()),
        Line::default(),
        Line::styled("Recovery options", ui::heading()),
    ];
    if rollback(row) {
        app.presented(
            "Rollback",
            "b",
            super::input::Target::Action(super::shell::Action::Rollback),
        );
        lines.push(field("Rollback", "Available for review"));
    } else {
        lines.push(note(
            "Rollback is unavailable for this record.",
            ui::muted(),
        ));
    }
    if resume(row) {
        app.presented(
            "Resume",
            "s",
            super::input::Target::Action(super::shell::Action::Resume),
        );
        lines.push(field("Resume", "Available for review"));
    } else if abandon(row) && row.phase == Some(Phase::Preparing) {
        lines.push(note(
            "Resume is unavailable: setup is incomplete. Abandon it, then create or attach again.",
            ui::warning(),
        ));
    } else {
        lines.push(note(
            "Resume is unavailable: setup is not intact.",
            ui::muted(),
        ));
    }
    if abandon(row) {
        app.presented(
            "Abandon",
            "x",
            super::input::Target::Action(super::shell::Action::Abandon),
        );
        lines.push(field("Abandon", "Available for review"));
        lines.push(note(
            "Keeps the published repository, registration and private files.",
            ui::warning(),
        ));
    } else {
        lines.push(note(
            "Abandon is unavailable: setup is incomplete or not intact.",
            ui::muted(),
        ));
    }
    lines.push(Line::default());
    lines.push(Line::styled("What recovery keeps", ui::heading()));
    lines.push(note(
        "Recovery checks current paths and files before each action.",
        Style::default(),
    ));
    lines.push(note(
        "Rollback restores files only when they still match.",
        ui::warning(),
    ));
    lines.push(note(
        "Tool declarations restore only an unchanged manifest.",
        ui::warning(),
    ));
    lines.push(note(
        "Packages are not removed. Private backups stay.",
        ui::warning(),
    ));
    Text::from(lines)
}

fn search(frame: &mut Frame, area: Rect, app: &App) {
    ui::design::filter(
        frame,
        area,
        "Filter history",
        &app.history_search.query,
        app.history_search.editing && app.hits.input_focus_allowed() && !app.busy,
    );
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
    ])
    .split(area);
    let available = app.data.as_ref().filter(|d| d.history_error.is_none());
    let (total, blockers) = available
        .map(|d| {
            (
                d.history.len().to_string(),
                d.history
                    .iter()
                    .filter(|r| r.blocks_writes())
                    .count()
                    .to_string(),
            )
        })
        .unwrap_or_else(|| ("Unknown".into(), "Unknown".into()));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}/{total} shown", app.visible_history_rows().len()),
                ui::heading(),
            ),
            Span::styled(
                format!(" | Blocked writes: {blockers}"),
                if available.is_some_and(|d| !d.history.iter().any(|r| r.blocks_writes())) {
                    ui::muted()
                } else {
                    ui::danger()
                },
            ),
        ])),
        rows[0],
    );
    search(frame, rows[1], app);
    if app.history_search.editing {
        app.presented(
            "Done",
            "Enter",
            super::input::Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        );
    } else if app.history_details {
        app.presented("Back", "Esc", super::input::Target::Back);
    } else if available.is_some() && !app.visible_history_rows().is_empty() {
        app.presented(
            "Details",
            "Enter",
            super::input::Target::Action(super::shell::Action::Inspect),
        );
    }
    app.hits.click(rows[1], super::input::Target::Filter);
    let content = rows[2];
    let focused = true;
    let Some(data) = &app.data else {
        ui::report(
            frame,
            content,
            "Recovery | Not loaded",
            Text::raw("Recovery has not loaded yet."),
            0,
            focused,
        );
        return;
    };
    if let Some(error) = &data.history_error {
        ui::report(
            frame,
            content,
            "Recovery unavailable",
            Text::from(vec![
                note(error, ui::warning()),
                Line::raw("Writes stay blocked until recovery loads."),
            ]),
            app.details_scroll,
            focused,
        );
        return;
    }
    let wide = super::workbench::wide_details(area, 2);
    if app.history_details {
        ui::report(
            frame,
            content,
            "Operation details",
            details(app),
            app.details_scroll,
            focused,
        );
        return;
    }
    let panes = if wide {
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).split(content)
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(content)
    };
    let visible = app.visible_history_rows();
    let title = format!(
        "Operations | {} / {} shown",
        visible.len(),
        data.history.len()
    );
    let block = ui::pane(&title, focused);
    if data.history.is_empty() || visible.is_empty() {
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        ui::text_view(
            frame,
            inner,
            Text::raw(if data.history.is_empty() {
                "History is empty."
            } else {
                "No matches. Blocks stay visible above."
            }),
            0,
        );
    } else {
        let table_rows = visible.iter().map(|&i| {
            let row = &data.history[i];
            let (status, style) = state(row);
            Row::new(vec![
                Cell::from(ui::inline(row.transaction_id.get(..8).unwrap_or("invalid"))),
                Cell::from(kind(row)),
                Cell::from(phase(row)),
                Cell::from(status).style(style),
            ])
        });
        let table = Table::new(
            table_rows,
            [
                Constraint::Min(8),
                Constraint::Length(16),
                Constraint::Length(12),
                Constraint::Length(13),
            ],
        )
        .header(Row::new(["OP ID", "TYPE", "STATE", "STATUS"]).style(ui::heading()))
        .block(block)
        .row_highlight_style(ui::selected())
        .highlight_symbol(format!("{} ", ui::glyphs::SELECTED));
        let mut selection =
            TableState::default().with_selected(visible.iter().position(|&i| i == app.selected));
        frame.render_stateful_widget(table, panes[0], &mut selection);
        app.hits.rows(panes[0], &visible, selection.offset());
    }
    if wide {
        ui::report(
            frame,
            panes[1],
            "Operation details",
            details(app),
            app.details_scroll,
            true,
        );
    }
}
