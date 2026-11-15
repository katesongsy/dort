#[path = "declaration_choices.rs"]
mod choices;

use super::{
    actions::{self, Output, Task},
    app::{App, Tab},
    shell::Action,
    tests::render,
    views,
};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::Modifier,
};
use std::{fs, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "# retained\nschema=1\n").unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
        }
    }
    fn app(&self, dry: bool) -> App {
        let mut app = App::new(dry);
        app.tab = Tab::Tools;
        app.tools_view.declare();
        let f = &mut app.tools_view.declaration.fields;
        f[0] = "demo".into();
        f[1] = "demo".into();
        f[4] = "brew".into();
        f[5] = "demo".into();
        f[8] = "apt".into();
        f[9] = "demo".into();
        app
    }
    fn output(&self, task: Task) -> dort_core::Result<Output> {
        actions::perform_scoped(task, &self.host, None, &[], |_| Ok(()))
    }
    fn review(&self, app: &mut App) {
        app.tools_view.declaration.focus = 0;
        let task = key(app, KeyCode::Enter).unwrap();
        assert!(matches!(task, Task::ToolDeclaration(_)));
        app.complete(self.output(task).map_err(|e| actions::diagnostic(&e)));
        assert!(app.preview.is_some());
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
#[test]
fn declaration_real_preview_confirm_noop_and_all_repository_recovery_without_provider() {
    let f = Fixture::new();
    let mut app = f.app(false);
    f.review(&mut app);
    assert!(!app.preview.as_ref().unwrap().pending.adoption());
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "# retained\nschema=1\n"
    );
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_some());
    let task = key(&mut app, KeyCode::Enter).unwrap();
    app.start(true);
    let refresh = app
        .complete(f.output(task).map_err(|e| actions::diagnostic(&e)))
        .unwrap();
    app.complete(f.output(refresh).map_err(|e| actions::diagnostic(&e)));
    assert!(
        fs::read_to_string(f.repo.join("dort.toml"))
            .unwrap()
            .contains("[[tools]]")
    );
    assert_eq!(app.receipt.as_ref().unwrap().operation, "Tool declaration");
    app.show_result = false;
    f.review(&mut app);
    assert!(
        app.preview
            .as_ref()
            .unwrap()
            .text
            .contains("Already declared: true")
    );
    key(&mut app, KeyCode::Esc);
    app.activate(Action::Destination(Tab::Recovery));
    let history = app.data.as_ref().unwrap();
    assert_eq!(history.history[0].journal_schema, Some(5));
    assert!(app.page_actions().contains(&Action::Rollback));
    let task = app.activate(Action::Rollback).unwrap();
    app.complete(f.output(task).map_err(|e| actions::diagnostic(&e)));
    key(&mut app, KeyCode::Enter);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.output(task).unwrap();
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "# retained\nschema=1\n"
    );
}
#[test]
fn declaration_draft_focus_caret_and_buttons_survive_compact_wide_navigation() {
    let f = Fixture::new();
    let mut app = f.app(false);
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for focus in app.tools_view.declaration.controls() {
            app.tools_view.declaration.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| views::draw(frame, &app)).unwrap();
            if ![4, 8].contains(&focus) {
                let cursor = terminal.get_cursor_position().unwrap();
                super::tests::assert_text_focus(&terminal, (w, h), cursor);
            } else {
                assert!(
                    terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .any(|c| c.modifier.contains(Modifier::BOLD))
                );
                assert!(
                    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), w, h)
                        .is_none()
                );
                assert!(app.choice.is_some());
                assert!(
                    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), w, h)
                        .is_none()
                );
                assert_eq!(app.tools_view.declaration.focus, focus);
            }
            super::tests::assert_primary_action(&terminal);
        }
        app.tools_view.declaration.focus = 0;
        let count = app.tools_view.declaration.controls().len();
        for _ in 0..count {
            key(&mut app, KeyCode::Tab);
        }
        assert_eq!(app.tools_view.declaration.focus, 0);
    }
    app.tools_view.declaration.focus = 0;
    for c in "q?;a".chars() {
        assert!(key(&mut app, KeyCode::Char(c)).is_none());
    }
    assert!(!app.quit && app.help.is_none() && app.shell.overlay.is_none());
    key(&mut app, KeyCode::Esc);
    app.activate(Action::ToolDeclaration);
    assert_eq!(app.tools_view.declaration.fields[0], "demoq?;a");
}
#[test]
fn dry_run_shrink_cancel_and_late_preparation_cannot_approve_declaration() {
    let f = Fixture::new();
    let mut app = f.app(true);
    f.review(&mut app);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declaration_open);
    let mut app = f.app(false);
    f.review(&mut app);
    key(&mut app, KeyCode::Enter);
    app.resize(47, 13);
    assert!(app.confirmation.is_none());
    app.resize(80, 24);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_some());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    app.tools_view.declaration.focus = 12;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    app.start(false);
    key(&mut app, KeyCode::Left);
    assert!(app.tab == Tab::Tools && app.interaction_notice.is_some());
    app.go_tab(Tab::Git); // Actual navigation is separately guarded against late completion.
    assert!(app.tab != Tab::Tools);
    app.complete(f.output(task).map_err(|e| actions::diagnostic(&e)));
    assert!(app.preview.is_none());
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "# retained\nschema=1\n"
    );
    assert!(dort_core::journal::diagnose(&f.host).unwrap().is_empty());
}
#[test]
fn stale_declaration_write_discards_approval_and_retry_only_prepares_new_preview() {
    let f = Fixture::new();
    let mut app = f.app(false);
    f.review(&mut app);
    key(&mut app, KeyCode::Enter);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    fs::write(f.repo.join("dort.toml"), "# later\nschema=1\n").unwrap();
    app.start(true);
    app.complete(f.output(task).map_err(|e| actions::diagnostic(&e)));
    assert!(app.failed_write && app.preview.is_none() && app.confirmation.is_none());
    let retry = key(&mut app, KeyCode::Char('r')).unwrap();
    assert!(matches!(retry, Task::ToolDeclaration(_)));
    app.complete(f.output(retry).map_err(|e| actions::diagnostic(&e)));
    assert!(app.preview.is_some() && app.confirmation.is_none());
    assert!(render(&app, 80, 24).contains("Add tool declaration"));
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "# later\nschema=1\n"
    );
}
