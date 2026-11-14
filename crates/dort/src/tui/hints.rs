//! Contextual keyboard hints for the visible surface. No payloads or approvals.
use super::{
    app::{App, Tab},
    shell::Overlay,
};

pub fn keys(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.help.is_some() {
        return vec![
            ("F1", "Back"),
            ("Esc", "Back"),
            ("PgDn", "Scroll"),
            ("Home", "Top"),
        ];
    }
    if app.navigation.menu.is_some() {
        return vec![("F1", "Help"), ("Esc", "Close")];
    }
    let mut keys = vec![("F1", "Help")];
    if app.busy {
        keys.extend([("q", "Quit waits"), ("PgDn", "Scroll")]);
    } else if app.error.is_some() {
        keys.extend([
            ("Esc", "Back"),
            ("r", if app.failed_write { "Refresh" } else { "Retry" }),
        ]);
    } else if app.show_result && app.preview.is_none() {
        keys.extend([("Esc", "Back"), ("r", "Refresh")]);
    } else if app.confirmation.is_some() {
        keys.extend([("Enter", "Confirm"), ("Esc", "Back")]);
    } else if let Some(preview) = &app.preview {
        if !app.dry_run && !preview.pending.blocked() {
            keys.push((
                "Enter",
                if preview.pending.adoption() {
                    "Adopt all"
                } else {
                    "Continue"
                },
            ));
        }
        keys.extend([("Esc", "Back"), ("i", "Full report")]);
    } else if let Some(editor) = app.batch_editor.as_ref().filter(|e| e.open) {
        if editor.details {
            keys.extend([("F2", "Draft"), ("PgDn", "Scroll"), ("Esc", "Back")]);
        } else {
            keys.extend([
                (
                    "Enter",
                    if editor.item_form {
                        "Done"
                    } else {
                        "Review all"
                    },
                ),
                ("F2", "Metadata"),
                ("Esc", "Back"),
            ]);
        }
    } else if let Some(overlay) = &app.shell.overlay {
        match overlay {
            Overlay::Ignored { paths, selected } => {
                if *selected < paths.len() {
                    keys.push(("Enter", "Unignore"));
                }
                keys.push(("Esc", "Back"));
            }
            Overlay::Actions { .. } if app.shell.picker_details => {
                keys.extend([("F3", "Actions"), ("PgDn", "Page"), ("Esc", "Actions")])
            }
            Overlay::Actions { .. } => {
                keys.extend([("Enter", "Choose"), ("F3", "Metadata"), ("Esc", "Close")])
            }
            Overlay::Repositories { .. } if app.shell.picker_details => {
                keys.extend([("F3", "Choices"), ("PgDn", "Page"), ("Esc", "Choices")])
            }
            Overlay::Repositories { .. } => keys.extend([
                ("Enter", "Select"),
                ("F2", "Create"),
                ("F3", "Metadata"),
                ("Esc", "Close"),
            ]),
            Overlay::Profiles(_) if app.shell.picker_details => {
                keys.extend([("F3", "Choices"), ("PgDn", "Page"), ("Esc", "Choices")])
            }
            Overlay::Profiles(picker) => {
                keys.push(("Enter", "Apply"));
                if picker
                    .report
                    .choices
                    .known
                    .iter()
                    .nth(picker.selected)
                    .is_some_and(|name| !picker.report.choices.mandatory.contains(name))
                {
                    keys.push(("Space", "Toggle"));
                }
                keys.extend([("F3", "Metadata"), ("Esc", "Close")]);
            }
            Overlay::Form(_) if app.shell.form_details => {
                keys.extend([("F2", "Fields"), ("PgDn", "Page"), ("Esc", "Fields")])
            }
            Overlay::Form(form) => keys.extend([
                ("Enter", super::forms::enter_hint(app, *form)),
                ("Esc", "Back"),
                ("Tab", "Field"),
            ]),
            Overlay::About => keys.push(("Esc", "Back")),
        }
    } else if app.tab == Tab::Git && app.git_view.navigation_input() {
        keys.extend([
            (
                "Enter",
                if app.git_view.editing_root {
                    "Read"
                } else {
                    "View patch"
                },
            ),
            ("Esc", "Back"),
        ]);
    } else if app.tab == Tab::Tools {
        if app.tools_view.declaration_open {
            keys.extend([("Tab", "Field"), ("Enter", "Review"), ("Esc", "Back")]);
        } else if app.tools_view.install_open {
            use super::tools_install::Mode;
            if app.tools_view.installation.details {
                keys.extend([("d/Esc", "Back"), ("PgDn", "Page")]);
                if matches!(
                    app.tools_view.installation.mode,
                    Mode::Result | Mode::Attempts
                ) {
                    keys.push(("f", "Fresh review"));
                }
            } else {
                match app.tools_view.installation.mode {
                    Mode::Review | Mode::CloseReview => {
                        keys.extend([("Enter", "Confirm once"), ("d", "Scope"), ("Esc", "Back")]);
                        if app.tools_view.installation.mode == Mode::Review
                            && app
                                .tools_view
                                .installation
                                .scope
                                .as_ref()
                                .is_some_and(|s| s.brew.is_some() || s.apt.is_some())
                        {
                            keys.push(("a", "Advanced"));
                        }
                    }
                    Mode::CloseInputs | Mode::PrefixInputs | Mode::AptInputs => {
                        keys.extend([("Tab", "Field"), ("Enter", "Review"), ("Esc", "Back")])
                    }
                    Mode::Result => keys.extend([
                        ("f", "Fresh review"),
                        ("d", "Outcome"),
                        ("i", "Attempts"),
                        ("Esc", "Back"),
                    ]),
                    Mode::Attempts => keys.extend([
                        ("Enter", "Details"),
                        ("f", "Fresh review"),
                        ("c", "Close review"),
                        ("r", "Refresh"),
                        ("Esc", "Back"),
                    ]),
                    _ => keys.extend([("i", "Attempts"), ("d", "Outcome"), ("Esc", "Back")]),
                }
            }
        } else if app.tools_view.recon_open {
            if app.tools_view.reconciliation.binding_metadata() {
                keys.extend([("d/Esc", "Back"), ("PgDn", "Scroll"), ("Home", "Top")]);
                return keys;
            }
            match app.tools_view.reconciliation.mode {
                super::tools_reconcile::Mode::Inputs => {
                    keys.extend([("Tab", "Field"), ("Enter", "Load"), ("Esc", "Back")])
                }
                super::tools_reconcile::Mode::Bindings => keys.extend([
                    ("Enter", "Reconcile"),
                    ("Space", "Snapshot"),
                    ("Tab", "Tool"),
                    ("d", "Scope"),
                    ("Esc", "Back"),
                ]),
                super::tools_reconcile::Mode::Results => keys.extend([
                    (
                        "Enter",
                        if app.tools_view.reconciliation.details {
                            "Back"
                        } else {
                            "Details"
                        },
                    ),
                    ("PgDn", "Page"),
                    ("Esc", "Back"),
                ]),
            }
        } else if app.tools_view.setup && app.tools_view.details {
            keys.extend([("d/Esc", "Back"), ("PgDn", "Scroll"), ("Home", "Top")]);
        } else if app.tools_view.setup {
            keys.extend([
                ("Enter", "Inspect"),
                ("Space", "Select"),
                ("e", "Advanced"),
                ("d", "Scopes"),
                ("r", "Refresh"),
                ("Esc", "Back"),
            ]);
        } else if app.tools_view.catalog_browser {
            keys.extend([
                ("Enter", "Select"),
                ("x", "Forget metadata"),
                ("Esc", "Back"),
            ]);
        } else if app.tools_view.preview.is_some() {
            keys.extend([
                ("Enter", "Inspect once"),
                ("Esc", "Draft"),
                ("PgDn", "Query details"),
            ]);
        } else if app.tools_view.configuring {
            keys.extend([("Tab", "Field"), ("Enter", "Review"), ("Esc", "Back")]);
        } else if app.tools_view.searching {
            keys.extend([("Enter", "Done"), ("Esc", "Done"), ("Ctrl-U", "Clear")]);
        } else if app.tools_view.details && app.tools_view.report.is_some() {
            keys.extend([("Enter", "Back"), ("PgDn", "Scroll"), ("Esc", "Back")]);
        } else if app.tools_view.report.is_some() {
            keys.extend([
                ("Enter", "Details"),
                ("/", "Search"),
                ("r", "Fresh review"),
                ("F10", "Menu"),
            ]);
        } else {
            keys.extend([
                ("F10", "Menu"),
                ("c", "Inspect tools"),
                ("s", "Snapshots"),
                ("p", "Reconcile"),
                ("r", "Fresh review"),
            ]);
        }
    } else if app.text_entry() {
        keys.extend([("Enter", "Done"), ("Esc", "Back")]);
    } else {
        keys.insert(0, ("F10", "Menu"));
        keys.push((";", "Actions"));
        if app.tab == Tab::Files {
            if app.discovering && app.content.focused {
                keys.push(("Tab", "List"));
            } else {
                keys.push(("Space", "Mark"));
            }
        } else if app.tab == Tab::Git && app.git_view.sensitive() {
            keys = vec![("F1", "Help"), ("Esc", "Close"), ("PgDn", "Scroll")];
        }
    }
    keys
}
