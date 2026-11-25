//! Workflow forms with field-only traversal and labelled actions.
use super::{app::App, components as ui, input::Target, shell::Form};
use ratatui::{Frame, layout::Rect};
#[derive(Clone, Copy)]
pub enum Command {
    Submit,
    Choose,
    Create,
    Profiles,
    Discard,
    Details,
    InsertLine,
    Toggle,
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, form: Form) {
    let s = &app.shell;
    let line_labels;
    let (fields, focus, primary): (Vec<(&str, &str)>, usize, &'static str) = match form {
        Form::Adopt => (
            vec![
                ("TARGET", &s.adoption.target),
                ("ID", &s.adoption.id),
                ("SOURCE", &s.adoption.source),
                ("Profiles (blank uses base)", &s.adoption.profiles),
            ],
            s.adoption.focused,
            "Review read-only preview",
        ),
        Form::Configure => {
            let Some(draft) = &s.configuration else {
                return;
            };
            (draft.fields(), draft.focus, draft.primary())
        }
        Form::Repair => {
            let Some(editor) = &s.repair else {
                return;
            };
            line_labels = (1..=editor.lines().len())
                .map(|i| format!("Line {i}"))
                .collect::<Vec<_>>();
            let fields = line_labels
                .iter()
                .map(String::as_str)
                .zip(editor.lines().iter().map(String::as_str))
                .collect();
            (fields, editor.focus, "Review manifest diff")
        }
        Form::Check => (
            vec![("Manifest path", &s.manifest_draft)],
            0,
            "Check manifest",
        ),
        Form::KeepId | Form::RemoveId => (
            vec![("Exact ID", &s.unmanage_id)],
            0,
            "Review read-only preview",
        ),
        Form::Clone => (
            vec![
                ("Git source", &s.clone_fields[0]),
                ("New folder", &s.clone_fields[1]),
            ],
            s.path_focused,
            "Review clone and registration",
        ),
        Form::Choose | Form::Init => (
            vec![("Path", s.draft(form))],
            0,
            if form == Form::Choose {
                "Select repository"
            } else {
                "Review read-only preview"
            },
        ),
    };
    let mut actions = vec![ui::ActionButton {
        label: primary,
        key: "Enter",
        target: Target::FormAction(Command::Submit),
    }];
    match form {
        Form::Adopt => {
            actions.push(ui::ActionButton {
                label: "Repo",
                key: "Ctrl-R",
                target: Target::FormAction(Command::Choose),
            });
            actions.push(ui::ActionButton {
                label: "New",
                key: "Ctrl-N",
                target: Target::FormAction(Command::Create),
            });
            actions.push(ui::ActionButton {
                label: "Reset",
                key: "Ctrl-D",
                target: Target::FormAction(Command::Discard),
            });
        }
        Form::Check => {
            actions.push(ui::ActionButton {
                label: "Profiles",
                key: "Ctrl-P",
                target: Target::FormAction(Command::Profiles),
            });
            actions.push(ui::ActionButton {
                label: "Reset",
                key: "Ctrl-D",
                target: Target::FormAction(Command::Discard),
            });
        }
        Form::Repair => actions.push(ui::ActionButton {
            label: "Insert line",
            key: "F4",
            target: Target::FormAction(Command::InsertLine),
        }),
        Form::Configure
            if app.form_has_focus()
                && s.configuration
                    .as_ref()
                    .is_some_and(|d| d.is_default_row(d.focus)) =>
        {
            actions.push(ui::ActionButton {
                label: "Toggle",
                key: "Space",
                target: Target::FormAction(Command::Toggle),
            });
        }
        Form::Choose => actions.push(ui::ActionButton {
            label: "New",
            key: "Ctrl-N",
            target: Target::FormAction(Command::Create),
        }),
        Form::Init => actions.push(ui::ActionButton {
            label: "Repo",
            key: "Ctrl-R",
            target: Target::FormAction(Command::Choose),
        }),
        _ => (),
    }
    actions.push(ui::ActionButton {
        label: "Details",
        key: "F3",
        target: Target::FormAction(Command::Details),
    });
    let toggles = s
        .configuration
        .as_ref()
        .filter(|_| form == Form::Configure)
        .map(|draft| draft.toggle_rows())
        .unwrap_or_default();
    ui::field_page(
        frame,
        area,
        &fields,
        focus,
        &app.hits,
        |i| Target::FormControl(i, false),
        ui::FormActions {
            choices: &[],
            toggles: &toggles,
            buttons: &actions,
            feedback: s
                .form_feedback
                .filter(|(owner, _)| *owner == form)
                .map(|(_, text)| ui::Feedback {
                    text,
                    tone: ui::design::Tone::Attention,
                }),
        },
    );
}
