use super::{Comparison, Metadata, Reader, preview};
use crate::tui::input::{Target, Wheel};
use crate::tui::workspace::{Region, Scope, Workspace};
use crate::tui::{components as ui, inventory::Search};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row},
};
use std::path::Path;
use ui::collection::{Collection, Header, RowTargets};

pub(super) struct View<'a> {
    pub hits: &'a crate::tui::input::Hits,
    pub root: Option<&'a Path>,
    pub follows_files: bool,
    pub comparison: Comparison,
    pub metadata: Option<&'a Metadata>,
    pub search: &'a Search,
    pub visible: &'a [usize],
    pub selected: usize,
    pub viewport: &'a crate::tui::navigation::ListViewport,
    pub reader: Reader,
    pub active: Region,
    pub preview: &'a preview::State,
    pub scroll: u16,
    pub busy: bool,
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
fn status_style(kind: &str) -> Style {
    match kind {
        "unmerged" => ui::danger(),
        "untracked" | "renamed" => ui::warning(),
        _ => Style::default(),
    }
}
fn details(view: &View<'_>, fallback: Option<&str>) -> Text<'static> {
    let mut lines = Vec::new();
    if let Some(reason) = fallback {
        lines.push(note(reason, ui::muted()));
    }
    if view.visible.contains(&view.selected) {
        match view.metadata {
            Some(Metadata::Status(status)) => {
                if let Some(entry) = status.entries.get(view.selected) {
                    lines.push(note(&entry.kind, status_style(&entry.kind)));
                    lines.push(field("XY", entry.xy.as_deref().unwrap_or("-")));
                    lines.push(field("Path", format!("{:?}", entry.path)));
                    lines.push(field("Original path", format!("{:?}", entry.original_path)));
                }
            }
            Some(Metadata::Diff(diff)) => {
                if let Some(entry) = diff.entries.get(view.selected) {
                    lines.push(field("Change", &entry.change));
                    lines.push(field(
                        "Modes",
                        format!("{} -> {}", entry.old_mode, entry.new_mode),
                    ));
                    lines.push(field("Path", format!("{:?}", entry.path)));
                }
            }
            None => {}
        }
    } else {
        lines.push(Line::raw(match view.metadata {
            None => "Summary unavailable/not loaded. Choose scope, then refresh.",
            Some(metadata) if metadata.len() == 0 => "No changes in this comparison.",
            Some(_) => "No displayed entry selected. Clear the filter or refresh.",
        }));
    }
    lines.push(Line::default());
    lines.push(Line::styled("Git scope | read-only", ui::heading()));
    lines.push(field("Git-only root", format!("{:?}", view.root)));
    lines.push(field(
        "Selection",
        if view.follows_files {
            "current file repository"
        } else {
            "independent session choice"
        },
    ));
    lines.push(field("Comparison", view.comparison.label()));
    if let Some(metadata) = view.metadata {
        lines.push(field("Entries", metadata.len()));
        match metadata {
            Metadata::Status(status) => {
                lines.push(Line::default());
                lines.push(Line::styled("Branch headers", ui::heading()));
                for header in &status.branch {
                    lines.push(note(header, Style::default()));
                }
                if status.branch.is_empty() {
                    lines.push(Line::raw("No branch headers reported."));
                }
            }
            Metadata::Diff(diff) => {
                lines.push(field("Staged", diff.staged));
            }
        }
    } else {
        lines.push(field("Entries", "Unknown (summary unavailable/not loaded)"));
    }
    lines.push(Line::default());
    lines.push(Line::styled("Details", ui::heading()));
    lines.push(Line::raw("Git requires a trusted executable and exact non-bare root; no manifest or registration required."));
    lines.push(Line::raw("No staging, commits, network or live-file writes. Untracked contents are not included in diffs."));
    Text::from(lines)
}
fn filter(frame: &mut Frame, area: Rect, view: &View<'_>) {
    ui::design::filter(
        frame,
        area,
        "Filter changes",
        &view.search.query,
        view.search.editing && view.hits.input_focus_allowed() && !view.busy,
    );
}

pub(super) fn draw(frame: &mut Frame, area: Rect, view: View<'_>) {
    let branch = match view.metadata {
        Some(Metadata::Status(status)) if area.height >= 12 => Some(status.branch.join(" | ")),
        _ => None,
    };
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(branch.is_some())),
        Constraint::Min(3),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if view.metadata.is_some() {
                    format!("{} displayed | ", view.visible.len())
                } else {
                    "Not loaded | ".into()
                },
                ui::muted(),
            ),
            Span::raw(ui::short_field(
                "Root",
                view.root.and_then(|p| p.to_str()).unwrap_or("not selected"),
                area.width.saturating_sub(24),
            )),
        ])),
        sections[0],
    );
    filter(frame, sections[1], &view);
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let content = sections[3];
    let workspace = Workspace::new(Scope::Git, content, view.active, !view.visible.is_empty());
    let wide = workspace.is_split();
    workspace.bind(view.hits);
    let action = if view.search.editing {
        Some((
            "Done",
            "Enter",
            crate::tui::input::Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ))
    } else if view.active == Region::Details {
        Some(("Back", "Esc", crate::tui::input::Target::Back))
    } else if view.metadata.is_some_and(|metadata| metadata.len() > 0) && !view.visible.is_empty() {
        Some((
            match (view.preview.available(), wide) {
                (true, true) => "Focus patch",
                (true, false) => "Open patch",
                _ => "Details",
            },
            "Enter",
            crate::tui::input::Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ))
    } else if view.metadata.is_none() {
        Some((
            "Choose Git scope",
            "Enter",
            crate::tui::input::Target::Action(crate::tui::shell::Action::GitRoot),
        ))
    } else {
        None
    };
    if let Some((label, key, target)) = action {
        view.hits.present(ui::ActionButton { label, key, target });
    }
    view.hits.present(ui::ActionButton {
        label: if view.reader == Reader::Metadata && view.active == Region::Details {
            "Entries"
        } else {
            "Scope details"
        },
        key: "F3",
        target: crate::tui::input::Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
    });
    view.hits
        .click(sections[1], crate::tui::input::Target::Filter);
    if let Some(branch) = branch {
        frame.render_widget(
            Paragraph::new(ui::clip(&branch, area.width)).style(ui::muted()),
            sections[2],
        );
    }
    if let Some(pane) = workspace.details {
        let focused = workspace.active == Region::Details;
        if view.reader == Reader::Metadata || !view.preview.draw(frame, pane, view.hits, focused) {
            draw_details(frame, pane, &view, view.preview.fallback(), focused);
        }
    }
    let Some(list) = workspace.queue else { return };
    let title = view
        .metadata
        .map(|m| format!("Details | {} entries", m.len()))
        .unwrap_or_else(|| "Details | Unknown".into());
    let focused = view.active == Region::Queue;
    if view.metadata.is_none()
        || view.metadata.is_some_and(|m| m.len() == 0)
        || view.visible.is_empty()
    {
        let block = ui::pane(&title, focused);
        let inner = block.inner(list);
        frame.render_widget(block, list);
        let text = match view.metadata {
            None => "Summary unavailable/not loaded. Choose scope, then refresh.",
            Some(m) if m.len() == 0 => "No changes in this comparison.",
            Some(_) => "No matches. Clear the details filter.",
        };
        ui::text_view(frame, inner, Text::raw(text), 0);
    } else if let Some(metadata) = view.metadata {
        let (widths, labels) = match metadata {
            Metadata::Status(_) => (
                [
                    Constraint::Min(8),
                    Constraint::Length(11),
                    Constraint::Length(2),
                ],
                ["PATH", "KIND", "XY"],
            ),
            Metadata::Diff(_) => (
                [
                    Constraint::Min(8),
                    Constraint::Length(6),
                    Constraint::Length(16),
                ],
                ["PATH", "CHANGE", "MODES"],
            ),
        };
        let collection = Collection::new(widths, Header::Heading(labels)).titled(&title, focused);
        let selected = view.visible.iter().position(|&i| i == view.selected);
        let window = collection.window(list, view.viewport, view.visible.len(), selected);
        let rows = view.visible[window.range].iter().filter_map(|&index| {
            let cells = match metadata {
                Metadata::Status(status) => status.entries.get(index).map(|entry| {
                    [
                        Cell::from(ui::inline(&entry.path)),
                        Cell::from(ui::inline(&entry.kind)).style(status_style(&entry.kind)),
                        Cell::from(ui::inline(entry.xy.as_deref().unwrap_or("-"))),
                    ]
                }),
                Metadata::Diff(diff) => diff.entries.get(index).map(|entry| {
                    [
                        Cell::from(ui::inline(&entry.path)),
                        Cell::from(ui::inline(&entry.change)).style(ui::warning()),
                        Cell::from(ui::inline(&format!(
                            "{} -> {}",
                            entry.old_mode, entry.new_mode
                        ))),
                    ]
                }),
            }?;
            Some((index, Row::new(cells)))
        });
        collection.draw(
            frame,
            list,
            rows,
            window.selection,
            RowTargets {
                hits: view.hits,
                wheel: Some(Wheel::Rows),
                target: Target::Row,
            },
        );
    }
}
fn draw_details(
    frame: &mut Frame,
    area: Rect,
    view: &View<'_>,
    fallback: Option<&str>,
    focused: bool,
) {
    ui::reader::Reader::new(view.hits).report(
        frame,
        area,
        "Git details",
        details(view, fallback),
        view.scroll,
        focused,
    );
}
