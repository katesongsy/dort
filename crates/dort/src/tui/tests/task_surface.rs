//! Actual input/cell tests for metadata-only task feedback; no filesystem access.
use super::{
    actions::{Completion, Data, Output, Retry, Task},
    app::{App, Tab},
    task_surfaces::{Activity, Progress},
    tests::render,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    style::Color,
};
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 48, 14)
}
fn click(app: &mut App, label: &str) -> Option<Task> {
    let view = render(app, 48, 14);
    let (x, y) = view
        .lines()
        .enumerate()
        .find_map(|(y, row)| {
            row.find(label)
                .map(|x| (row[..x].chars().count() as u16, y as u16))
        })
        .expect("visible typed feedback button");
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        48,
        14,
    )
}
fn task_cells(app: &mut App) -> String {
    let mut joined = String::new();
    for _ in 0..45 {
        let view = render(app, 48, 14);
        let rows = view.lines().collect::<Vec<_>>();
        let top = rows.iter().position(|row| row.starts_with('┌')).unwrap();
        let viewport = rows
            .iter()
            .skip(top + 2)
            .take_while(|row| !row.starts_with('└'))
            .flat_map(|row| row.chars().skip(1).take(46))
            .collect::<String>();
        let overlap = (0..=viewport.len().min(joined.len()))
            .rev()
            .find(|n| joined.ends_with(&viewport[..*n]))
            .unwrap();
        joined.push_str(&viewport[overlap..]);
        assert!(key(app, KeyCode::PageDown).is_none());
    }
    key(app, KeyCode::Home);
    joined
}
fn begin(app: &mut App, task: &Task, writing: bool) {
    app.activity = Some(Activity::for_task(task, app));
    app.start(writing);
}
#[test]
fn activity_keeps_scope_and_real_boundary_through_navigation_and_quit_waits() {
    let mut app = App::new(false);
    app.repository = Some("/synthetic/immutable-root/long-segment/scope-end".into());
    app.requested_profiles = vec!["dev-original".into()];
    begin(&mut app, &Task::Refresh, true);
    app.activity.as_mut().unwrap().progress = Some(Progress {
        transaction_id: "b".repeat(64),
        boundary: "Durable checkpoint",
        sequence: 17,
    });
    let first = render(&app, 48, 14);
    assert!(first.contains("Approved execution") && first.contains("quit waits"));
    assert!(key(&mut app, KeyCode::Right).is_none());
    assert!(app.tab == Tab::Files);
    for code in [
        KeyCode::Char('a'),
        KeyCode::Char('r'),
        KeyCode::Enter,
        KeyCode::Tab,
    ] {
        assert!(key(&mut app, code).is_none());
    }
    let mut pages = String::new();
    for _ in 0..30 {
        pages.push_str(&render(&app, 48, 14));
        assert!(key(&mut app, KeyCode::PageDown).is_none());
    }
    assert!(pages.contains("scope-end") && pages.contains("dev-original"));
    assert!(pages.contains("Durable checkpoint") && pages.contains("17"));
    key(&mut app, KeyCode::Home);
    let cells = task_cells(&mut app);
    assert!(cells.contains(&"b".repeat(64)));
    assert!(cells.contains("/synthetic/immutable-root/long-segment/scope-end"));
    assert!(!pages.contains("100%") && !pages.contains("Cancel"));
    key(&mut app, KeyCode::Home);
    assert!(render(&app, 48, 14).contains("immutable-root"));
    app.shell.overlay = Some(super::shell::Overlay::Form(super::shell::Form::Adopt));
    key(&mut app, KeyCode::Char('q'));
    assert!(app.exit_requested && !app.quit);
    assert!(render(&app, 48, 14).contains("Exit requested"));
}
#[test]
fn read_failure_buttons_retry_only_retained_intent_and_back_retains_draft() {
    let mut app = App::new(false);
    let raw = std::path::PathBuf::from("/synthetic/q?manifest\u{1b}[31m");
    app.shell.choose_draft = "retained-q?/scope".into();
    let task = Task::Check {
        manifest: Some(raw.clone()),
        requested: vec!["dev".into()],
    };
    app.retry = task.retry_intent();
    begin(&mut app, &task, false);
    app.complete(Err("sanitized failure\nsecond diagnostic line".into()));
    let text = render(&app, 48, 14);
    assert!(text.contains("Read failed:") && text.contains("sanitized failure"));
    let retry = click(&mut app, "[Retry read").unwrap();
    assert!(matches!(retry, Task::Check { manifest: Some(path), .. } if path == raw));
    app.retry = Some(Retry::Refresh);
    begin(&mut app, &Task::Refresh, false);
    app.complete(Err("again".into()));
    assert!(click(&mut app, "[Back").is_none());
    assert!(app.error.is_none());
    assert_eq!(app.shell.choose_draft, "retained-q?/scope");
    assert!(!app.write_warning);
    app.shell.overlay = Some(super::shell::Overlay::Form(super::shell::Form::Choose));
    begin(&mut app, &Task::Refresh, false);
    app.complete(Err("failure above retained form".into()));
    key(&mut app, KeyCode::Char('q'));
    assert!(app.quit, "an error panel is not a text editor");
}
#[test]
fn unresolved_active_root_is_not_presented_as_a_frozen_old_observation() {
    let mut app = App::new(false);
    app.data = Some(Data {
        repository: Some("/synthetic/old-observed-root".into()),
        ..Default::default()
    });
    app.requested_profiles = vec!["prior-session".into()];
    let activity = Activity::for_task(
        &Task::Check {
            manifest: None,
            requested: vec!["actual-request".into()],
        },
        &app,
    );
    assert!(
        activity
            .scope
            .iter()
            .any(|(label, _)| *label == "Selection")
    );
    assert!(activity.scope.iter().any(|(label, value)| *label == "Prior root" && value == "/synthetic/old-observed-root"));
    assert!(
        !activity
            .scope
            .iter()
            .any(|(label, _)| *label == "Repository")
    );
    assert!(
        activity
            .scope
            .iter()
            .any(|(label, value)| *label == "Additions" && value == "actual-request")
    );
    let selection = Activity::for_task(&Task::Select(None), &app);
    assert!(
        !selection
            .scope
            .iter()
            .any(|(label, _)| *label == "Additions" || *label == "Prior root")
    );
}
#[test]
fn failed_write_pins_risk_after_back_and_reads_and_never_retries_execution() {
    let mut app = App::new(false);
    app.repository = Some("/synthetic/original-scope".into());
    app.retry = Some(Retry::Discover); // Must be removed even if a caller retained an old read.
    begin(&mut app, &Task::Refresh, true);
    app.complete(Err("lock or guard refusal".into()));
    let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
    terminal
        .draw(|frame| super::views::draw(frame, &app))
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|c| c.symbol() == "P" && c.fg == Color::Red)
    );
    assert!(render(&app, 48, 14).contains("Possible partial effects"));
    assert!(app.retry.is_none() && app.preview.is_none() && app.confirmation.is_none());
    for _ in 0..20 {
        key(&mut app, KeyCode::PageDown);
        assert!(render(&app, 48, 14).contains("Possible partial effects"));
    }
    assert!(matches!(click(&mut app, "[Refresh"), Some(Task::Refresh)));
    begin(&mut app, &Task::Refresh, false);
    app.complete(Ok(Output::Data(Data::default())));
    assert!(render(&app, 48, 14).contains("Earlier write failed"));
    begin(&mut app, &Task::Refresh, true);
    app.complete(Err("second failure".into()));
    assert!(matches!(click(&mut app, "[Recovery"), Some(Task::History)));
    assert!(app.tab == Tab::Recovery);
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
#[test]
fn git_activity_copies_only_public_independent_scope_and_progress_uses_actual_event() {
    let mut app = App::new(false);
    app.repository = Some("/synthetic/files-scope".into());
    app.git_view.comparison = super::git::Comparison::Unstaged;
    let task = app.git_view.request(
        "/synthetic/git-scope".into(),
        "/synthetic/trusted-git".into(),
    );
    begin(&mut app, &task, false);
    app.git_view.root = Some("/synthetic/later-view-root".into());
    let event = dort_core::adoption::Event {
        transaction_id: "0123456789abcdef".repeat(4),
        boundary: dort_core::adoption::Boundary::Mutation,
        sequence: 23,
    };
    let progress = Progress::from_event(&event);
    assert_eq!(progress.transaction_id, event.transaction_id);
    assert_eq!(progress.sequence, 23);
    assert_eq!(progress.boundary, "Mutation boundary");
    let cells = task_cells(&mut app);
    assert!(cells.contains("/synthetic/git-scope"));
    assert!(cells.contains("Unstaged (worktree vs index)"));
    assert!(!cells.contains("files-scope") && !cells.contains("later-view-root"));
}
#[test]
fn receipt_preserves_raw_transaction_scope_and_noop_through_refresh_without_timer() {
    for transaction in [Some("a".repeat(64)), None] {
        let mut app = App::new(false);
        begin(&mut app, &Task::Refresh, true);
        app.shell.return_to_adopt = true; // Success must not vanish behind a returned form.
        assert!(matches!(
            app.complete(Ok(Output::Completed(Completion {
                operation: "File operation",
                transaction_id: transaction.clone(),
                repository: None,
            }))),
            Some(Task::Refresh)
        ));
        begin(&mut app, &Task::Refresh, false);
        assert!(render(&app, 48, 14).contains("Operation result"));
        app.complete(Ok(Output::Data(Data::default())));
        assert_eq!(app.receipt.as_ref().unwrap().transaction_id, transaction);
        for (width, height) in [(48, 14), (80, 24), (120, 30)] {
            let cells = render(&app, width, height);
            assert!(cells.contains("Operation result") && cells.contains("[Back"));
        }
        if let Some(id) = &transaction {
            assert!(task_cells(&mut app).contains(id));
        }
        let mut pages = String::new();
        for _ in 0..20 {
            pages.push_str(&render(&app, 48, 14));
            key(&mut app, KeyCode::PageDown);
        }
        assert!(pages.contains("Transaction ID") || pages.contains("Everything is up to date"));
        key(&mut app, KeyCode::Home);
        assert!(key(&mut app, KeyCode::Tab).is_none() && app.show_result);
        assert!(click(&mut app, "[Back").is_none());
        assert!(!app.show_result && app.receipt.is_some());
        assert!(render(&app, 48, 14).contains("TARGET"));
        app.show_result = true;
        assert!(matches!(
            key(&mut app, KeyCode::Char('h')),
            Some(Task::History)
        ));
        assert!(app.tab == Tab::Recovery);
    }
}
