//! Ignore picker. Enter previews; approval is separate.
use super::{
    components as ui,
    input::{Target, Wheel},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Paragraph, Row},
};
use std::path::PathBuf;
use ui::collection::{Collection, Header, RowTargets};

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &super::app::App,
    paths: &[PathBuf],
    selected: usize,
) {
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
    let collection = Collection::new([Constraint::Percentage(100)], Header::None)
        .titled("Ignored exact paths and descendants", true);
    let current = paths.get(selected).map(|_| selected);
    let window = collection.window(list, &app.shell.ignored_viewport, paths.len(), current);
    let rows = window.range.map(|index| {
        (
            index,
            Row::new([ui::inline(
                paths[index].to_str().expect("validated ignore path"),
            )]),
        )
    });
    collection.draw(
        frame,
        list,
        rows,
        window.selection,
        RowTargets {
            hits: &app.hits,
            wheel: Some(Wheel::Choices),
            target: |index| Target::Picker(index, false),
        },
    );
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
