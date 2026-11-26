//! Session pickers built from typed choices.
use super::{
    app::App,
    components as ui,
    input::{Target, Wheel},
    shell::{Overlay, ProfilePicker},
    workspace::{Region, Scope, Workspace},
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Text},
    widgets::Row,
};
use ui::collection::{Collection, Header, RowTargets, Selection};

use ui::design::Tone;
use ui::reader::{field, section};

fn fact(label: &str, value: &str) -> Line<'static> {
    field(label, value, Tone::Neutral)
}
fn names(
    lines: &mut Vec<Line<'static>>,
    label: &'static str,
    values: impl IntoIterator<Item = impl AsRef<str>>,
) {
    lines.push(section(label));
    let mut empty = true;
    for value in values {
        empty = false;
        lines.push(Line::raw(ui::inline(value.as_ref())));
    }
    if empty {
        lines.push(Line::raw("None"));
    }
}
pub fn profile_details(picker: &ProfilePicker) -> Text<'static> {
    let choices = &picker.report.choices;
    let effective: std::collections::BTreeSet<_> =
        choices.mandatory.iter().chain(&picker.requested).collect();
    let mut lines = vec![section("Temporary profiles | Never persisted")];
    if let Some(name) = choices.known.iter().nth(picker.selected) {
        lines.push(fact("Selected", name));
        lines.push(fact(
            "Membership",
            if choices.mandatory.contains(name) {
                "Mandatory"
            } else {
                "Optional session addition"
            },
        ));
    } else {
        lines.push(fact("Selected", "Apply additions (validation only)"));
    }
    lines.push(Line::raw(
        "Apply validates before changing this session. Esc keeps current profiles.",
    ));
    names(&mut lines, "Requested additions", &picker.requested);
    names(&mut lines, "Effective union", effective);
    lines.push(Line::raw("Base is always active."));
    lines.push(Line::default());
    lines.push(section("Profile sources"));
    lines.push(fact(
        "Scope",
        if picker.standalone {
            "standalone manifest only"
        } else {
            "repository file/check scope"
        },
    ));
    lines.push(fact(
        "Manifest",
        picker.report.manifest.to_str().unwrap_or("Unavailable"),
    ));
    names(&mut lines, "Defaults", &choices.defaults);
    names(&mut lines, "Platform", &choices.platform);
    names(&mut lines, "Local", &choices.local);
    names(&mut lines, "Known profiles", &choices.known);
    Text::from(lines)
}
pub fn repository_details(app: &App, selected: usize) -> Text<'static> {
    let roots = app
        .data
        .as_ref()
        .map(|d| d.known_roots.as_slice())
        .unwrap_or_default();
    let mut lines = vec![section("Choose repository | Session only")];
    match selected {
        0 => {
            lines.push(fact("Selected", "Enter exact repository path"));
            lines.push(fact("Retained path", &app.shell.choose_draft));
            lines.push(Line::raw(
                "The exact root becomes this session's selection.",
            ));
        }
        1 => {
            lines.push(fact("Selected", "Use registered active repository"));
            lines.push(Line::raw("Missing or invalid config keeps the old selection. Choose an explicit root instead."));
        }
        i if i == roots.len() + 2 => {
            lines.push(fact("Selected", "Create/attach"));
            lines.push(fact("Retained path", &app.shell.init_draft));
            lines.push(Line::raw(
                "Opens the saved draft for review. Registration needs separate approval.",
            ));
        }
        i => {
            if let Some(root) = roots.get(i.saturating_sub(2)) {
                lines.push(fact(
                    "Selected root",
                    root.to_str().unwrap_or("Unavailable"),
                ));
            }
        }
    }
    lines.push(Line::raw(
        "Enter selects for this session. Esc keeps adoption drafts.",
    ));
    lines.push(Line::default());
    lines.push(section("Session and registry"));
    lines.push(fact(
        "Session root",
        app.data
            .as_ref()
            .and_then(|d| d.repository.as_ref())
            .or(app.repository.as_ref())
            .and_then(|p| p.to_str())
            .unwrap_or("None selected"),
    ));
    lines.push(fact("Origin", app.origin));
    if selected == 0 {
        lines.push(fact(
            "Launch dir",
            app.launch_directory.to_str().unwrap_or("Unavailable"),
        ));
    }
    let overview = app.data.as_ref().and_then(|d| d.overview.as_ref());
    let configuration_error = overview.and_then(|o| o.configuration_error.as_deref());
    lines.push(fact(
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
        lines.push(Line::styled(ui::inline(error), Tone::Attention.style()));
    }
    if roots.is_empty() && overview.and_then(|o| o.registry_count).is_none() {
        lines.push(fact("Known roots", "Unknown"));
    } else {
        names(
            &mut lines,
            "Known roots",
            roots.iter().map(|p| p.to_str().unwrap_or("Unavailable")),
        );
    }
    Text::from(lines)
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, overlay: &Overlay) {
    let (scope, title, details) = match overlay {
        Overlay::Profiles(picker) => (
            Scope::Profiles,
            if picker.standalone {
                "Standalone profiles"
            } else {
                "Repository profiles"
            },
            profile_details(picker),
        ),
        Overlay::Repositories { selected } => (
            Scope::Repositories,
            "Repository choices",
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
            toggles: &[],
            buttons: &actions,
            feedback: match overlay {
                Overlay::Profiles(picker) => picker.problem.as_deref().map(|text| ui::Feedback {
                    text,
                    tone: Tone::Attention,
                }),
                _ => None,
            },
        },
    );
    let active = Region::from_details(app.shell.picker_details);
    let paired =
        !matches!(overlay, Overlay::Profiles(picker) if picker.report.choices.known.is_empty());
    let workspace = Workspace::new(scope, content, active, paired);
    workspace.bind(&app.hits);
    if let Some(pane) = workspace.details {
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .report(
                frame,
                pane,
                "Selection details",
                details,
                app.shell.scroll,
                active == Region::Details,
            );
    }
    let Some(list) = workspace.queue else { return };
    app.shell
        .page_rows
        .set(list.height.saturating_sub(2).max(1));
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
                    ui::clip(name, list.width.saturating_sub(22)),
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
            .titled(title, active == Region::Queue)
            .draw(
                frame,
                list,
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
                        list.width.saturating_sub(12),
                    ),
                    "Known".into(),
                ])
            }));
            Collection::new(
                [Constraint::Min(12), Constraint::Length(6)],
                Header::Quiet(["REPOSITORY", "CHOICE"]),
            )
            .titled(title, active == Region::Queue)
            .draw(
                frame,
                list,
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
