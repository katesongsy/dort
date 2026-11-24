//! Files workspace built from typed rows.
use super::input::{Target, Wheel};
use super::workspace::{Region, Scope, Workspace};
use super::{app::App, components as ui, views::safe};
use dort_core::observation::Kind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row},
};
use ui::collection::{Collection, Header, RowTargets};

#[cfg(test)]
thread_local! { pub(super) static FORMATTED_ROWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

fn draw_selected(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    if app.discovering && app.content.available() {
        let parts = Layout::vertical([Constraint::Min(3), Constraint::Length(5)]).split(area);
        super::discover_content::draw(frame, parts[0], &app.content, focused);
        app.hits.wheel(parts[0], super::input::Wheel::Content);
        app.hits
            .click(parts[0], super::input::Target::DiscoverFocus(true));
        ui::reader::Reader::new(&app.hits).report(
            frame,
            parts[1],
            "Details",
            details(app),
            app.details_scroll,
            false,
        );
    } else {
        ui::reader::Reader::new(&app.hits).report(
            frame,
            area,
            "Selected item",
            details(app),
            app.details_scroll,
            focused,
        );
    }
}

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
        if let Some(node) = super::tree::selected(app) {
            lines.push(Line::styled("Folder · header only", ui::heading()));
            lines.push(ui::metadata(
                "Full path",
                node.path.to_str().unwrap_or("unavailable"),
            ));
            lines.push(Line::raw(
                "Marking a folder marks its eligible files. Marks stay through filters and folds.",
            ));
        }
    } else if app.discovering {
        if let Some(candidate) = app
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(app.candidate_selected))
            .filter(|_| super::discovery_tree::contains(app, app.candidate_selected))
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
                "Review this candidate before adoption."
            } else {
                "Adoption requires an eligible regular file."
            }));
            lines.push(Line::raw(if eligible { "Choose Adopt to edit target, ID and source." } else { "Only owned, single-link regular files without special permission bits are candidates." }));
            lines.push(Line::raw(
                "The preview checks paths, sensitivity and remaining limits.",
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
            lines.push(Line::raw("Select a file, or clear the filter."));
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
                "Keep writes a regular file. Both actions keep the source.",
            ));
        } else if let Some((_, detail)) = data
            .files
            .get(app.selected)
            .filter(|_| app.visible_file_rows().contains(&app.selected))
        {
            lines.extend(Text::from(safe(detail)).lines);
        } else {
            lines.push(Line::raw("Select a file, or clear the filter."));
        }
    }
    lines.push(Line::styled(
        "Search changes the display only. Apply covers every active file.",
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
    let shown_marks = if marked == 0 {
        0
    } else if app.discovering {
        super::discovery_tree::shown_marks(app)
    } else {
        nodes.iter().filter(|n| super::tree::marked(app, n)).count()
    };
    let title = format!(
        "{} marked ({} hidden)",
        marked,
        marked.saturating_sub(shown_marks)
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                app.inventory
                    .mark_refusal
                    .as_ref()
                    .map(|r| format!(" {r} "))
                    .unwrap_or_else(|| format!(" {title}  ")),
                if app.inventory.mark_refusal.is_some() {
                    ui::danger()
                } else {
                    ui::heading()
                },
            ),
            Span::styled(
                if app.discovering {
                    "Selected-file preview"
                } else {
                    "Full active-profile scope"
                },
                ui::muted(),
            ),
        ])),
        sections[0],
    );
    let search = app.inventory.search(app.discovering);
    ui::design::filter(
        frame,
        sections[1],
        "Filter files",
        &search.query,
        search.editing && app.hits.input_focus_allowed() && !app.busy,
    );
    if search.editing {
        app.presented(
            "Done",
            "Enter",
            super::input::Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        );
    } else if !nodes.is_empty() && !app.inventory.details && !app.content.focused {
        app.presented(
            "Details",
            "Enter",
            super::input::Target::Action(super::shell::Action::Inspect),
        );
    }
    if app.inventory.details || app.content.focused {
        app.presented("Back", "Esc", super::input::Target::Back);
    }
    app.hits.click(sections[1], super::input::Target::Filter);
    let content = sections[2];
    let active = if !search.editing && (app.inventory.details || app.content.focused) {
        Region::Details
    } else {
        Region::Queue
    };
    let scope = Scope::Files {
        discovering: app.discovering,
    };
    let workspace = Workspace::new(scope, content, active, !nodes.is_empty());
    if !workspace.is_split() && app.result.is_some() && app.receipt.is_none() && !search.editing {
        draw_selected(frame, content, app, true);
        return;
    }
    workspace.bind(&app.hits);
    if let Some(area) = workspace.details {
        draw_selected(frame, area, app, active == Region::Details);
    }
    let Some(list_area) = workspace.queue else {
        return;
    };
    let focused = active == Region::Queue;
    let selected = super::tree::position(app, app.file_selection());
    let total = if app.discovering {
        app.discovery
            .as_ref()
            .map_or(0, |d| d.report.candidates.len())
    } else {
        app.data.as_ref().map_or(0, |d| d.files.len())
    };
    let title = format!(
        "{} / {} shown",
        if app.discovering {
            super::discovery_tree::snapshot(app).matching.len()
        } else {
            app.visible_file_rows().len()
        },
        total
    );
    let (adoption_width, kind_width) = if list_area.width < 48 {
        (11, 4)
    } else {
        (15, 10)
    };
    let name_width = if app.discovering {
        list_area
            .width
            .saturating_sub(adoption_width + kind_width + 4)
    } else {
        list_area.width.saturating_mul(32) / 100
    };
    let indent_limit = usize::from(name_width.saturating_sub(15) / 2).min(12);
    let collection = if app.discovering {
        Collection::new(
            [
                Constraint::Min(18),
                Constraint::Length(adoption_width),
                Constraint::Length(kind_width),
            ],
            Header::Heading(["NAME", "ADOPTION", "TYPE"]),
        )
    } else {
        Collection::new(
            [
                Constraint::Percentage(32),
                Constraint::Length(13),
                Constraint::Min(4),
            ],
            Header::Heading(["FILE", "STATE", "DESTINATION"]),
        )
    }
    .titled(&title, focused);
    let window = collection.window(
        list_area,
        app.inventory.viewport(app.discovering),
        nodes.len(),
        selected,
    );
    let mut rows = Vec::new();
    let mut visible = Vec::new();
    for node in nodes.window(window.range.start, window.range.len()) {
        let previous = rows.len();
        #[cfg(test)]
        FORMATTED_ROWS.with(|count| count.set(count.get() + 1));
        let prefix = format!(
            "{}{} {}",
            "  ".repeat(node.depth.min(indent_limit)),
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
                            "  ".repeat(node.depth.min(indent_limit)),
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
        if rows.len() > previous {
            visible.push(node);
        }
    }
    if nodes.is_empty() {
        let message = if !app.discovering {
            app.data
                .as_ref()
                .and_then(|d| d.files_error.as_deref())
                .unwrap_or(if total == 0 {
                    "No active files. Discover a regular file to start."
                } else {
                    "No matches. Delete clears the display filter."
                })
        } else if app.discovery.is_none() {
            "Discovery not loaded. Use Refresh."
        } else if super::discovery_tree::pending(app) {
            "Updating list... Search, Help and Quit remain available."
        } else if total == 0 {
            "No candidates found."
        } else {
            "No matches. Delete clears the display filter."
        };
        frame.render_widget(
            Paragraph::new(safe(message)).block(ui::pane(&title, focused)),
            list_area,
        );
    } else {
        let rendered = collection.draw(
            frame,
            list_area,
            visible.iter().map(|node| node.key).zip(rows),
            window.selection,
            RowTargets {
                hits: &app.hits,
                wheel: Some(Wheel::Rows),
                target: Target::Row,
            },
        );
        for ((row, _), node) in rendered
            .rows()
            .zip(visible.into_iter().skip(rendered.offset))
        {
            let x = row.x + 2 + (node.depth.min(indent_limit) * 2) as u16;
            if x + 3 < row.right() {
                app.hits.click(
                    Rect::new(x, row.y, 3, 1),
                    if node.branch {
                        Target::TreeFold(node.key)
                    } else {
                        Target::TreeMark(node.key)
                    },
                );
            }
        }
    }
}
