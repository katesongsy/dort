//! Cell-geometry and semantic style checks, using synthetic metadata only.
use super::{
    actions::{Data, ManagedRow, Output},
    app::{App, Tab},
    shell::{Focus, Form, Overlay},
    tests::render,
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Modifier},
};
fn key(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24);
}
fn buffer(app: &App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| views::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn locate(buffer: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        if let Some(byte) = row.find(needle) {
            return (row[..byte].chars().count() as u16, y);
        }
    }
    panic!("missing fixture label: {needle}")
}
fn files_app() -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        repository: Some("/synthetic/repository".into()),
        files: vec![("editor".into(), "fallback must not be parsed".into())],
        managed: vec![ManagedRow {
            id: "editor".into(),
            source: "/synthetic/repository/files/editor".into(),
            target: "/synthetic/home/.editor".into(),
            state: "Blocked",
            source_kind: "Regular file",
            target_kind: "Directory",
            blockers: vec!["synthetic blocker".into()],
        }],
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app
}
#[test]
fn workbench_has_semantic_styles_and_selected_identity_at_all_supported_sizes() {
    let app = files_app();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        let active = locate(&b, "2 Files");
        assert!(b[active].modifier.contains(Modifier::BOLD));
        assert!(b[active].modifier.contains(Modifier::REVERSED));
        locate(&b, "STATE");
        let (_, row_y) = locate(&b, "> editor");
        let row: String = (0..w).map(|x| b[(x, row_y)].symbol()).collect();
        let byte = row
            .find("Blocked")
            .expect("selected row keeps its status column");
        let status = (row[..byte].chars().count() as u16, row_y);
        assert_eq!(
            b[status].bg,
            Color::Cyan,
            "selected row background at {w}x{h}"
        );
        assert_eq!(
            b[status].fg,
            Color::Black,
            "selected row foreground at {w}x{h}"
        );
        let title = locate(&b, "Focus");
        assert_eq!(b[title].fg, Color::Cyan);
        locate(&b, "Review apply");
        // Chrome occupies two normal rows or three compact rows, not nine.
        assert!(locate(&b, "Files / Managed").1 <= 3);
        assert!(!render(&app, w, h).contains("fallback must not be parsed"));
    }
}
#[test]
fn input_has_a_real_visible_caret_and_field_button_hierarchy() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    app.shell.adoption.target = "~/.editor".into();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let b = t.backend().buffer();
        let (x, y) = locate(b, "~/.editor");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 9, y).into());
        let label = locate(t.backend().buffer(), "TARGET");
        assert_eq!(t.backend().buffer()[label].fg, Color::Cyan);
    }
    app.shell.adoption.focused = 3;
    let b = buffer(&app, 48, 14);
    let action = locate(&b, "> [Review");
    assert!(b[action].modifier.contains(Modifier::REVERSED));
    assert_ne!(b[action].bg, Color::Reset);
}
#[test]
fn arrows_navigate_visible_focused_controls_not_hidden_global_tabs() {
    let mut app = files_app();
    key(&mut app, KeyCode::Right);
    assert!(app.tab == Tab::Files);
    key(&mut app, KeyCode::Tab);
    assert!(app.shell.focus == Focus::Actions);
    key(&mut app, KeyCode::Right);
    assert_eq!(app.action_selected, 1);
    assert!(render(&app, 80, 24).contains("> [Review apply"));
    key(&mut app, KeyCode::Tab);
    assert!(app.shell.focus == Focus::Destinations);
    key(&mut app, KeyCode::Right);
    assert!(app.tab == Tab::Recovery);
}
#[test]
fn single_path_field_cannot_submit_until_the_visible_action_is_activated() {
    let mut app = App::new(false);
    app.git = Ok("/trusted/git".into());
    key(&mut app, KeyCode::Char('n'));
    app.shell.init_draft = "/synthetic/new-repository".into();
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24)
            .is_none()
    );
    assert_eq!(app.shell.path_focused, 1);
    assert!(render(&app, 80, 24).contains("> [Review read-only preview"));
    assert!(matches!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24),
        Some(super::actions::Task::Init { .. })
    ));
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
