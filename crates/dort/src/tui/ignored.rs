//! Ignore picker. Enter previews; approval is separate.
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
    use super::input::Target;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    if paths.get(selected).is_some() || app.shell.picker_details {
        app.presented(
            if app.shell.picker_details {
                "Paths"
            } else {
                "Full path"
            },
            "F3",
            Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
        );
    }
    let details = |frame: &mut Frame, area| {
        let lines = vec![
            ui::metadata(
                "Saved ignored path",
                paths
                    .get(selected)
                    .and_then(|p| p.to_str())
                    .unwrap_or("No selection"),
            ),
            Line::raw("Parent rules and safety limits still apply."),
        ];
        ui::report(
            frame,
            area,
            "Ignored path details",
            lines.into(),
            app.shell.scroll,
            false,
        );
    };
    if app.shell.picker_details {
        details(frame, area);
        app.presented("Back", "Esc", Target::Back);
        return;
    }
    if paths.get(selected).is_some() {
        app.presented(
            "Unignore",
            "Enter",
            Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
    }
    let wide = super::workbench::wide_details(area, 1);
    let [heading, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(if wide { 0 } else { 2 }),
    ])
    .areas(area);
    let (list, pane) =
        super::workbench::detail_panes(body, super::workbench::wide_details(body, 0));
    frame.render_widget(
        Paragraph::new("Local HOME discovery preferences").style(ui::muted()),
        heading,
    );
    app.hits.wheel(list, super::input::Wheel::Choices);
    let capacity = usize::from(list.height.saturating_sub(1));
    let current = paths.get(selected).map(|_| selected);
    let first = app
        .shell
        .ignored_viewport
        .start(paths.len(), capacity, current);
    let rows = paths.iter().skip(first).take(capacity).map(|path| {
        Row::new(vec![ui::inline(
            path.to_str().expect("validated ignore path"),
        )])
    });
    let mut state = TableState::default().with_selected(current.and_then(|i| i.checked_sub(first)));
    frame.render_stateful_widget(
        Table::new(rows, [Constraint::Percentage(100)])
            .block(ui::pane("Ignored exact paths and descendants", true))
            .row_highlight_style(ui::selected())
            .highlight_symbol(format!("{} ", ui::glyphs::SELECTED)),
        list,
        &mut state,
    );
    for (y, index) in (first..paths.len()).take(capacity).enumerate() {
        app.hits.click(
            Rect::new(list.x, list.y + 1 + y as u16, list.width, 1),
            super::input::Target::Picker(index, false),
        );
    }
    if let Some(pane) = pane {
        details(frame, pane);
        return;
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
            Line::raw("Ancestor rules and safety exclusions still apply."),
        ]),
        footer,
    );
}
