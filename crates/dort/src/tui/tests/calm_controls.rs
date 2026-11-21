//! Foreground reading and joined-worker guards independent of Tools workflows.
use super::{
    actions::Task,
    app::{App, Tab},
    tests::render,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
#[test]
fn draft_git_metadata_is_complete_escaped_and_enter_cannot_read_hidden_fields() {
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = format!("/PREFIX/{}/\u{202e}\u{1b}[31m/TAIL", "path/".repeat(30));
    let draft = app.git_view.draft.clone();
    assert!(key(&mut app, KeyCode::F(3)).is_none());
    assert!(app.git_view.root_metadata());
    let mut seen = String::new();
    for _ in 0..30 {
        seen.push_str(&render(&app, 48, 14));
        assert!(key(&mut app, KeyCode::PageDown).is_none());
    }
    assert!(seen.contains("PREFIX") && seen.contains("TAIL"));
    assert!(seen.contains("\\u{202e}") && seen.contains("\\u{1b}"));
    assert!(!seen.contains('\u{202e}') && !seen.contains('\u{1b}'));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.error.is_none() && app.git_view.metadata.is_none());
    assert!(key(&mut app, KeyCode::Esc).is_none());
    assert!(!app.git_view.root_metadata() && app.git_view.editing_root);
    assert_eq!(app.git_view.draft, draft);
}
#[test]
fn foreground_work_navigation_explains_pause_without_losing_the_parent_draft() {
    let mut app = App::new(false);
    app.shell.overlay = Some(super::shell::Overlay::Form(super::shell::Form::Init));
    app.shell.init_draft = "/retained/draft".into();
    app.busy = true;
    for code in [KeyCode::Right, KeyCode::Left, KeyCode::Esc, KeyCode::Tab] {
        assert!(key(&mut app, code).is_none());
        assert!(app.tab == Tab::Overview && app.busy);
        assert!(app.interaction_notice.is_some());
        assert!(app.shell.overlay.is_some());
        assert_eq!(app.shell.init_draft, "/retained/draft");
    }
    assert!(key(&mut app, KeyCode::Char('q')).is_none());
    assert!(app.exit_requested && !app.quit && app.busy);
}
