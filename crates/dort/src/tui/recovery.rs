//! Typed content-free history presentation. Eligibility is advisory; core previews decide.
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
        && matches!(row.journal_schema, Some(1 | 2 | 4))
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
            "schema {:?} phase {:?} checkpoints {} {} {}",
            row.journal_schema,
            row.phase,
            row.valid_checkpoints,
            if row.blocks_writes() {
                "blocks writes"
            } else {
                "terminal"
            },
            row.problem.as_deref().unwrap_or("validated")
        ))
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
        ("INTEGRITY!", ui::danger())
    } else if row.blocks_writes() {
        ("Blocks writes", ui::warning())
    } else {
        // Terminal journal state is not a claim about live paths or rollback safety.
        ("Terminal", ui::muted())
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
        return Text::raw("No displayed transaction selected. Clear the filter.");
    };
    let (status, style) = state(row);
    let mut lines = vec![
        Line::styled(format!("[{status}]"), style),
        Line::styled("Transaction identity", ui::heading()),
        field("Repository ID", &row.repository_id),
        field("Transaction ID", &row.transaction_id),
        field(
            "Schema",
            row.journal_schema
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".into()),
        ),
        field("Phase", phase(row)),
        field("Validated checkpoints", row.valid_checkpoints),
        field(
            "Integrity",
            row.problem
                .as_deref()
                .unwrap_or("validated journal; preview checks live drift"),
        ),
        field("Blocks new writes", row.blocks_writes()),
        Line::default(),
        Line::styled("Actions | eligibility hints only", ui::heading()),
    ];
    if rollback(row) {
        lines.push(ui::keys(&[("b", "Preview rollback (schemas 1/2/4)")]));
    } else {
        lines.push(note(
            "Rollback unavailable for this diagnosis.",
            ui::muted(),
        ));
    }
    if resume(row) {
        lines.push(ui::keys(&[("s", "Preview bootstrap resume")]));
    } else if abandon(row) && row.phase == Some(Phase::Preparing) {
        lines.push(note("Resume unavailable: incomplete preparation; abandon, then create/attach a new transaction.", ui::warning()));
    } else {
        lines.push(note(
            "Resume unavailable: not an intact Running bootstrap.",
            ui::muted(),
        ));
    }
    if abandon(row) {
        lines.push(ui::keys(&[("x", "Preview bootstrap abandon")]));
        lines.push(note(
            "Abandon retains published repository, registration and private artifacts.",
            ui::warning(),
        ));
    } else {
        lines.push(note(
            "Abandon unavailable: not an intact nonterminal bootstrap.",
            ui::muted(),
        ));
    }
    lines.push(Line::default());
    lines.push(Line::styled("Guards & retained artifacts", ui::heading()));
    lines.push(note("Core preview independently checks live paths and artifacts. Each requested recovery action obtains a fresh, guarded preview.", Style::default()));
    lines.push(note("Adoption rollback (schema 1 or 4) withdraws new sources into private storage; lifecycle rollback (schema 2 or 4) leaves repository sources unchanged. Backups/private artifacts are retained; no garbage collection.", ui::warning()));
    Text::from(lines)
}

fn search(frame: &mut Frame, area: Rect, app: &App) {
    let editing = app.history_search.editing;
    let prefix = if editing { "Filter > " } else { "/ Filter " };
    // Keep the caret and escaped query tail visible without reserving three rows.
    let escaped = ui::inline(&app.history_search.query);
    let suffix = if app.data.as_ref().is_some_and(|d| d.history_error.is_none()) {
        format!(" | Showing {}", app.visible_history_rows().len())
    } else {
        String::new()
    };
    let available = usize::from(area.width).saturating_sub(prefix.len() + suffix.len() + 1);
    let shown = if escaped.len() > available && available >= 3 {
        format!("...{}", &escaped[escaped.len() - available + 3..])
    } else {
        escaped.chars().take(available).collect()
    };
    let caret = area.x + prefix.len() as u16 + shown.len() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix, ui::accent()),
            Span::raw(shown),
            Span::styled(
                if app.history_search.query.is_empty() && !editing {
                    "[display only]"
                } else {
                    ""
                },
                ui::muted(),
            ),
            Span::styled(suffix, ui::muted()),
        ])),
        area,
    );
    if editing && !app.busy && area.width > 0 {
        frame.set_cursor_position((caret.min(area.right().saturating_sub(1)), area.y));
    }
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
            Span::styled(format!("All repositories: {total}"), ui::heading()),
            Span::styled(
                format!(" | Blockers: {blockers}"),
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
    app.hits.click(rows[1], super::input::Target::Filter);
    let content = rows[2];
    let focused = true;
    let Some(data) = &app.data else {
        ui::report(
            frame,
            content,
            "Recovery | Not loaded",
            Text::raw("Recovery diagnosis not loaded. r: read all-repository history"),
            0,
            focused,
        );
        return;
    };
    if let Some(error) = &data.history_error {
        ui::report(
            frame,
            content,
            "Recovery diagnosis unavailable",
            Text::from(vec![
                note(error, ui::warning()),
                Line::raw("Global write safety is unavailable until diagnosis succeeds."),
                Line::raw("r: retry diagnosis"),
            ]),
            app.details_scroll,
            focused,
        );
        return;
    }
    let wide = super::workbench::wide_details(area, 2);
    if !wide && app.history_details {
        ui::report(
            frame,
            content,
            "Transaction details | Esc list",
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
        "Transactions | {} / {} shown",
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
                "No matches. Global blockers remain visible above."
            }),
            0,
        );
    } else {
        let table_rows = visible.iter().map(|&i| {
            let row = &data.history[i];
            let (status, style) = state(row);
            Row::new(vec![
                Cell::from(ui::inline(row.transaction_id.get(..8).unwrap_or("invalid"))),
                Cell::from(
                    row.journal_schema
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "?".into()),
                ),
                Cell::from(phase(row)),
                Cell::from(status).style(style),
            ])
        });
        let table = Table::new(
            table_rows,
            [
                Constraint::Min(8),
                Constraint::Length(3),
                Constraint::Length(12),
                Constraint::Length(13),
            ],
        )
        .header(Row::new(["TX ID", "SCM", "PHASE", "STATE"]).style(ui::heading()))
        .block(block)
        .row_highlight_style(ui::selected())
        .highlight_symbol("> ");
        let mut selection =
            TableState::default().with_selected(visible.iter().position(|&i| i == app.selected));
        frame.render_stateful_widget(table, panes[0], &mut selection);
        app.hits.rows(panes[0], &visible, selection.offset());
    }
    if wide {
        ui::report(
            frame,
            panes[1],
            "Transaction details",
            details(app),
            app.details_scroll,
            true,
        );
    }
}
