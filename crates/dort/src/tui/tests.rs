use super::{
    actions::{Data, Output},
    app::{App, Tab},
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

// Invoked only by an isolated PTY child for terminal cleanup checks; never a runtime switch.
#[test]
fn terminal_cleanup_probe() {
    let Ok(mode) = std::env::var("DORT_TEST_TERMINAL_CLEANUP") else {
        return;
    };
    let result = super::terminal_session(|terminal, _| {
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new("cleanup probe"),
                    frame.area(),
                )
            })
            .unwrap();
        match mode.as_str() {
            "panic" => panic!("synthetic terminal panic"),
            "error" => Err(dort_core::Error::Invalid(
                "synthetic terminal I/O failure".into(),
            )),
            _ => Ok(()),
        }
    });
    if mode == "error" {
        assert!(result.is_err());
    } else {
        result.unwrap();
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
pub(super) fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| views::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn small_layout_snapshot_disables_approval() {
    let mut app = App::new(false);
    assert_eq!(
        render(&app, 40, 6),
        "Dort: terminal too small\nResize to 48x14; approval disabled.\nq/Ctrl-C: exit (waits for worker)\n\n\n"
    );
    assert!(app.key(key(KeyCode::Enter), 40, 6).is_none());
    assert!(!app.busy);
    app.key(key(KeyCode::Char('q')), 40, 6);
    assert!(app.quit);
}
#[test]
fn quit_during_work_waits_for_completion() {
    let mut app = App::new(false);
    app.start(true);
    app.key(key(KeyCode::Char('q')), 80, 24);
    assert!(app.exit_requested);
    assert!(!app.quit);
    assert!(app.key(key(KeyCode::Char('a')), 80, 24).is_none());
    assert!(app.complete(Ok(Output::Completed)).is_none());
    assert!(app.quit);
}
#[test]
fn navigation_and_metadata_escaping() {
    let mut app = App::new(true);
    app.complete(Ok(Output::Data(Data {
        overview: "no repository\n\u{1b}[31m\u{202e}".into(),
        files: vec![],
        history: vec![],
        history_error: None,
    })));
    let screen = render(&app, 80, 24);
    assert!(screen.contains("DRY RUN"));
    assert!(screen.contains("\\u{1b}[31m\\u{202e}"));
    app.key(key(KeyCode::Tab), 80, 24);
    assert!(app.tab == Tab::Files);
    app.key(key(KeyCode::Down), 80, 24);
    assert_eq!(app.selected, 0);
    assert!(render(&app, 48, 14).contains("0 active mapping(s)"));
}
