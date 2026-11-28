//! Content-free history presentation. Recovery previews make the final decision.
use super::input::{Target, Wheel};
use super::workspace::{Region, Scope, Workspace};
use super::{app::App, components as ui};
use dort_core::journal::{Diagnosis, Phase};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row},
};
use ui::collection::{Collection, Header, RowTargets};
use ui::design::Tone;
use ui::reader::{field, section};

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
        _ => "Unknown operation",
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
fn state(row: &Diagnosis) -> (&'static str, Tone) {
    if row.problem.is_some() {
        ("Damaged", Tone::Error)
    } else if row.blocks_writes() {
        ("Blocks writes", Tone::Error)
    } else {
        ("Finished", Tone::Success)
    }
}
fn details(app: &App) -> Text<'static> {
    let Some(row) = app.history_selection() else {
        return Text::raw("No operation record selected. Clear the filter.");
    };
    let (status, tone) = state(row);
    let mut lines = vec![
        section(kind(row)),
        field("Status", &format!("[{status}]"), tone),
        field("State", phase(row), Tone::Neutral),
        Line::default(),
        section("Recovery options"),
    ];
    use super::shell::Action;
    let options = [
        (rollback(row), "Rollback", "b", Action::Rollback),
        (resume(row), "Resume", "s", Action::Resume),
        (abandon(row), "Abandon", "x", Action::Abandon),
    ];
    for (_, label, key, action) in options.into_iter().filter(|option| option.0) {
        app.presented(label, key, Target::Action(action));
        lines.push(field(label, "Available for review", Tone::Neutral));
    }
    if row.problem.is_some() {
        lines.push(field(
            "Blocked",
            "Recovery record could not be verified.",
            Tone::Error,
        ));
    } else if abandon(row) && row.phase == Some(Phase::Preparing) {
        lines.push(field(
            "Resume blocked",
            "Repository setup is incomplete.",
            Tone::Error,
        ));
        lines.push(field(
            "Next",
            "Abandon, then create or attach again.",
            Tone::Neutral,
        ));
    } else if !options.iter().any(|option| option.0) {
        lines.push(Line::raw("No recovery action available."));
    }
    if abandon(row) {
        lines.push(Line::styled(
            "Abandon keeps the published repository, registration and private files.",
            Tone::Attention.style(),
        ));
    }
    lines.extend([
        Line::default(),
        section("What recovery keeps"),
        Line::raw("Recovery checks current paths and files before each action."),
        Line::styled(
            "Rollback restores files only when they still match.",
            Tone::Attention.style(),
        ),
        Line::styled(
            "Tool declarations restore only an unchanged manifest.",
            Tone::Attention.style(),
        ),
        Line::styled(
            "Packages are not removed. Private backups stay.",
            Tone::Attention.style(),
        ),
        Line::default(),
        section("Operation record"),
        field("Repository ID", &row.repository_id, Tone::Neutral),
        field("Operation ID", &row.transaction_id, Tone::Neutral),
        field(
            "Integrity",
            row.problem.as_deref().unwrap_or("Valid"),
            if row.problem.is_some() {
                Tone::Error
            } else {
                Tone::Neutral
            },
        ),
        field(
            "Saved steps",
            &row.valid_checkpoints.to_string(),
            Tone::Neutral,
        ),
    ]);
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
    let generated = app
        .data
        .as_ref()
        .and_then(|data| data.generated_recovery.as_ref());
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(generated.is_some())),
        Constraint::Length(1),
        Constraint::Min(3),
    ])
    .split(area);
    if let Some(pending) = generated {
        let notice = if pending.is_ok() {
            if !app.history_search.editing {
                use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                app.presented(
                    "Review generated recovery",
                    "u",
                    Target::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
                );
            }
            "Generated update needs recovery."
        } else {
            "Generated recovery unavailable. r retries."
        };
        frame.render_widget(Paragraph::new(Line::styled(notice, ui::danger())), rows[1]);
    }
    if !app.history_search.editing
        && app.history_region != Region::Details
        && app
            .data
            .as_ref()
            .and_then(|d| d.overview.as_ref())
            .and_then(|o| o.registered)
            == Some(true)
    {
        app.presented(
            "Environment run",
            "e",
            super::input::Target::Action(super::shell::Action::EnvironmentRecord),
        );
    }
    let available = app.data.as_ref().filter(|d| d.history_error.is_none());
    let (total, blockers) = available
        .map(|d| {
            (
                d.history.len().to_string(),
                if generated.is_some_and(Result::is_err) {
                    "Unknown".into()
                } else {
                    (d.history.iter().filter(|r| r.blocks_writes()).count()
                        + usize::from(generated.is_some()))
                    .to_string()
                },
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
                if generated.is_none()
                    && available.is_some_and(|d| !d.history.iter().any(|r| r.blocks_writes()))
                {
                    ui::muted()
                } else {
                    ui::danger()
                },
            ),
        ])),
        rows[0],
    );
    search(frame, rows[2], app);
    if app.history_search.editing {
        app.presented(
            "Done",
            "Enter",
            super::input::Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        );
    } else if app.history_region == Region::Details {
        app.presented("Back", "Esc", super::input::Target::Back);
    } else if available.is_some() && !app.visible_history_rows().is_empty() {
        app.presented(
            "Details",
            "Enter",
            super::input::Target::Action(super::shell::Action::Inspect),
        );
    }
    app.hits.click(rows[2], super::input::Target::Filter);
    let content = rows[3];
    let Some(data) = &app.data else {
        ui::reader::Reader::new(&app.hits).report(
            frame,
            content,
            "Recovery | Not loaded",
            Text::raw("Recovery has not loaded yet."),
            0,
            true,
        );
        return;
    };
    if let Some(error) = &data.history_error {
        ui::reader::Reader::new(&app.hits).report(
            frame,
            content,
            "Recovery unavailable",
            Text::from(vec![
                field(
                    "Blocked",
                    "Writes stay blocked until recovery loads.",
                    Tone::Error,
                ),
                field("Next", "Refresh Recovery.", Tone::Neutral),
                field("History error", error, Tone::Attention),
            ]),
            app.details_scroll,
            true,
        );
        return;
    }
    let visible = app.visible_history_rows();
    let active = if app.history_search.editing {
        Region::Queue
    } else {
        app.history_region
    };
    let workspace = Workspace::new(Scope::Recovery, content, active, !visible.is_empty());
    workspace.bind(&app.hits);
    if let Some(pane) = workspace.details {
        ui::reader::Reader::new(&app.hits).report(
            frame,
            pane,
            "Operation details",
            details(app),
            app.details_scroll,
            active == Region::Details,
        );
    }
    let Some(list) = workspace.queue else { return };
    let title = "Operations";
    let focused = active == Region::Queue;
    if data.history.is_empty() || visible.is_empty() {
        let block = ui::pane(title, focused);
        let inner = block.inner(list);
        frame.render_widget(block, list);
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
        let collection = Collection::new(
            [
                Constraint::Min(8),
                Constraint::Length(if list.width < 44 { 4 } else { 12 }),
                Constraint::Length(9),
                Constraint::Length(if list.width < 44 { 7 } else { 12 }),
            ],
            Header::Heading(["OP ID", "TYPE", "STATE", "STATUS"]),
        )
        .titled(title, focused);
        let selected = visible.iter().position(|&i| i == app.selected);
        let window = collection.window(list, &app.history_viewport, visible.len(), selected);
        let rows = visible[window.range].iter().map(|&i| {
            let row = &data.history[i];
            let (status, tone) = state(row);
            (
                i,
                Row::new([
                    Cell::from(ui::inline(row.transaction_id.get(..8).unwrap_or("invalid"))),
                    Cell::from(kind(row)),
                    Cell::from(phase(row)),
                    Cell::from(status).style(tone.style()),
                ]),
            )
        });
        collection.draw(
            frame,
            list,
            rows,
            window.selection,
            RowTargets {
                hits: &app.hits,
                wheel: Some(Wheel::Rows),
                target: Target::Row,
            },
        );
    }
}
