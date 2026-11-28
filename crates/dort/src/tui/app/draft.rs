use super::{Action, App, Form, Overlay, Tab, Task};
use crate::tui::navigation::Place;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone, Copy)]
pub(in crate::tui) enum DraftExit {
    Back,
    Navigate(Place),
    Open(Action),
    Recovery,
    Quit,
}

impl DraftExit {
    pub fn message(self) -> String {
        let destination = match self {
            Self::Back => return "Discard your changes and go back?".into(),
            Self::Quit => return "Discard your changes and quit Dort?".into(),
            Self::Navigate(Place::Manifest) => "Checks / Manifest",
            Self::Navigate(Place::Files(page)) => page.location(),
            Self::Navigate(Place::Destination(tab)) => Action::Destination(tab).label(),
            Self::Navigate(Place::Action(action)) | Self::Open(action) => action.label(),
            Self::Recovery => "Recovery",
        };
        format!("Discard your changes?\nDestination: {destination}")
    }
}

impl App {
    pub(in crate::tui) fn template_form(&self) -> Option<Form> {
        match self.shell.overlay {
            Some(Overlay::Form(form @ (Form::Generated | Form::LocalInputs)))
            | Some(Overlay::DraftAbout(form)) => Some(form),
            _ => None,
        }
    }

    pub(super) fn template_form_visible(&self) -> bool {
        matches!(
            self.shell.overlay,
            Some(Overlay::Form(Form::Generated | Form::LocalInputs))
        ) && self.dialog_title().is_none()
            && !self.busy
            && self.error.is_none()
            && !self.show_result
            && self.preview.is_none()
    }

    pub(in crate::tui) fn draft_continue_key(&self) -> &'static str {
        if self.form_has_focus() { "" } else { "Enter" }
    }

    pub(in crate::tui) fn defer_draft_exit(&mut self, exit: DraftExit) -> bool {
        if self.draft_exit.is_some() {
            return true;
        }
        let dirty = match self.template_form() {
            Some(Form::Generated) => self
                .shell
                .generated
                .as_ref()
                .is_some_and(|draft| draft.dirty()),
            Some(Form::LocalInputs) => self
                .shell
                .local_inputs
                .as_ref()
                .is_some_and(|panel| panel.dirty()),
            _ => false,
        };
        if dirty {
            self.draft_exit = Some(exit);
            self.hits.invalidate_review();
            self.hits.clear();
        }
        dirty
    }

    pub(in crate::tui) fn clear_template_form(&mut self) {
        if self.template_form().is_some() {
            self.shell.generated = None;
            self.shell.local_inputs = None;
            self.shell.form_feedback = None;
            self.shell.open_template_editor = false;
            self.form_focus = None;
        }
    }

    pub(super) fn leave_template_form(&mut self) {
        if self.defer_draft_exit(DraftExit::Back) {
            return;
        }
        self.back_from_template_form();
    }

    fn back_from_template_form(&mut self) {
        self.shell.form_feedback = None;
        if self
            .shell
            .local_inputs
            .as_mut()
            .is_some_and(|panel| panel.back())
        {
            return;
        }
        self.clear_template_form();
        self.shell.back();
    }

    pub(super) fn confirm_draft_exit(&mut self, key: KeyEvent) -> Option<Task> {
        if !key.modifiers.is_empty() {
            return None;
        }
        let exit = match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                self.draft_exit = None;
                return None;
            }
            KeyCode::Char('d') => self.draft_exit.take()?,
            _ => return None,
        };
        match exit {
            DraftExit::Back => {
                self.back_from_template_form();
                None
            }
            DraftExit::Navigate(place) => {
                self.clear_template_form();
                self.go_place(place)
            }
            DraftExit::Open(action) => {
                self.leave_page_for_navigation();
                self.activate(action)
            }
            DraftExit::Recovery => self.recover_from_error(),
            DraftExit::Quit => {
                self.clear_template_form();
                self.request_quit();
                None
            }
        }
    }

    pub(super) fn request_quit(&mut self) {
        self.git_view.discard_preview();
        if self.busy {
            self.exit_requested = true;
            self.status =
                "Exit requested; waiting for the active worker. Approved writes run to completion."
                    .into();
        } else {
            self.quit = true;
        }
    }

    pub(super) fn recover_from_error(&mut self) -> Option<Task> {
        self.error = None;
        self.preview = None;
        self.retry = None;
        self.clear_template_form();
        self.shell.overlay = None;
        self.tab = Tab::Recovery;
        self.details_scroll = 0;
        Some(Task::History)
    }
}
