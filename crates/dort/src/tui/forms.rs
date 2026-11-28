//! Workflow forms with field-only traversal and labelled actions.
use super::{
    app::App,
    components as ui,
    input::Target,
    shell::{AdoptionDraft, Form},
};
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
    OpenTemplateEditor,
    ToggleGeneratedInput,
    ToggleGeneratedDeclaration,
    ToggleGeneratedSource,
    ClearLocalInput,
    Toggle,
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App, form: Form) {
    let s = &app.shell;
    let line_labels;
    let generated_fields;
    let mut area = area;
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
        Form::Generated => {
            let Some(draft) = &s.generated else {
                return;
            };
            let heading = Rect::new(area.x, area.y, area.width, 1);
            ui::prose(frame, heading, &draft.title(), ui::heading());
            let instructions = draft.instructions();
            let height = if usize::from(area.width) < instructions.len() {
                2
            } else {
                1
            };
            ui::prose(
                frame,
                Rect::new(area.x, area.y + 1, area.width, height),
                instructions,
                ui::muted(),
            );
            area = Rect::new(
                area.x,
                area.y + 1 + height,
                area.width,
                area.height.saturating_sub(1 + height),
            );
            generated_fields = draft.input_fields();
            let primary = if draft.entering_inputs() {
                "Review generated file"
            } else {
                "Continue"
            };
            (
                generated_fields
                    .iter()
                    .map(|(label, value)| (label.as_str(), *value))
                    .collect(),
                draft.focus(),
                primary,
            )
        }
        Form::TemplateFile => {
            if let Some(screen) = &s.template_file {
                screen.draw(frame, area, app);
            }
            return;
        }
        Form::LocalInputs => {
            if let Some(panel) = &s.local_inputs {
                panel.draw(frame, area, app);
            }
            return;
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
        key: if form == Form::Generated {
            app.draft_continue_key()
        } else {
            "Enter"
        },
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
        Form::Generated
            if s.generated
                .as_ref()
                .is_some_and(|draft| !draft.entering_inputs()) =>
        {
            if let Some(draft) = &s.generated {
                if app.form_has_focus() && draft.on_content_row() {
                    let next = if draft.content_is_new() {
                        "In dort.toml"
                    } else if draft.content_is_file() {
                        "New template file"
                    } else {
                        "Template file"
                    };
                    actions.push(ui::ActionButton {
                        label: next,
                        key: "Space",
                        target: Target::FormAction(Command::ToggleGeneratedSource),
                    });
                } else if !draft.content_is_file() {
                    actions.push(ui::ActionButton {
                        label: "Open in editor",
                        key: "F5",
                        target: Target::FormAction(Command::OpenTemplateEditor),
                    });
                }
            }
        }
        Form::Generated if app.form_has_focus() => {
            if let Some(label) = s.generated.as_ref().and_then(|draft| draft.value_action()) {
                actions.push(ui::ActionButton {
                    label,
                    key: "F6",
                    target: Target::FormAction(Command::ToggleGeneratedInput),
                });
            } else {
                actions.push(ui::ActionButton {
                    label: "Toggle",
                    key: "Space",
                    target: Target::FormAction(Command::ToggleGeneratedDeclaration),
                });
            }
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
    if form != Form::Generated {
        actions.push(ui::ActionButton {
            label: "Details",
            key: "F3",
            target: Target::FormAction(Command::Details),
        });
    }
    let choices = match form {
        Form::Adopt => vec![AdoptionDraft::PROFILES_FIELD],
        Form::Configure => s
            .configuration
            .as_ref()
            .map(|d| d.choice_rows())
            .unwrap_or_default(),
        _ => vec![],
    };
    let toggles = match form {
        Form::Configure => s.configuration.as_ref().map(|draft| draft.toggle_rows()),
        Form::Generated => s.generated.as_ref().map(|draft| draft.toggle_rows()),
        _ => None,
    }
    .unwrap_or_default();
    let feedback = s
        .configuration
        .as_ref()
        .filter(|_| form == Form::Configure)
        .and_then(|draft| draft.feedback.as_ref())
        .map(|(text, problem)| ui::Feedback {
            text,
            tone: if *problem {
                ui::design::Tone::Attention
            } else {
                ui::design::Tone::Neutral
            },
        })
        .or_else(|| {
            s.form_feedback
                .as_ref()
                .filter(|(owner, _)| *owner == form)
                .map(|(_, text)| ui::Feedback {
                    text,
                    tone: ui::design::Tone::Attention,
                })
        });
    let controls = ui::FormActions {
        choices: &choices,
        toggles: &toggles,
        buttons: &actions,
        feedback,
    };
    if form == Form::Generated
        && let Some(draft) = &s.generated
        && !draft.entering_inputs()
    {
        let area = ui::action_bar(frame, area, &app.hits, controls);
        draft.draw_template(frame, area, &app.hits);
        return;
    }
    ui::field_page(
        frame,
        area,
        &fields,
        focus,
        &app.hits,
        |i| Target::FormControl(i, false),
        controls,
    );
}
