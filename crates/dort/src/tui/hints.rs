//! Contextual keyboard hints for the visible surface. No payloads or approvals.
use super::{
    app::{App, Tab},
    shell::Overlay,
};

pub fn keys(app: &App) -> Vec<(&'static str, &'static str)> {
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
        keys.extend([("Enter", "Submit"), ("Esc", "Back")]);
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
        keys.extend([
            (
                "Enter",
                if editor.item_form && editor.focused.saturating_sub(editor.item * 3) >= 3 {
                    "Done"
                } else if editor.item_form || editor.focused < editor.drafts.len() * 3 {
                    "Next"
                } else {
                    "Review all"
                },
            ),
            ("Esc", "Back"),
        ]);
    } else if let Some(overlay) = &app.shell.overlay {
        match overlay {
            Overlay::Ignored { paths, selected } => {
                if *selected < paths.len() {
                    keys.push(("Enter", "Unignore"));
                }
                keys.push(("Esc", "Back"));
            }
            Overlay::Actions { searching, .. } => {
                keys.extend([
                    ("Enter", "Choose"),
                    ("Esc", if *searching { "Results" } else { "Back" }),
                ]);
            }
            Overlay::Repositories { .. } => keys.extend([("Enter", "Select"), ("Esc", "Back")]),
            Overlay::Profiles(picker) => {
                if picker.selected == picker.report.choices.known.len() {
                    keys.push(("Enter", "Apply"));
                } else if picker
                    .report
                    .choices
                    .known
                    .iter()
                    .nth(picker.selected)
                    .is_some_and(|name| !picker.report.choices.mandatory.contains(name))
                {
                    keys.push(("Space", "Toggle"));
                }
                keys.push(("Esc", "Back"));
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
