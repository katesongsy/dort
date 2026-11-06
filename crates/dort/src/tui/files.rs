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
    if app.inventory.group.is_some() {
        if let Some(node) = super::tree::visible(app)
            .into_iter()
            .find(|n| n.key == app.file_selection())
        {
            lines.push(Line::styled(
                "Directory / tree group · metadata only",
                ui::heading(),
            ));
            lines.push(ui::metadata(
                "Full path",
                node.path.to_str().unwrap_or("unavailable"),
            ));
            lines.push(Line::raw("Enter or h/l folds/unfolds. x marks/unmarks matching descendants, including folded rows; discovery directories/links are never adopted. Marks survive filters and folding."));
        }
    } else if app.discovering {
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
    let nodes = super::tree::visible(app);
    let marked = if app.discovering {
        app.inventory.candidate_marks.len()
    } else {
        app.inventory.managed_marks.len()
    };
    let shown_marks = nodes.iter().filter(|n| super::tree::marked(app, n)).count();
    let title = format!(
        "Files / {} · {} marked ({} hidden)",
        if app.discovering {
            "Discover"
        } else {
            "Managed"
        },
        marked,
        marked.saturating_sub(shown_marks)
    );
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
    if !wide && (app.inventory.details || (app.result.is_some() && app.receipt.is_none())) {
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
    let visible: Vec<_> = nodes.iter().map(|n| n.key).collect();
    let selected = app.file_selection();
    let list_area = split[0];
    let total = if app.discovering {
        app.discovery
            .as_ref()
            .map_or(0, |d| d.report.candidates.len())
    } else {
        app.data.as_ref().map_or(0, |d| d.files.len())
    };
    let title = format!(
        "{} / {} shown | x mark h/l fold",
        app.visible_file_rows().len(),
        total
    );
    let mut rows = Vec::new();
    for node in &nodes {
        let prefix = format!(
            "{}{} {}",
            "  ".repeat(node.depth.min(12)),
            if node.branch {
                let folded = if app.discovering {
                    &app.inventory.candidate_folded
                } else {
                    &app.inventory.managed_folded
                };
                if folded.contains(&node.path) {
                    "[>]"
                } else {
                    "[v]"
                }
            } else if super::tree::marked(app, node) {
                "[x]"
            } else {
                "[ ]"
            },
            node.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unavailable")
        );
        if node.branch {
            rows.push(Row::new(vec![
                Cell::from(ui::inline(&prefix)).style(ui::heading()),
                Cell::from("Group").style(ui::muted()),
                Cell::from("Directory"),
            ]));
        } else if let Some(i) = node.raw {
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
                    rows.push(Row::new(vec![
                        Cell::from(ui::inline(&prefix)),
                        Cell::from(state).style(ui::state(state)),
                        Cell::from(kind(&c.entry.kind)),
                    ]));
                }
            } else if let Some(data) = &app.data {
                if let Some(row) = data.managed.get(i) {
                    rows.push(Row::new(vec![
                        Cell::from(format!(
                            "{}{} {}",
                            "  ".repeat(node.depth.min(12)),
                            if super::tree::marked(app, node) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            ui::inline(&row.id)
                        )),
                        Cell::from(row.state).style(ui::state(row.state)),
                        Cell::from(ui::inline(
                            node.path
                                .file_name()
                                .and_then(|p| p.to_str())
                                .unwrap_or("unavailable"),
                        )),
                    ]));
                } else if let Some((id, _)) = data.files.get(i) {
                    rows.push(Row::new(vec![
                        Cell::from(format!(
                            "{} {}",
                            if super::tree::marked(app, node) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            ui::inline(id)
                        )),
                        Cell::from("Unknown"),
                        Cell::from(""),
                    ]));
                }
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
        // Register only actual rendered rows, after the generic row hit regions.
        let first = state.offset();
        let count = list_area.height.saturating_sub(3) as usize;
        for (line, node) in nodes.iter().skip(first).take(count).enumerate() {
            let x = list_area.x + 3 + (node.depth.min(12) * 2) as u16;
            if x + 3 < list_area.right() {
                app.hits.click(
                    Rect::new(x, list_area.y + 2 + line as u16, 3, 1),
                    if node.branch {
                        super::input::Target::TreeFold(node.key)
                    } else {
                        super::input::Target::TreeMark(node.key)
                    },
                );
            }
        }
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
