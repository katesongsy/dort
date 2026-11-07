//! Session-only pickers rendered from typed scope/choice data, never report strings.
use super::{
    app::App,
    components as ui,
    shell::{Overlay, ProfilePicker},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Paragraph, Row, Table, TableState},
};

fn field(label: &str, value: &str) -> Line<'static> {
    ui::metadata(&format!("{label}: "), value)
}
fn names(
    lines: &mut Vec<Line<'static>>,
    label: &'static str,
    values: impl IntoIterator<Item = impl AsRef<str>>,
) {
    lines.push(Line::styled(label, ui::heading()));
    let mut empty = true;
    for value in values {
        empty = false;
        lines.push(Line::raw(ui::inline(value.as_ref())));
    }
    if empty {
        lines.push(Line::styled("(none)", ui::muted()));
    }
}
pub fn profile_details(picker: &ProfilePicker) -> Text<'static> {
    let choices = &picker.report.choices;
    let effective: std::collections::BTreeSet<_> =
        choices.mandatory.iter().chain(&picker.requested).collect();
    let mut lines = vec![
        Line::styled("Temporary profiles | Never persisted", ui::heading()),
        field(
            "Scope",
            if picker.standalone {
                "standalone manifest only"
            } else {
                "repository file/check scope"
            },
        ),
        field(
            "Manifest",
            picker.report.manifest.to_str().unwrap_or("Unavailable"),
        ),
        Line::raw("Base is always active. Additional profiles join mandatory contributors."),
    ];
    if let Some(name) = choices.known.iter().nth(picker.selected) {
        lines.push(field("Selected", name));
        lines.push(field(
            "Membership",
            if choices.mandatory.contains(name) {
                "Mandatory"
            } else {
                "Optional session addition"
            },
        ));
    } else {
        lines.push(field("Selected", "Apply additions (read-only validation)"));
    }
    names(&mut lines, "Defaults", &choices.defaults);
    names(&mut lines, "Platform", &choices.platform);
    names(&mut lines, "Local", &choices.local);
    names(&mut lines, "Requested additions", &picker.requested);
    names(&mut lines, "Effective union", effective);
    names(&mut lines, "Known profiles", &choices.known);
    lines.push(Line::styled(
        "Apply validates before changing session scope. Esc leaves it unchanged.",
        ui::muted(),
    ));
    Text::from(lines)
}
pub fn repository_details(app: &App, selected: usize) -> Text<'static> {
    let roots = app
        .data
        .as_ref()
        .map(|d| d.known_roots.as_slice())
        .unwrap_or_default();
    let mut lines = vec![
        Line::styled("Choose repository | Session only", ui::heading()),
        Line::raw("The exact root becomes this session's selection."),
        field(
            "Session root",
            app.data
                .as_ref()
                .and_then(|d| d.repository.as_ref())
                .or(app.repository.as_ref())
                .and_then(|p| p.to_str())
                .unwrap_or("None selected"),
        ),
        field("Origin", app.origin),
    ];
    let overview = app.data.as_ref().and_then(|d| d.overview.as_ref());
    let configuration_error = overview.and_then(|o| o.configuration_error.as_deref());
    lines.push(field(
        "Registry",
        if overview.is_none() {
            "Not loaded; known roots unknown"
        } else if overview.and_then(|o| o.registry_count).is_none() {
            "Unavailable; known roots unknown"
        } else if roots.is_empty() {
            "No known roots"
        } else {
            "Known roots loaded"
        },
    ));
    if let Some(error) = configuration_error {
        lines.push(Line::styled(ui::inline(error), ui::warning()));
    }
    match selected {
        0 => {
            lines.push(field("Selected", "Enter exact repository path"));
            lines.push(field("Retained path", &app.shell.choose_draft));
            lines.push(field(
                "Launch dir",
                app.launch_directory.to_str().unwrap_or("Unavailable"),
            ));
        }
        1 => {
            lines.push(field("Selected", "Use registered active repository"));
            lines.push(Line::raw("The read-only service resolves the registered active root. The explicit session root remains separately selectable. Missing or invalid configuration is an error; the old scope is retained."));
        }
        i if i == roots.len() + 2 => {
            lines.push(field("Selected", "Create/attach"));
            lines.push(field("Retained path", &app.shell.init_draft));
            lines.push(Line::raw("Opens a retained path draft, then a read-only preview. Only separate explicit approval can publish registration."));
        }
        i => {
            if let Some(root) = roots.get(i.saturating_sub(2)) {
                lines.push(field(
                    "Selected root",
                    root.to_str().unwrap_or("Unavailable"),
                ));
            }
        }
    }
    if roots.is_empty() && overview.and_then(|o| o.registry_count).is_none() {
        lines.push(field("Known roots", "Unknown"));
    } else {
        names(
            &mut lines,
            "Known roots",
            roots.iter().map(|p| p.to_str().unwrap_or("Unavailable")),
        );
    }
    lines.push(Line::styled(
        "Enter selects for this session. Esc returns with adoption drafts intact.",
        ui::muted(),
    ));
    Text::from(lines)
}
fn table(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: Vec<Row<'static>>,
    headers: Row<'static>,
    widths: &[Constraint],
    selected: Option<usize>,
) -> usize {
    let table = Table::new(rows, widths.to_vec())
        .header(headers.style(ui::muted()))
        .block(ui::pane(title, selected.is_some()))
        .row_highlight_style(ui::selected())
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(table, area, &mut state);
    state.offset()
}
fn choices_hits(app: &App, area: Rect, count: usize, offset: usize, toggle: bool) {
    app.hits.wheel(area, super::input::Wheel::Choices);
    for (y, index) in (offset..count)
        .take(usize::from(area.height.saturating_sub(3)))
        .enumerate()
    {
        app.hits.click(
            Rect::new(
                area.x + 1,
                area.y + 2 + y as u16,
                area.width.saturating_sub(2),
                1,
            ),
            super::input::Target::Picker(index, toggle),
        );
    }
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, overlay: &Overlay) {
    let (title, details) = match overlay {
        Overlay::Profiles(picker) => (
            if picker.standalone {
                "Profiles | standalone manifest"
            } else {
                "Profiles | repository scope"
            },
            profile_details(picker),
        ),
        Overlay::Repositories { selected } => (
            "Choose repository | Session only",
            repository_details(app, *selected),
        ),
        _ => unreachable!("only typed pickers use this renderer"),
    };
    let parts = Layout::vertical([
        Constraint::Length(0),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(0),
        Constraint::Length(if area.height >= 14 { 6 } else { 4 }),
    ])
    .split(area);
    app.shell
        .page_rows
        .set(parts[4].height.saturating_sub(2).max(1));
    ui::report(
        frame,
        parts[4],
        "Selection details",
        details,
        app.shell.scroll,
        false,
    );
    match overlay {
        Overlay::Profiles(picker) => {
            let choices = &picker.report.choices;
            let count = choices.known.len();
            let apply = picker.selected == count;
            let rows = choices
                .known
                .iter()
                .map(|name| {
                    let mandatory = choices.mandatory.contains(name);
                    Row::new(vec![
                        if mandatory || picker.requested.contains(name) {
                            "[x]".into()
                        } else {
                            "[ ]".into()
                        },
                        ui::clip(name, parts[1].width.saturating_sub(22)),
                        if mandatory {
                            "Mandatory".into()
                        } else {
                            "Optional".into()
                        },
                    ])
                })
                .collect();
            let offset = table(
                frame,
                parts[1],
                title,
                rows,
                Row::new(["ON", "PROFILE", "MEMBERSHIP"]),
                &[
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(10),
                ],
                (!apply).then_some(picker.selected),
            );
            choices_hits(app, parts[1], count, offset, true);
            app.hits.click(
                Rect::new(
                    parts[2].x,
                    parts[2].y,
                    parts[2]
                        .width
                        .min(ui::button("Apply additions", "Enter", apply).width() as u16),
                    1,
                ),
                super::input::Target::Picker(count, true),
            );
            frame.render_widget(
                Paragraph::new(Line::from(ui::button("Apply additions", "Enter", apply))),
                parts[2],
            );
        }
        Overlay::Repositories { selected } => {
            let roots = app
                .data
                .as_ref()
                .map(|d| d.known_roots.as_slice())
                .unwrap_or_default();
            let create = *selected == roots.len() + 2;
            let mut rows = vec![
                Row::new(["Enter exact repository path", "Path"]),
                Row::new(["Use registered active repository", "Active"]),
            ];
            rows.extend(roots.iter().map(|p| {
                Row::new([
                    ui::clip(
                        p.to_str().unwrap_or("Unavailable"),
                        parts[1].width.saturating_sub(12),
                    ),
                    "Known".into(),
                ])
            }));
            let offset = table(
                frame,
                parts[1],
                "Repository selector",
                rows,
                Row::new(["REPOSITORY", "CHOICE"]),
                &[Constraint::Min(12), Constraint::Length(6)],
                (!create).then_some(*selected),
            );
            choices_hits(app, parts[1], roots.len() + 2, offset, false);
            app.hits.click(
                Rect::new(
                    parts[2].x,
                    parts[2].y,
                    parts[2]
                        .width
                        .min(ui::button("Create/attach", "Enter", create).width() as u16),
                    1,
                ),
                super::input::Target::Picker(roots.len() + 2, true),
            );
            frame.render_widget(
                Paragraph::new(Line::from(ui::button("Create/attach", "Enter", create))),
                parts[2],
            );
        }
        _ => unreachable!(),
    }
}
