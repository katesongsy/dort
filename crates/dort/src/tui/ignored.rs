//! Read-only preference picker. Enter requests a preview, never approval.
use super::components as ui;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Paragraph, Row, Table, TableState},
};
use std::path::PathBuf;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &super::app::App,
    paths: &[PathBuf],
    selected: usize,
) {
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new("Local HOME discovery preferences\nEnter previews Unignore · Esc returns")
            .style(ui::muted()),
        parts[0],
    );
    app.hits.wheel(parts[1], super::input::Wheel::Choices);
    if !paths.is_empty() {
        app.hits
            .click(parts[0], super::input::Target::Picker(selected, true));
    }
    let rows = paths.iter().map(|path| {
        Row::new(vec![ui::inline(
            path.to_str().expect("validated ignore path"),
        )])
    });
    let mut state = TableState::default().with_selected((!paths.is_empty()).then_some(selected));
    frame.render_stateful_widget(
        Table::new(rows, [Constraint::Percentage(100)])
            .block(ui::pane("Ignored exact paths and descendants", true))
            .row_highlight_style(ui::accent())
            .highlight_symbol("> "),
        parts[1],
        &mut state,
    );
    for (y, index) in (state.offset()..paths.len())
        .take(usize::from(parts[1].height.saturating_sub(2)))
        .enumerate()
    {
        app.hits.click(
            Rect::new(
                parts[1].x + 1,
                parts[1].y + 1 + y as u16,
                parts[1].width.saturating_sub(2),
                1,
            ),
            super::input::Target::Picker(index, false),
        );
    }
    let detail = paths
        .get(selected)
        .map(|p| {
            format!(
                "Selected: {}",
                ui::inline(p.to_str().expect("validated ignore path"))
            )
        })
        .unwrap_or_else(|| "No ignored paths.".into());
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(detail),
            Line::raw("↑/↓ j/k · PgUp/PgDn · Home/End"),
            Line::raw("Ancestor rules and safety exclusions still apply."),
        ]),
        parts[2],
    );
}
