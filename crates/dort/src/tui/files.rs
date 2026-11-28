//! Files workspace built from typed rows.
use super::input::{Target, Wheel};
use super::workspace::{Region, Scope, Workspace};
use super::{app::App, components as ui, views::safe};
use super::{inventory::Page, shell::Action};
use dort_core::{
    model::{FileMode, ManagedFile},
    observation::Kind,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row},
};
use ui::collection::{Collection, Header, RowTargets};
use ui::design::Tone;
use ui::reader::{field, section};

#[cfg(test)]
thread_local! { pub(super) static FORMATTED_ROWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

fn draw_selected(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    if app.content.available() {
        let parts =
            Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);
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

fn kind_short(kind: &Kind) -> &'static str {
    match kind {
        Kind::Regular => "File",
        Kind::Directory => "Dir",
        Kind::Symlink => "Link",
        Kind::Missing => "Gone",
        Kind::Special => "Spec",
    }
}
fn kind_style(kind: &Kind) -> ratatui::style::Style {
    match kind {
        Kind::Directory => ui::note(),
        Kind::Symlink => ui::accent(),
        Kind::Missing | Kind::Special => ui::warning(),
        Kind::Regular => ratatui::style::Style::default(),
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
    if app.inventory.group.is_some() {
        if let Some(node) = super::tree::selected(app) {
            lines.push(Line::styled("Folder · header only", ui::heading()));
            lines.push(field(
                "Full path",
                node.path.to_str().unwrap_or("unavailable"),
                Tone::Neutral,
            ));
            lines.push(Line::raw(
                "Marking a folder marks its eligible files. Marks stay through filters and folds.",
            ));
        }
    } else if app.discovering() {
        if let Some(candidate) = app
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(app.candidate_selected))
            .filter(|_| super::discovery_tree::contains(app, app.candidate_selected))
        {
            let eligible = candidate.supported_for_adoption;
            let managed = app.managed_destination(&candidate.entry.path);
            lines.push(section(kind(&candidate.entry.kind)));
            lines.push(field(
                "Path",
                candidate.entry.path.to_str().unwrap_or("unavailable"),
                Tone::Neutral,
            ));
            let (status, tone) = if managed.is_some() {
                ("Managed", Tone::Success)
            } else if eligible {
                ("Eligible", Tone::Success)
            } else {
                ("Ineligible", Tone::Attention)
            };
            lines.push(field("Status", status, tone));
            if let Some(row) = managed {
                lines.push(field("Managed", &row.id, Tone::Neutral));
                lines.push(Line::raw(if row.mode == FileMode::Template {
                    "Open Files / Generated for this destination."
                } else {
                    "Change the existing entry in Managed."
                }));
            } else {
                lines.push(Line::raw(if eligible {
                    "Choose Adopt to edit target, ID and source."
                } else {
                    "Adoption needs an owned, single-link regular file without special permission bits."
                }));
            }
            if let Some(d) = &app.discovery {
                for warning in &d.report.warnings {
                    lines.push(Line::styled(safe(warning), Tone::Attention.style()));
                }
                lines.push(field(
                    "Protected root",
                    &format!("{:?}", d.repository),
                    Tone::Neutral,
                ));
            }
        } else {
            lines.push(Line::raw("Select a file, or clear the filter."));
        }
    } else if app.files_page == Page::Generated {
        if let Some(file) = app.selected_mapping()
            && let Some(row) = app
                .data
                .as_ref()
                .and_then(|data| data.managed.get(app.file_selection()))
        {
            lines.extend([
                section(&file.id),
                ui::metadata("Destination", row.target.to_str().unwrap_or("unavailable")),
                ui::metadata(
                    "Template",
                    row.source
                        .as_ref()
                        .and_then(|path| path.to_str())
                        .unwrap_or("In dort.toml"),
                ),
                ui::metadata("Profiles", &file.profiles.join(", ")),
                Line::default(),
                Line::raw("Preview for current output and destination status."),
                Line::raw(if file.template.is_some() {
                    "Edit to change the inline template or its inputs."
                } else {
                    "Edit the template source in the repository."
                }),
            ]);
        }
    } else if let Some(data) = &app.data {
        if let Some(error) = &data.files_error {
            lines.extend(Text::from(safe(error)).lines);
        } else if let Some(row) = data
            .managed
            .get(app.selected)
            .filter(|_| app.visible_file_rows().contains(&app.selected))
        {
            lines.push(section(&row.id));
            lines.push(field(
                "Status",
                row.state,
                match row.state {
                    "Linked" => Tone::Success,
                    "Needs apply" => Tone::Attention,
                    "Blocked" => Tone::Error,
                    _ => Tone::Neutral,
                },
            ));
            for reason in &row.blockers {
                lines.push(field("Reason", reason, Tone::Error));
            }
            lines.push(ui::metadata(
                "Source",
                row.source
                    .as_ref()
                    .and_then(|p| p.to_str())
                    .unwrap_or("unavailable"),
            ));
            lines.push(ui::metadata(
                "Destination",
                row.target.to_str().unwrap_or("unavailable"),
            ));
            lines.push(Line::raw(
                "Keep writes a regular file. Both actions keep the source.",
            ));
            lines.push(Line::default());
            lines.push(section("File types"));
            lines.push(field("Source kind", row.source_kind, Tone::Neutral));
            lines.push(field("Dest. kind", row.target_kind, Tone::Neutral));
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
    if let Some(result) = &app.result {
        lines.push(Line::default());
        lines.push(section("Result"));
        lines.extend(Text::from(safe(result)).lines);
    }
    Text::from(lines)
}

impl App {
    pub(super) fn managed_destination(
        &self,
        path: &std::path::Path,
    ) -> Option<&super::actions::ManagedRow> {
        self.data
            .as_ref()?
            .managed
            .iter()
            .find(|row| row.target == path)
    }

    fn mapping_at(&self, index: usize) -> Option<&ManagedFile> {
        let data = self.data.as_ref()?;
        let (id, _) = data.files.get(index)?;
        data.file_declarations.iter().find(|file| &file.id == id)
    }

    pub(super) fn selected_mapping(&self) -> Option<&ManagedFile> {
        if self.tab != super::app::Tab::Files || self.discovering() {
            return None;
        }
        self.mapping_at(super::tree::selected(self)?.raw?)
    }

    pub(super) fn template_selected(&self) -> bool {
        self.files_page == Page::Generated && self.selected_mapping().is_some()
    }
}

pub(super) fn controls(app: &App) -> impl Iterator<Item = ui::ActionButton> {
    use super::configuration::Kind;
    let mapping = app.selected_mapping();
    let environment = (
        Action::EnvironmentPlan.label(),
        "w",
        Action::EnvironmentPlan,
        app.registered_repository() && mapping.is_some_and(|file| !file.requires.is_empty()),
    );
    let add = ("Add file mapping", "n", Action::Configure(Kind::File), true);
    let edit = (
        "Edit file mapping",
        "e",
        Action::Configure(Kind::EditFile),
        mapping.is_some(),
    );
    let convert = (
        Action::ConvertGenerated.label(),
        "c",
        Action::ConvertGenerated,
        mapping.is_some_and(|file| file.mode == FileMode::Link),
    );
    let managed = if app.inventory.details {
        [environment, edit, add, convert]
    } else {
        [environment, add, edit, convert]
    };
    let generated = [
        (
            Action::LocalInputs.label(),
            "l",
            Action::LocalInputs,
            app.action_reason(Action::LocalInputs).is_none(),
        ),
        (
            Action::NewGenerated.label(),
            "s",
            Action::NewGenerated,
            app.action_reason(Action::NewGenerated).is_none(),
        ),
        (
            Action::PreviewGenerated.label(),
            "v",
            Action::PreviewGenerated,
            mapping.is_some(),
        ),
        (
            Action::EditGenerated.label(),
            "e",
            Action::EditGenerated,
            app.registered_repository()
                && mapping.is_some_and(|file| file.template.is_some() || file.source.is_some()),
        ),
    ];
    match app.files_page {
        Page::Managed => &managed[..],
        Page::Generated => &generated[..],
        Page::Templates => &[],
        Page::Discover => &[],
    }
    .iter()
    .copied()
    .filter(|&(_, _, _, visible)| visible)
    .map(|(label, key, action, _)| ui::ActionButton {
        label,
        key,
        target: Target::Action(action),
    })
    .collect::<Vec<_>>()
    .into_iter()
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    for control in controls(app) {
        app.hits.present(control);
    }
    let generated = app.files_page == Page::Generated;
    let sections = Layout::vertical([
        Constraint::Length(u16::from(!generated)),
        Constraint::Length(1),
        Constraint::Min(3),
    ])
    .split(area);
    let nodes = super::tree::visible(app);
    if !generated {
        let marked = if app.discovering() {
            app.inventory.candidate_marks.len()
        } else {
            app.inventory.managed_marks.len()
        };
        let shown_marks = if marked == 0 {
            0
        } else if app.discovering() {
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
                    if app.discovering() {
                        "Selected-file preview"
                    } else {
                        "Full active-profile scope"
                    },
                    ui::muted(),
                ),
            ])),
            sections[0],
        );
    }
    let search = app.inventory.search(app.files_page);
    ui::design::filter(
        frame,
        sections[1],
        if generated {
            "Filter generated files"
        } else {
            "Filter files"
        },
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
    } else if app.template_selected() {
        app.presented(
            Action::PreviewGenerated.label(),
            "Enter",
            Target::Action(Action::PreviewGenerated),
        );
        if !app.inventory.details {
            app.presented("Details", "F3", Target::Action(Action::Inspect));
        }
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
    let scope = Scope::Files(app.files_page);
    let workspace = Workspace::new(scope, content, active, !nodes.is_empty());
    if !generated
        && !workspace.is_split()
        && app.result.is_some()
        && app.receipt.is_none()
        && !search.editing
    {
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
    let total = if app.discovering() {
        app.discovery
            .as_ref()
            .map_or(0, |d| d.report.candidates.len())
    } else {
        app.data.as_ref().map_or(0, |d| {
            (0..d.files.len())
                .filter(|i| d.is_template(*i) == generated)
                .count()
        })
    };
    let title = format!(
        "{} / {} shown",
        if app.discovering() {
            super::discovery_tree::snapshot(app).matching.len()
        } else {
            app.visible_file_rows().len()
        },
        total
    );
    let (adoption_width, kind_width) = if list_area.width < 60 {
        (12, 4)
    } else {
        (16, 9)
    };
    let name_width = if app.discovering() {
        list_area
            .width
            .saturating_sub(adoption_width + kind_width + 4)
    } else {
        list_area.width.saturating_mul(32) / 100
    };
    let indent_limit = usize::from(name_width.saturating_sub(15) / 2).min(12);
    let collection = if app.discovering() {
        Collection::new(
            [
                Constraint::Min(18),
                Constraint::Length(adoption_width),
                Constraint::Length(kind_width),
            ],
            Header::Heading(["NAME", "ADOPTION", "TYPE"]),
        )
    } else if generated {
        Collection::new(
            [
                Constraint::Percentage(32),
                Constraint::Length(13),
                Constraint::Min(4),
            ],
            Header::Heading(["FILE", "TEMPLATE", "DESTINATION"]),
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
        app.inventory.viewport(app.files_page),
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
                let folded = if app.discovering() {
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
                Cell::from(""),
                Cell::from("Dir").style(kind_style(&Kind::Directory)),
            ]));
        } else if let Some(i) = node.raw {
            if app.discovering() {
                if let Some(c) = app
                    .discovery
                    .as_ref()
                    .and_then(|d| d.report.candidates.get(i))
                {
                    let managed = app.managed_destination(&c.entry.path);
                    let state = if managed.is_some() {
                        "Managed"
                    } else if c.supported_for_adoption {
                        "Eligible"
                    } else {
                        "Ineligible"
                    };
                    rows.push(Row::new(vec![
                        Cell::from(ui::inline(&prefix)),
                        Cell::from(state).style(ui::state(state)),
                        Cell::from(kind_short(&c.entry.kind)).style(kind_style(&c.entry.kind)),
                    ]));
                }
            } else if let Some(data) = &app.data {
                if let Some(row) = data.managed.get(i) {
                    rows.push(Row::new(vec![
                        Cell::from(if generated {
                            ui::inline(&row.id)
                        } else {
                            format!(
                                "{}{} {}",
                                "  ".repeat(node.depth.min(indent_limit)),
                                if super::tree::marked(app, node) {
                                    "[x]"
                                } else {
                                    "[ ]"
                                },
                                ui::inline(&row.id)
                            )
                        }),
                        if generated {
                            Cell::from(if row.source.is_some() {
                                "Template file"
                            } else {
                                "In dort.toml"
                            })
                        } else {
                            Cell::from(row.state).style(ui::state(row.state))
                        },
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
        let message = if generated {
            if total != 0 {
                "No matches. Delete clears the display filter."
            } else if app.action_reason(Action::NewGenerated).is_none() {
                "No generated files. Press s for New generated file."
            } else if !app.registered_repository() {
                "Choose a registered repository to create a generated file."
            } else {
                "Open Checks to fix the manifest before creating a generated file."
            }
        } else if !app.discovering() {
            app.data
                .as_ref()
                .and_then(|d| d.files_error.as_deref())
                .unwrap_or(if total == 0 {
                    "No mappings. Add a mapping or discover a regular file."
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
            Paragraph::new(safe(message))
                .wrap(ratatui::widgets::Wrap { trim: true })
                .block(ui::pane(&title, focused)),
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
            if !generated && x + 3 < row.right() {
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
