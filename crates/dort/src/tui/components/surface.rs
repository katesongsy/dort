//! Shared foreground frame. Workflows and approval ownership remain in App.
use crate::tui::app::App;
use ratatui::{Frame, layout::Rect};

pub(in crate::tui) fn dialog(
    frame: &mut Frame,
    app: &App,
    title: &'static str,
    _cancel: bool,
) -> Rect {
    let inner = super::design::choice_dialog(frame, title, &app.hits);
    app.hits.input_focus(true); // Only the active foreground may own an input caret.
    inner
}
