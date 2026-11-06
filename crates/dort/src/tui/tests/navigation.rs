//! Files navigation and click semantics use typed rows, never displayed labels.
use super::{
    actions::{Data, Output},
    app::{App, Tab},
    input::Target,
    tests::render,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::{Duration, Instant};

fn app() -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        files: (0..100)
            .map(|i| (format!("/synthetic/.row-{i:03}"), "metadata".into()))
            .collect(),
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app.resize(140, 40);
    render(&app, 140, 40);
    app
}
fn key(app: &mut App, code: KeyCode) {
    assert!(
        app.key(KeyEvent::new(code, KeyModifiers::NONE), 140, 40)
            .is_none()
    );
}
fn ctrl(app: &mut App, c: char) {
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
            140,
            40
        )
        .is_none()
    );
}
#[test]
fn vim_pages_use_the_rendered_viewport_and_chords_reset() {
    let mut app = app();
    let page = app.inventory.page_capacity.get();
    assert!(page > 1);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.selected, 1);
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(app.selected, 0);
    ctrl(&mut app, 'f');
    assert_eq!(app.selected, page);
    ctrl(&mut app, 'd');
    assert_eq!(app.selected, page + page / 2);
    ctrl(&mut app, 'u');
    assert_eq!(app.selected, page);
    ctrl(&mut app, 'b');
    assert_eq!(app.selected, 0);
    key(&mut app, KeyCode::Char('G'));
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.selected, 0);
    key(&mut app, KeyCode::Char('G'));
    key(&mut app, KeyCode::Char('g'));
    app.resize(80, 24);
    app.resize(140, 40);
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Char(' '));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    assert!(app.shell.overlay.is_none());
    key(&mut app, KeyCode::Char(';'));
    assert!(app.shell.overlay.is_some());
    key(&mut app, KeyCode::Char('g'));
    assert!(!app.inventory.pending_g);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('/'));
    for c in ['g', 'g', ' ', ';', 'j', 'k'] {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.inventory.managed.query, "gg ;jk");
    assert!(!app.inventory.pending_g && !app.quit);
}
fn row_point(app: &App, raw: usize) -> (u16, u16) {
    (0..40)
        .flat_map(|y| (0..70).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x,y),Some(Target::Row(i)) if i==raw))
        .unwrap()
}
fn click(app: &mut App, point: (u16, u16), at: Instant) {
    assert!(
        app.mouse_at(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: point.0,
                row: point.1,
                modifiers: KeyModifiers::NONE
            },
            140,
            40,
            at
        )
        .is_none()
    );
}
#[test]
fn double_click_marks_once_without_a_draw_and_keyboard_timeout_resize_invalidate() {
    let mut app = app();
    let now = Instant::now();
    let point = row_point(&app, 1);
    click(&mut app, point, now);
    assert_eq!(app.selected, 1);
    assert!(app.inventory.managed_marks.is_empty());
    click(&mut app, point, now + Duration::from_millis(100));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    assert!(app.preview.is_none() && app.confirmation.is_none());
    render(&app, 140, 40);
    let point = row_point(&app, 2);
    click(&mut app, point, now);
    click(&mut app, point, now + Duration::from_millis(401));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    key(&mut app, KeyCode::Char('j'));
    render(&app, 140, 40);
    click(&mut app, point, now + Duration::from_millis(450));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    app.resize(80, 24);
    app.resize(140, 40);
    render(&app, 140, 40);
    click(&mut app, point, now + Duration::from_millis(500));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    click(&mut app, point, now + Duration::from_millis(550));
    assert_eq!(app.inventory.managed_marks.len(), 2);
}
