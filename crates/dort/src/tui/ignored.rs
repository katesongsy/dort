//! Ignore picker. Enter previews; approval is separate.
use super::{
    components as ui,
    input::{Target, Wheel},
    workspace::{Region, Scope, Workspace},
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
    let details = |frame: &mut Frame, area, focused| {
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
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .report(
                frame,
                area,
                "Ignored path details",
                lines.into(),
                app.shell.scroll,
                focused,
            );
    };
    let [heading, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    frame.render_widget(
        Paragraph::new("Local HOME discovery preferences").style(ui::muted()),
        heading,
    );
    let active = Region::from_details(app.shell.picker_details);
    let workspace = Workspace::new(Scope::Ignored, body, active, paths.get(selected).is_some());
    workspace.bind(&app.hits);
    if let Some(pane) = workspace.details {
        details(frame, pane, active == Region::Details);
    }
    if active == Region::Details {
        app.presented("Back", "Esc", Target::Back);
    } else if paths.get(selected).is_some() {
        app.presented(
            "Unignore",
            "Enter",
            Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
    }
    let Some(list) = workspace.queue else { return };
    if paths.is_empty() {
        let block = ui::pane("Ignored paths", true);
        frame.render_widget(Paragraph::new("No ignored paths.").block(block), list);
        return;
    }
    let collection = Collection::new([Constraint::Percentage(100)], Header::None)
        .titled("Ignored paths", active == Region::Queue);
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
}
