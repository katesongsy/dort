//! Workflow pages: field-only traversal and direct, explicitly labelled actions.
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
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, form: Form) {
    let s = &app.shell;
    let (fields, focus, primary): (Vec<(&str, &str)>, usize, &'static str) = match form {
        Form::Adopt => (
            vec![
                ("TARGET", &s.adoption.target),
                ("ID", &s.adoption.id),
                ("SOURCE", &s.adoption.source),
            ],
            s.adoption.focused,
            "Review read-only preview",
        ),
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
    ui::field_page(
        frame,
        area,
        &fields,
        focus,
        &app.hits,
        |i| Target::FormControl(i, false),
        ui::FormActions {
            choices: &[],
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
