//! Session pickers built from typed choices.
use super::{
    app::App,
    components as ui,
    input::{Target, Wheel},
    shell::{Overlay, ProfilePicker},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::Row,
};
use ui::collection::{Collection, Header, RowTargets, Selection};

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
        Line::raw("Base is always active. Extra profiles join the always-active set."),
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
        lines.push(field("Selected", "Apply additions (validation only)"));
    }
    names(&mut lines, "Defaults", &choices.defaults);
    names(&mut lines, "Platform", &choices.platform);
    names(&mut lines, "Local", &choices.local);
    names(&mut lines, "Requested additions", &picker.requested);
    names(&mut lines, "Effective union", effective);
    names(&mut lines, "Known profiles", &choices.known);
    lines.push(Line::styled(
        "Apply validates before changing this session. Esc keeps the current profiles.",
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
            lines.push(Line::raw("Uses the registered active repository. You can still choose an explicit root. A missing or invalid config is an error; the old selection stays."));
        }
        i if i == roots.len() + 2 => {
            lines.push(field("Selected", "Create/attach"));
            lines.push(field("Retained path", &app.shell.init_draft));
            lines.push(Line::raw("Opens a saved path draft, then a read-only preview. Registration needs separate approval."));
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
    let mut actions = vec![];
    if app.shell.picker_details {
        actions.push(ui::ActionButton {
            label: "Choices",
            key: "F3",
            target: Target::PickerDetails,
        });
    } else {
        match overlay {
            Overlay::Profiles(picker) => actions.push(ui::ActionButton {
                label: "Apply additions",
                key: "Enter",
                target: Target::Picker(picker.report.choices.known.len(), true),
            }),
            Overlay::Repositories { selected } => {
                let count = app.data.as_ref().map_or(0, |d| d.known_roots.len()) + 2;
                actions.push(ui::ActionButton {
                    label: "Select",
                    key: "Enter",
                    target: Target::Picker((*selected).min(count - 1), true),
                });
                actions.push(ui::ActionButton {
                    label: "Create/attach",
                    key: "Ctrl-N",
                    target: Target::Picker(count, true),
                });
            }
            _ => unreachable!(),
        }
        actions.push(ui::ActionButton {
            label: "Details",
            key: "F3",
            target: Target::PickerDetails,
        });
    }
    let content = ui::action_bar(
        frame,
        area,
        &app.hits,
        ui::FormActions {
            choices: &[],
            buttons: &actions,
            feedback: None,
        },
    );
    if app.shell.picker_details {
        app.shell
            .page_rows
            .set(content.height.saturating_sub(2).max(1));
        ui::report(
            frame,
            content,
            "Selection details",
            details,
            app.shell.scroll,
            false,
        );
        return;
    }
    let detail_height = if content.height >= 12 { 6 } else { 0 };
    let parts =
        Layout::vertical([Constraint::Min(4), Constraint::Length(detail_height)]).split(content);
    app.shell
        .page_rows
        .set(parts[0].height.saturating_sub(2).max(1));
    if detail_height > 0 {
        ui::report(
            frame,
            parts[1],
            "Selection details",
            details,
            app.shell.scroll,
            false,
        );
    }
    match overlay {
        Overlay::Profiles(picker) => {
            let choices = &picker.report.choices;
            let count = choices.known.len();
            let rows = choices.known.iter().map(|name| {
                let mandatory = choices.mandatory.contains(name);
                Row::new(vec![
                    if mandatory || picker.requested.contains(name) {
                        "[x]".into()
                    } else {
                        "[ ]".into()
                    },
                    ui::clip(name, parts[0].width.saturating_sub(22)),
                    if mandatory {
                        "Mandatory".into()
                    } else {
                        "Optional".into()
                    },
                ])
            });
            let selected = (count > 0).then_some(picker.selected.min(count.saturating_sub(1)));
            Collection::new(
                [
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(10),
                ],
                Header::Quiet(["ON", "PROFILE", "MEMBERSHIP"]),
            )
            .titled(title, selected.is_some())
            .draw(
                frame,
                parts[0],
                rows.enumerate(),
                Selection::at(selected),
                RowTargets {
                    hits: &app.hits,
                    wheel: Some(Wheel::Choices),
                    target: |index| Target::Picker(index, true),
                },
            );
        }
        Overlay::Repositories { selected } => {
            let roots = app
                .data
                .as_ref()
                .map(|d| d.known_roots.as_slice())
                .unwrap_or_default();
            let mut rows = vec![
                Row::new(["Enter exact repository path", "Path"]),
                Row::new(["Use registered active repository", "Active"]),
            ];
            rows.extend(roots.iter().map(|p| {
                Row::new([
                    ui::clip(
                        p.to_str().unwrap_or("Unavailable"),
                        parts[0].width.saturating_sub(12),
                    ),
                    "Known".into(),
                ])
            }));
            Collection::new(
                [Constraint::Min(12), Constraint::Length(6)],
                Header::Quiet(["REPOSITORY", "CHOICE"]),
            )
            .titled(title, true)
            .draw(
                frame,
                parts[0],
                rows.into_iter().enumerate(),
                Selection::at(Some((*selected).min(roots.len() + 1))),
                RowTargets {
                    hits: &app.hits,
                    wheel: Some(Wheel::Choices),
                    target: |index| Target::Picker(index, false),
                },
            );
        }
        _ => unreachable!(),
    }
}
