use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    recovery,
    shell::{Action, Overlay},
    tests::render,
};
use dort_core::journal::{Diagnosis, Phase};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn row(schema: Option<u32>, phase: Option<Phase>, problem: Option<&str>, id: char) -> Diagnosis {
    Diagnosis {
        repository_id: id.to_string().repeat(64),
        transaction_id: id.to_string().repeat(64),
        journal_schema: schema,
        phase,
        problem: problem.map(str::to_owned),
        valid_checkpoints: 3,
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
#[test]
fn recovery_action_hints_reject_uncertain_terminal_and_preparing_resume() {
    for (schema, phase, problem, rollback, resume, abandon) in [
        (Some(1), Some(Phase::Completed), None, true, false, false),
        (Some(2), Some(Phase::RollingBack), None, true, false, false),
        (Some(2), Some(Phase::RolledBack), None, false, false, false),
        (Some(3), Some(Phase::Preparing), None, false, false, true),
        (Some(3), Some(Phase::Running), None, false, true, true),
        (Some(3), Some(Phase::Completed), None, false, false, false),
        (Some(3), Some(Phase::RolledBack), None, false, false, false),
        (
            Some(3),
            Some(Phase::Running),
            Some("corrupt tail"),
            false,
            false,
            false,
        ),
        (None, None, None, false, false, false),
        (Some(9), Some(Phase::Running), None, false, false, false),
    ] {
        let diagnosis = row(schema, phase, problem, 'a');
        assert_eq!(recovery::rollback(&diagnosis), rollback);
        assert_eq!(recovery::resume(&diagnosis), resume);
        assert_eq!(recovery::abandon(&diagnosis), abandon);
        let mut app = App::new(false);
        app.tab = Tab::Recovery;
        app.complete(Ok(Output::Data(Data {
            history: vec![diagnosis],
            ..Default::default()
        })));
        for (code, eligible) in [('b', rollback), ('s', resume), ('x', abandon)] {
            let visible = app.page_actions().iter().any(|action| {
                matches!(
                    (code, action),
                    ('b', Action::Rollback) | ('s', Action::Resume) | ('x', Action::Abandon)
                )
            });
            assert_eq!(visible, eligible, "page buttons reflect typed eligibility");
            assert_eq!(key(&mut app, KeyCode::Char(code)).is_some(), eligible);
        }
    }
}
#[test]
fn recovery_search_preserves_raw_identity_and_never_hides_global_blockers() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::Data(Data {
        history: vec![
            row(Some(2), Some(Phase::Completed), None, 'a'),
            row(Some(3), Some(Phase::Preparing), None, 'b'),
            row(None, None, Some("unsafe\u{1b}[31m tail"), 'c'),
        ],
        ..Default::default()
    })));
    key(&mut app, KeyCode::Char('/'));
    for c in "bbbb".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.selected, 1);
    assert_eq!(app.visible_history_rows(), vec![1]);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('1'));
    key(&mut app, KeyCode::Char('3'));
    assert_eq!(
        app.selected, 1,
        "returning to filtered history selects a displayed row"
    );
    assert!(render(&app, 100, 30).contains("Blockers: 2"));
    key(&mut app, KeyCode::Enter);
    let text = render(&app, 140, 45);
    assert!(text.contains(&"b".repeat(64)));
    assert!(text.contains("incomplete preparation"));
    assert!(key(&mut app, KeyCode::Char('s')).is_none());
    assert!(
        matches!(key(&mut app, KeyCode::Char('x')), Some(Task::Bootstrap(r,t,_)) if r == "b".repeat(64) && t == "b".repeat(64))
    );
    app.complete(Ok(Output::History {
        rows: vec![
            row(Some(3), Some(Phase::Preparing), None, 'b'),
            row(Some(2), Some(Phase::Completed), None, 'a'),
        ],
        error: None,
    }));
    assert_eq!(app.selected, 0);
    assert_eq!(
        app.history_selection().unwrap().repository_id,
        "b".repeat(64)
    );
    app.history_search.query = "no match".into();
    assert!(key(&mut app, KeyCode::Char('x')).is_none());
    assert!(render(&app, 80, 24).contains("No displayed transaction"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Delete);
    key(&mut app, KeyCode::Char('/'));
    for c in "q?/".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert!(!app.quit);
    assert_eq!(app.history_search.query, "q?/");
}
#[test]
fn compact_long_history_keeps_selected_row_and_global_blockers_visible() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    let mut history: Vec<_> = (0..128)
        .map(|i| {
            let mut diagnosis = row(Some(2), Some(Phase::Completed), None, 'a');
            diagnosis.transaction_id = format!("{i:08x}{}", "a".repeat(56));
            diagnosis
        })
        .collect();
    history.push(row(Some(3), Some(Phase::Preparing), None, 'b'));
    app.complete(Ok(Output::Data(Data {
        history,
        ..Default::default()
    })));
    for _ in 0..120 {
        key(&mut app, KeyCode::Down);
    }
    for (width, height) in [(48, 14), (80, 24), (140, 40), (48, 14)] {
        app.resize(width, height);
        let screen = render(&app, width, height);
        assert!(screen.contains("> 00000078"));
        assert_eq!(app.selected, 120);
    }
    let screen = render(&app, 48, 14);
    assert!(screen.contains("All repositories: 129 | Blockers: 1"));
    assert!(screen.contains("Completed") && screen.contains("Terminal"));
    assert!(screen.contains("SCM") && screen.contains("PHASE") && screen.contains("STATE"));
    app.history_search.query = "00000078".into();
    assert!(render(&app, 48, 14).contains("Blockers: 1"));
    assert!(
        matches!(key(&mut app, KeyCode::Char('b')), Some(Task::Rollback(_, t)) if t.starts_with("00000078"))
    );
}

#[test]
fn unavailable_history_is_not_empty_or_an_actionable_stale_selection() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: vec![],
        error: Some("unreadable state".into()),
    }));
    let text = render(&app, 100, 24);
    assert!(text.contains("Recovery diagnosis unavailable"));
    assert!(!text.contains("No retained transactions"));
    assert!(key(&mut app, KeyCode::Char('b')).is_none());
    assert!(matches!(
        key(&mut app, KeyCode::Char('r')),
        Some(Task::History)
    ));
    app.complete(Ok(Output::History {
        rows: vec![row(None, None, Some("unsafe\u{1b}[31m tail"), 'a')],
        error: None,
    }));
    key(&mut app, KeyCode::Enter);
    let text = render(&app, 100, 40);
    assert!(text.contains("\\u{1b}[31m"));
    assert!(!text.contains('\u{1b}'));
    let mut seen = String::new();
    for _ in 0..20 {
        seen.push_str(&render(&app, 48, 14));
        app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 48, 14);
    }
    assert!(seen.contains("Repository ID:"));
    assert!(seen.contains("Transaction ID:"));
    assert!(seen.contains("Validated checkpoints:"));
}
#[test]
fn preparing_table_enter_inspects_then_the_visible_action_uses_raw_identity() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: vec![row(Some(3), Some(Phase::Preparing), None, 'b')],
        error: None,
    }));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.history_details && app.preview.is_none() && app.confirmation.is_none());
    assert_eq!(app.page_actions().len(), 2);
    key(&mut app, KeyCode::Tab); // no region focus cycle
    assert!(render(&app, 80, 24).contains("[Review abandon · x]"));
    assert!(
        matches!(key(&mut app, KeyCode::Char('x')), Some(Task::Bootstrap(r,t,dort_core::bootstrap::RecoveryAction::Abandon)) if r == "b".repeat(64) && t == "b".repeat(64))
    );
    assert!(app.preview.is_none() && app.confirmation.is_none());
    // Discovery in the global palette still includes inapplicable operations.
    key(&mut app, KeyCode::Char(' ')); // search-first, no extra / keystroke
    for c in "resume".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    let screen = render(&app, 100, 30);
    assert!(screen.contains("Resume unavailable"));
    assert!(key(&mut app, KeyCode::Enter).is_none());
}
#[test]
fn error_retry_never_retains_approval_and_failed_writes_offer_read_only_recovery() {
    let mut app = App::new(false);
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.complete(Err("stale preview read".into()));
    assert!(matches!(
        key(&mut app, KeyCode::Char('r')),
        Some(Task::Files(_))
    ));
    assert!(app.preview.is_none() && app.confirmation.is_none());
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.start(true);
    app.complete(Err("held writer lock".into()));
    assert!(app.retry.is_none());
    assert!(matches!(
        key(&mut app, KeyCode::Char('r')),
        Some(Task::Refresh)
    ));
    app.start(true);
    app.complete(Err("failed after publication".into()));
    assert!(matches!(
        key(&mut app, KeyCode::Char('h')),
        Some(Task::History)
    ));
    assert!(app.tab == Tab::Recovery && app.preview.is_none() && app.confirmation.is_none());
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.complete(Err("another preview read failure".into()));
    key(&mut app, KeyCode::Esc);
    assert!(app.retry.is_none());
    app.tab = Tab::Git;
    app.repository = Some(std::path::PathBuf::from("/missing-git-root"));
    key(&mut app, KeyCode::Char('r'));
    assert!(app.error.is_some());
    assert!(
        key(&mut app, KeyCode::Char('r')).is_none(),
        "missing Git must not retry an unrelated file intent"
    );
    key(&mut app, KeyCode::Esc);
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.complete(Ok(Output::Data(Data::default())));
    assert!(
        app.retry.is_none(),
        "successful jobs do not retain retry history"
    );
}
#[test]
fn about_reports_build_version_and_returns_to_prior_focus_and_details() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.details_scroll = 8;
    app.shell.overlay = Some(Overlay::About);
    let text = render(&app, 120, 45);
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    assert!(text.contains("Not implemented:"));
    key(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Recovery);
    assert_eq!(app.details_scroll, 8);
}
