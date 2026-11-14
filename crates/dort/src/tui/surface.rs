//! Explicit navigation surfaces. Editing text is never a reason to make a page modal.
use super::{
    app::{App, Tab},
    components as ui,
    input::Target,
    shell::{Form, Overlay},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::Line,
    widgets::{Clear, Paragraph},
};

impl App {
    pub(super) fn dialog_title(&self) -> Option<&'static str> {
        if self.help.is_some() {
            return Some("Help");
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
            Some(Overlay::Actions { .. }) => Some("Actions"),
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
        if self.tab == Tab::Tools {
            let t = &self.tools_view;
            return Some(if t.declaration_open {
                "Tools / Add tool declaration"
            } else if t.install_open {
                "Tools / Installation and attempts"
            } else if t.recon_open {
                "Tools / Compare declarations"
            } else if t.setup {
                "Tools / Inspect installed tools"
            } else if t.configuring {
                "Tools / Advanced inventory scope"
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
            return Some("Git / Choose inspection scope");
        }
        None
    }
    /// Called only after selecting a menu destination, never opening/dismissing
    /// a menu. Retain drafts and historical metadata, discard one-use approvals.
    pub(super) fn leave_page_for_navigation(&mut self) {
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
/// A workflow page has an explicit breadcrumb and Back. Its menus remain active.
pub(super) fn page(frame: &mut Frame, area: Rect, app: &App, title: &'static str) -> Rect {
    let back = if app.busy {
        ui::button("Work running", "q waits", false)
    } else {
        ui::button("Back", "Esc", false)
    };
    let w = (back.width() as u16).min(area.width);
    let button = Rect::new(area.right().saturating_sub(w), area.y, w, 1);
    frame.render_widget(
        Paragraph::new(ui::chrome_clip(
            &format!("Page · {title}"),
            area.width.saturating_sub(w),
        )),
        Rect::new(area.x, area.y, area.width.saturating_sub(w), 1),
    );
    frame.render_widget(Paragraph::new(Line::from(back)), button);
    app.hits.click(
        button,
        if app.busy {
            Target::NavigationBlocked
        } else {
            Target::Back
        },
    );
    Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    )
}
/// True dialogs dim and capture the entire background. Only their explicit
/// controls can act; clicking anywhere else explains the navigation restriction.
pub(super) fn dialog(frame: &mut Frame, app: &App, title: &'static str, cancel: bool) -> Rect {
    let area = frame.area();
    for cell in frame.buffer_mut().content.iter_mut() {
        cell.set_style(
            cell.style()
                .remove_modifier(Modifier::BOLD | Modifier::REVERSED)
                .add_modifier(Modifier::DIM),
        );
    }
    app.hits.clear();
    app.hits.input_focus(true); // Only the foreground dialog may own a caret.
    app.hits.click(area, Target::NavigationBlocked);
    let width = area.width.saturating_sub(2).min(94);
    let height = area.height.saturating_sub(2).min(28);
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, modal);
    let block = ui::pane(&format!("Dialog · {title}"), true);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let close = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(ui::button(
            if app.shell.picker_details
                && app.help.is_none()
                && matches!(
                    app.shell.overlay,
                    Some(
                        Overlay::Profiles(_)
                            | Overlay::Repositories { .. }
                            | Overlay::Actions { .. }
                    )
                )
            {
                "Choices"
            } else if cancel {
                "Cancel"
            } else {
                "Close"
            },
            "Esc",
            false,
        ))),
        close,
    );
    app.hits.click(
        close,
        if app.confirmation.is_some() && app.help.is_none() {
            Target::CancelConfirmation
        } else {
            Target::Back
        },
    );
    let hint = app
        .interaction_notice
        .unwrap_or("Dialog · background navigation paused · Esc closes");
    ui::chrome_prose(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
        hint,
        ui::warning(),
    );
    Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    )
}
