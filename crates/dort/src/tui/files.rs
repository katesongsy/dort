//! Structured Files workspace; display identity is never used to construct a request.
use super::{app::App, components as ui, views::safe};
use dort_core::observation::Kind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};

fn kind(kind: &Kind) -> &'static str {
    match kind {
        Kind::Regular => "File",
        Kind::Directory => "Directory",
        Kind::Symlink => "Link",
        Kind::Missing => "Missing",
        Kind::Special => "Special",
    }
}
fn details(app: &App) -> Text<'static> {
    let mut lines = Vec::new();
    if let Some(result) = &app.result {
        lines.push(Line::styled("Result", ui::heading()));
        lines.extend(Text::from(safe(result)).lines);
        lines.push(Line::raw(""));
    }
    if app.discovering {
        if let Some(candidate) = app
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(app.candidate_selected))
            .filter(|_| app.visible_file_rows().contains(&app.candidate_selected))
        {
            let eligible = candidate.supported_for_adoption;
            let state = if eligible {
                "Eligible hint"
            } else {
                "Not eligible"
            };
            lines.push(Line::styled(
                format!("[{state}] {}", kind(&candidate.entry.kind)),
                ui::state(state),
            ));
            lines.push(ui::metadata(
                "Path",
                candidate.entry.path.to_str().unwrap_or("unavailable"),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::raw(if eligible {
                "Metadata eligibility is a hint, not approval."
            } else {
                "Directories, links and special files cannot be adopted."
            }));
            lines.push(Line::raw(if eligible { "Choose Adopt selected to edit target, ID and source." } else { "Only owned, single-link regular files without special permission bits are candidates." }));
            lines.push(Line::raw(
                "Core preview checks paths, sensitivity and remaining guards.",
            ));
            if let Some(d) = &app.discovery {
                lines.push(Line::raw(""));
                lines.push(ui::metadata(
                    "Protected root",
                    &format!("{:?}", d.repository),
                ));
                lines.push(Line::raw(safe(d.report.sensitivity_notice)));
                for warning in &d.report.warnings {
                    lines.push(Line::styled(safe(warning), ui::warning()));
                }
            }
        } else {
            lines.push(Line::raw(
                "No displayed selection. Clear the filter or refresh discovery.",
            ));
        }
    } else if let Some(data) = &app.data {
        if let Some(error) = &data.files_error {
            lines.extend(Text::from(safe(error)).lines);
        } else if let Some(row) = data
            .managed
            .get(app.selected)
            .filter(|_| app.visible_file_rows().contains(&app.selected))
        {
            lines.push(Line::styled(
                format!("{}  [{}]", safe(&row.id), row.state),
                ui::state(row.state),
            ));
            lines.push(ui::metadata(
                "Source",
                row.source.to_str().unwrap_or("unavailable"),
            ));
            lines.push(ui::metadata(
                "Destination",
                row.target.to_str().unwrap_or("unavailable"),
            ));
            lines.push(ui::metadata("Source kind", row.source_kind));
            lines.push(ui::metadata("Dest. kind", row.target_kind));
            for reason in &row.blockers {
                lines.push(Line::styled(safe(reason), ui::danger()));
            }
            lines.push(Line::raw(""));
            lines.push(Line::raw(
                "Keep materializes an ordinary file; both modes retain the source.",
            ));
        } else if let Some((_, detail)) = data
            .files
            .get(app.selected)
            .filter(|_| app.visible_file_rows().contains(&app.selected))
        {
            lines.extend(Text::from(safe(detail)).lines);
        } else {
            lines.push(Line::raw(
                "No displayed selection. Clear the filter or discover a file.",
            ));
        }
    }
    lines.push(Line::styled(
        "Search changes display only. Apply covers the full active scope.",
        ui::muted(),
    ));
    Text::from(lines)
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
    ])
    .split(area);
    let title = if app.discovering {
        "Files / Discover"
    } else {
        "Files / Managed"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {title}  "), ui::heading()),
            Span::styled(
                if app.discovering {
                    "Metadata only"
                } else {
                    "Full active-profile scope"
                },
                ui::muted(),
            ),
        ])),
        sections[0],
    );
    let search = app.inventory.search(app.discovering);
    let query = safe(&search.query);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if search.editing {
                    " Search > "
                } else {
                    " / Search "
                },
                ui::accent(),
            ),
            Span::raw(if query.is_empty() {
                "[type / to filter]".into()
            } else {
                query.clone()
            }),
            Span::styled("  · Display only", ui::muted()),
        ])),
        sections[1],
    );
    if search.editing && !app.busy {
        let x =
            (sections[1].x + 10 + query.len() as u16).min(sections[1].right().saturating_sub(1));
        frame.set_cursor_position((x, sections[1].y));
    }
    app.hits.click(sections[1], super::input::Target::Filter);
    let content = sections[2];
    let wide = super::workbench::wide_details(area, 2);
    let split = if wide {
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).split(content)
    } else {
        Layout::vertical([Constraint::Percentage(100)]).split(content)
    };
    let focused = true;
    if !wide && (app.inventory.details || app.result.is_some()) {
        ui::report(
            frame,
            content,
            "Selected item · Esc list",
            details(app),
            app.details_scroll,
            focused,
        );
        return;
    }
    let visible = app.visible_file_rows();
    let selected = app.file_selection();
    let list_area = split[0];
    let total = if app.discovering {
        app.discovery
            .as_ref()
            .map_or(0, |d| d.report.candidates.len())
    } else {
        app.data.as_ref().map_or(0, |d| d.files.len())
    };
    let title = format!("{} / {} shown", visible.len(), total);
    let mut rows = Vec::new();
    for &i in &visible {
        if app.discovering {
            if let Some(c) = app
                .discovery
                .as_ref()
                .and_then(|d| d.report.candidates.get(i))
            {
                let state = if c.supported_for_adoption {
                    "Eligible hint"
                } else {
                    "Not eligible"
                };
                let name = c
                    .entry
                    .path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unavailable");
                rows.push(Row::new(vec![
                    Cell::from(ui::inline(name)),
                    Cell::from(state).style(ui::state(state)),
                    Cell::from(kind(&c.entry.kind)),
                ]));
            }
        } else if let Some(data) = &app.data {
            if let Some(row) = data.managed.get(i) {
                rows.push(Row::new(vec![
                    Cell::from(ui::inline(&row.id)),
                    Cell::from(row.state).style(ui::state(row.state)),
                    Cell::from(safe(row.target.to_str().unwrap_or("unavailable"))),
                ]));
            } else if let Some((id, _)) = data.files.get(i) {
                rows.push(Row::new(vec![
                    Cell::from(safe(id)),
                    Cell::from("Unknown"),
                    Cell::from(""),
                ]));
            }
        }
    }
    let block = ui::pane(&title, focused);
    if rows.is_empty() {
        let message = if !app.discovering {
            app.data
                .as_ref()
                .and_then(|d| d.files_error.as_deref())
                .unwrap_or(if total == 0 {
                    "0 active mapping(s). Discover a regular file to get started."
                } else {
                    "No matches. Delete clears the display filter."
                })
        } else if app.discovery.is_none() {
            "Discovery not loaded. Use Refresh."
        } else if total == 0 {
            "No metadata candidates found in this bounded scan."
        } else {
            "No matches. Delete clears the display filter."
        };
        frame.render_widget(Paragraph::new(safe(message)).block(block), list_area);
    } else {
        let widths = if app.discovering {
            vec![
                Constraint::Percentage(46),
                Constraint::Length(15),
                Constraint::Min(6),
            ]
        } else {
            vec![
                Constraint::Percentage(32),
                Constraint::Length(13),
                Constraint::Min(4),
            ]
        };
        let header = Row::new(if app.discovering {
            vec!["NAME", "ADOPTION", "TYPE"]
        } else {
            vec!["FILE", "STATE", "DESTINATION"]
        })
        .style(ui::heading());
        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(ui::selected())
            .highlight_symbol("> ");
        let mut state =
            TableState::default().with_selected(visible.iter().position(|&i| i == selected));
        frame.render_stateful_widget(table, list_area, &mut state);
        app.hits.rows(list_area, &visible, state.offset());
    }
    if wide {
        ui::report(
            frame,
            split[1],
            "Selected item | PgUp/PgDn details",
            details(app),
            app.details_scroll,
            true,
        );
    }
}
