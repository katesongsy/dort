//! Explicit navigation surfaces. Editing text is never a reason to make a page modal.
use super::{
    app::{App, Tab},
    shell::{Form, Overlay},
};

impl App {
    pub(super) fn dialog_title(&self) -> Option<&'static str> {
        if self.help.is_some() {
            return Some("Help");
        }
        if let Some(choice) = &self.choice {
            return Some(choice.title);
        }
        if let Some(menu) = &self.more {
            return Some(menu.title);
        }
        if self.navigation.menu.is_some() {
            return Some("Places");
        }
        if self.confirmation.is_some() {
            return Some("Confirm reviewed changes");
        }
        if self.tab == Tab::Git && self.git_view.disclosure_pending() {
            return Some("View Git patch content");
        }
        if self.error.is_some() || self.busy || self.show_result || self.preview.is_some() {
            return None; // A retained choice is not the currently displayed surface.
        }
        match self.shell.overlay {
            Some(Overlay::Repositories { .. }) => Some("Choose repository"),
            Some(Overlay::Profiles(_)) => Some("Choose temporary profiles"),
            Some(Overlay::Actions { .. }) => Some(if self.shell.global_commands {
                "Commands"
            } else {
                "Actions"
            }),
            Some(Overlay::About) => Some("About Dort"),
            _ => None,
        }
    }
    pub(super) fn navigation_block(&self) -> Option<&'static str> {
        if self.busy {
            Some("Menus paused while work completes; q/Ctrl-C waits for the owned worker.")
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
        if self.preview.is_some() {
            return Some("Review changes");
        }
        if self.show_result {
            return Some("Operation result");
        }
        if let Some(editor) = self.batch_editor.as_ref().filter(|e| e.open) {
            return Some(if editor.details {
                "Files / Adoption metadata"
            } else if editor.item_form {
                "Files / Adopt / Edit item"
            } else {
                "Files / Adopt marked"
            });
        }
        match self.shell.overlay {
            Some(Overlay::Form(form)) => {
                if self.shell.form_details {
                    return Some("Draft metadata");
                }
                return Some(match form {
                    Form::Choose => "Session / Exact repository path",
                    Form::Init => "Session / Create or attach repository",
                    Form::Adopt => "Files / Adoption details",
                    Form::Check => "Checks / Standalone manifest",
                    Form::KeepId => "Files / Keep local file",
                    Form::RemoveId => "Files / Delete managed link",
                });
            }
            Some(Overlay::Ignored { .. }) => return Some("Files / Manage ignored paths"),
            _ => (),
        }
        if self.tab == Tab::Files && (self.inventory.details || self.content.focused) {
            return Some(if self.content.focused {
                "Files / Selected content"
            } else {
                "Files / File details"
            });
        }
        if self.tab == Tab::Recovery && self.history_details {
            return Some("Recovery / Transaction details");
        }
        if self.tab == Tab::Git
            && self.git_view.browsing_details()
            && !self.git_view.sensitive()
            && !self.git_view.editing_root
        {
            return Some("Git / Metadata details");
        }
        if self.tab == Tab::Overview && self.overview_details {
            return Some("Overview / Details");
        }
        if self.tab == Tab::Tools {
            let t = &self.tools_view;
            return Some(if t.declaration_open {
                "Tools / Add tool declaration"
            } else if t.install_open && t.installation.draft_metadata() {
                "Tools / Draft input metadata"
            } else if t.install_open {
                match t.installation.mode {
                    super::tools_install::Mode::Attempts => "Tools / Installation attempts",
                    super::tools_install::Mode::Review => "Tools / Review installation",
                    super::tools_install::Mode::Result => "Tools / Installation outcome",
                    super::tools_install::Mode::CloseInputs
                    | super::tools_install::Mode::LoadingClose
                    | super::tools_install::Mode::CloseReview => "Tools / Close attempt",
                    super::tools_install::Mode::PrefixInputs
                    | super::tools_install::Mode::AptInputs => "Tools / Installation inputs",
                    super::tools_install::Mode::LoadingInstall => {
                        "Tools / Prepare installation review"
                    }
                }
            } else if t.recon_open {
                if t.reconciliation.binding_metadata() {
                    "Tools / Comparison metadata"
                } else {
                    "Tools / Compare declarations"
                }
            } else if t.setup {
                if t.details {
                    "Tools / Inspection scopes"
                } else {
                    "Tools / Inspect installed tools"
                }
            } else if t.configuring {
                if t.config_details {
                    "Tools / Draft inspection details"
                } else {
                    "Tools / Inspection settings"
                }
            } else if t.preview.is_some() {
                "Tools / Review inventory query"
            } else if t.catalog_browser {
                "Tools / Session snapshots"
            } else if t.details {
                "Tools / Inventory details"
            } else {
                return None;
            });
        }
        if self.tab == Tab::Git && self.git_view.editing_root {
            return Some(if self.git_view.root_metadata() {
                "Git / Draft scope metadata"
            } else {
                "Git / Choose inspection scope"
            });
        }
        None
    }
    /// Called only after selecting a menu destination, never opening/dismissing
    /// a menu. Retain drafts and historical metadata, discard one-use approvals.
    pub(super) fn leave_page_for_navigation(&mut self) {
        self.more = None;
        self.choice = None;
        self.shell.global_commands = false;
        self.preview = None;
        self.confirmation = None;
        self.error = None;
        self.failure = None;
        self.retry = None;
        self.show_result = false;
        self.shell.overlay = None;
        self.shell.return_to_adopt = false;
        self.tools_view.leave();
        self.git_view.leave_page();
        if let Some(editor) = &mut self.batch_editor {
            editor.open = false;
        }
        self.interaction_notice = None;
    }
}
pub(super) use super::components::surface::dialog;
