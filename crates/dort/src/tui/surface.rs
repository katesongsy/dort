//! Navigation surface titles.
use super::{
    app::{App, Tab},
    shell::{Form, Overlay},
};

impl App {
    pub(super) fn dialog_title(&self) -> Option<&'static str> {
        if self.draft_exit.is_some() {
            return Some("Discard unsaved changes?");
        }
        if self.help.is_some() {
            return Some("Help");
        }
        if let Some(choice) = &self.choice {
            return Some(choice.title);
        }
        if let Some(menu) = &self.more {
            return Some(menu.title);
        }
        if matches!(self.shell.overlay, Some(Overlay::DraftAbout(_))) {
            return Some("About Dort");
        }
        if self.error.is_some() || self.busy || self.show_result || self.preview.is_some() {
            return None; // A retained choice is not the currently displayed surface.
        }
        match self.shell.overlay {
            Some(Overlay::Repositories { .. }) => Some("Choose repository"),
            Some(Overlay::Profiles(_)) => Some("Choose temporary profiles"),
            Some(Overlay::About) => Some("About Dort"),
            _ => None,
        }
    }
    pub(super) fn navigation_block(&self) -> Option<&'static str> {
        if self.busy {
            Some("Navigation paused while work completes; q/Ctrl-C waits for the owned worker.")
        } else if self.dialog_title().is_some() {
            Some("Dialog open: close or cancel it with Esc before navigating.")
        } else {
            None
        }
    }
    pub(super) fn page_title(&self) -> Option<&'static str> {
        if self.error.is_some() {
            return Some("Operation problem");
        }
        if self.busy {
            return Some("Activity / Work in progress");
        }
        if self.environment_review_open() {
            return Some(if self.tab == Tab::Overview {
                "Overview / Environment"
            } else {
                "Review environment"
            });
        }
        if self.preview.is_some() {
            return Some("Review changes");
        }
        if self.show_result {
            return Some("Operation result");
        }
        if let Some(editor) = self.batch_editor.as_ref().filter(|e| e.open) {
            return Some(if editor.details {
                "Files / Adoption details"
            } else if editor.item_form {
                "Files / Adopt / Edit item"
            } else {
                "Files / Adopt marked"
            });
        }
        match self.shell.overlay {
            Some(Overlay::Form(form)) => {
                if self.shell.form_details {
                    return Some("Draft details");
                }
                return Some(match form {
                    Form::Choose => "Session / Exact repository path",
                    Form::Clone => "Session / Clone repository",
                    Form::Repair => "Manifest repair draft",
                    Form::Generated => "Files / Generated file draft",
                    Form::TemplateFile if self.form_page().is_some() => {
                        "Files / Template file draft"
                    }
                    Form::TemplateFile => "Files / Template source",
                    Form::LocalInputs => "Files / Local inputs",
                    Form::Configure => self
                        .shell
                        .configuration
                        .as_ref()
                        .map_or("Edit manifest", |draft| draft.kind.label()),
                    Form::Init => "Session / Create or attach repository",
                    Form::Adopt => "Files / Adoption details",
                    Form::Check => "Checks / Standalone manifest",
                    Form::KeepId => "Files / Keep local file",
                    Form::RemoveId => "Files / Delete managed link",
                });
            }
            Some(Overlay::Ignored { .. }) => return Some("Files / Manage ignored paths"),
            Some(Overlay::Generated(_)) => return Some("Files / Generated file preview"),
            _ => (),
        }
        if self.tab == Tab::Files && (self.inventory.details || self.content.focused) {
            return Some(if self.content.focused {
                "Files / Selected content"
            } else {
                "Files / File details"
            });
        }
        if self.tab == Tab::Recovery && self.history_region == super::workspace::Region::Details {
            return Some("Recovery / Operation details");
        }
        if self.tab == Tab::Git && self.git_view.browsing_details() && !self.git_view.editing_root {
            return Some("Git / Details");
        }
        if self.tab == Tab::Overview {
            if self.profiles.open {
                return Some("Overview / Profiles");
            }
            if self.overview_details {
                return Some("Overview / Details");
            }
        }
        if self.tab == Tab::Tools {
            if let Some(title) = self.tools_view.declared.title() {
                return Some(title);
            }
            let t = &self.tools_view;
            return Some(if t.installed_open {
                "Tools / Installed packages"
            } else if t.declaration_open {
                match t.declaration.change {
                    None => "Tools / Add tool declaration",
                    Some(dort_core::tools::declaration::Change::Edit) => {
                        "Tools / Edit tool declaration"
                    }
                    Some(dort_core::tools::declaration::Change::Remove) => {
                        "Tools / Remove tool declaration"
                    }
                }
            } else if t.manifest_open {
                "Tools / Check manifest file"
            } else if t.settings_open {
                "Tools / Tool settings"
            } else if t.install_open {
                match t.installation.mode {
                    super::tools_install::Mode::Attempts => {
                        if t.installation.details {
                            "Tools / Install record"
                        } else {
                            "Tools / Installation history"
                        }
                    }
                    super::tools_install::Mode::CloseInputs => "Tools / Close record",
                    super::tools_install::Mode::LoadingClose
                    | super::tools_install::Mode::CloseReview => {
                        "Tools / Close installation record"
                    }
                }
            } else if t.declared_details {
                "Tools / Tool details"
            } else {
                "Tools / Declared tools"
            });
        }
        if self.tab == Tab::Git && self.git_view.editing_root {
            return Some(if self.git_view.root_metadata() {
                "Git / Draft details"
            } else {
                "Git / Choose inspection scope"
            });
        }
        None
    }
    /// Called only after choosing another destination. Retain drafts and
    /// historical metadata, discard one-use approvals.
    pub(super) fn leave_page_for_navigation(&mut self) {
        self.hits.invalidate_review();
        self.more = None;
        self.choice = None;
        self.preview = None;
        self.error = None;
        self.failure = None;
        self.retry = None;
        self.show_result = false;
        self.status = "".into();
        self.clear_template_form();
        self.shell.overlay = None;
        self.shell.return_to_adopt = false;
        self.profiles.open = false;
        self.tools_view.leave();
        self.git_view.leave_page();
        if let Some(editor) = &mut self.batch_editor {
            editor.open = false;
        }
        self.interaction_notice = None;
    }
}
