//! Shared page/dialog framing. Navigation and approval ownership remain in App.
use super::{self as ui};
use crate::tui::{app::App, input::Target, shell::Overlay};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph},
};

/// A workflow page has an explicit breadcrumb and Back. Its menus remain active.
pub(in crate::tui) fn page(frame: &mut Frame, area: Rect, app: &App, title: &'static str) -> Rect {
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
pub(in crate::tui) fn dialog(
    frame: &mut Frame,
    app: &App,
    title: &'static str,
    cancel: bool,
) -> Rect {
    let area = frame.area();
    for cell in frame.buffer_mut().content.iter_mut() {
        cell.set_style(super::design::backdrop(cell.style()));
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
