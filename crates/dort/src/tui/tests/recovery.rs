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
    super::tests::destination(&mut app, super::app::Tab::Overview, 80, 24);
    super::tests::destination(&mut app, super::app::Tab::Recovery, 80, 24);
    assert_eq!(
        app.selected, 1,
        "returning to filtered history selects a displayed row"
    );
    assert!(render(&app, 100, 30).contains("Blocked writes: 2"));
    key(&mut app, KeyCode::Enter);
    let text = render(&app, 140, 45);
    assert!(text.contains(&"b".repeat(64)));
    assert!(text.contains("setup is incomplete"));
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
        generated_recovery: None,
    }));
    assert_eq!(app.selected, 0);
    assert_eq!(
        app.history_selection().unwrap().repository_id,
        "b".repeat(64)
    );
    app.history_search.query = "no match".into();
    assert!(key(&mut app, KeyCode::Char('x')).is_none());
    assert!(render(&app, 80, 24).contains("No operation record"));
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
        assert!(screen.contains("› 00000078"));
        assert_eq!(app.selected, 120);
    }
    let screen = render(&app, 48, 14);
    assert!(screen.contains("/129 shown") && screen.contains("Blocked writes: 1"));
    assert!(screen.contains("Completed") && screen.contains("Finished"));
    assert!(screen.contains("TYPE") && screen.contains("STATE") && screen.contains("STATE"));
    app.history_search.query = "00000078".into();
    assert!(render(&app, 48, 14).contains("Blocked writes: 1"));
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
        generated_recovery: None,
    }));
    let text = render(&app, 100, 24);
    assert!(text.contains("Recovery unavailable"));
    assert!(!text.contains("No retained transactions"));
    assert!(key(&mut app, KeyCode::Char('b')).is_none());
    assert!(matches!(
        key(&mut app, KeyCode::Char('R')),
        Some(Task::History)
    ));
    app.complete(Ok(Output::History {
        rows: vec![row(None, None, Some("unsafe\u{1b}[31m tail"), 'a')],
        error: None,
        generated_recovery: None,
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
    assert!(seen.contains("Operation ID:"));
    assert!(seen.contains("Saved steps:"));
}
#[test]
fn preparing_table_enter_inspects_then_the_visible_action_uses_raw_identity() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: vec![row(Some(3), Some(Phase::Preparing), None, 'b')],
        error: None,
        generated_recovery: None,
    }));
    for code in [KeyCode::PageDown, KeyCode::PageUp, KeyCode::Home] {
        assert!(key(&mut app, code).is_none());
        assert!(
            app.history_region == crate::tui::workspace::Region::Queue && app.preview.is_none()
        );
    }
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.history_region == crate::tui::workspace::Region::Details && app.preview.is_none());
    assert!(app.page_actions() == vec![Action::Abandon]);
    key(&mut app, KeyCode::Tab); // no region focus cycle
    assert!(super::tests::has_action(&app, "Abandon"));
    assert!(
        matches!(key(&mut app, KeyCode::Char('x')), Some(Task::Bootstrap(r,t,dort_core::bootstrap::RecoveryAction::Abandon)) if r == "b".repeat(64) && t == "b".repeat(64))
    );
    assert!(app.preview.is_none());
    // Unavailable recovery operations still explain their reason.
    assert!(app.activate(Action::Resume).is_none());
    assert!(app.status.contains("Resume is unavailable"));
}
#[test]
fn error_retry_never_retains_approval_and_failed_writes_offer_read_only_recovery() {
    let mut app = App::new(false);
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.complete(Err("stale preview read".into()));
    assert!(matches!(
        key(&mut app, KeyCode::Char('R')),
        Some(Task::Files(_))
    ));
    assert!(app.preview.is_none());
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.start(true);
    app.complete(Err("held writer lock".into()));
    assert!(app.retry.is_none());
    assert!(matches!(
        key(&mut app, KeyCode::Char('R')),
        Some(Task::Refresh)
    ));
    app.start(true);
    app.complete(Err("failed after publication".into()));
    assert!(matches!(
        key(&mut app, KeyCode::Char('h')),
        Some(Task::History)
    ));
    assert!(app.tab == Tab::Recovery && app.preview.is_none());
    app.retry = Task::Files(dort_core::planner::Request::Apply).retry_intent();
    app.complete(Err("another preview read failure".into()));
    key(&mut app, KeyCode::Esc);
    assert!(app.retry.is_none());
    app.tab = Tab::Git;
    app.repository = Some(std::path::PathBuf::from("/missing-git-root"));
    key(&mut app, KeyCode::Char('R'));
    assert!(app.error.is_some());
    assert!(
        key(&mut app, KeyCode::Char('R')).is_none(),
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
    let mut text = render(&app, 120, 45);
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    for _ in 0..40 {
        assert!(key(&mut app, KeyCode::PageDown).is_none());
        text.push_str(&render(&app, 120, 45));
    }
    assert!(text.contains("Not supported:"));
    assert!(text.contains("Homebrew/npm"));
    assert!(text.contains("macOS, Linux and WSL"));
    key(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Recovery);
    assert_eq!(app.details_scroll, 8);
}

#[test]
fn recovery_split_focus_survives_resize_and_compact_return() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: ('a'..='z')
            .map(|id| row(Some(3), Some(Phase::Preparing), None, id))
            .collect(),
        error: None,
        generated_recovery: None,
    }));
    let press = |app: &mut App, code, size: (u16, u16)| {
        assert!(
            app.key(KeyEvent::new(code, KeyModifiers::NONE), size.0, size.1)
                .is_none()
        );
    };
    for width in [89, 90, 99, 100, 106, 107, 110] {
        let screen = render(&app, width, 18);
        assert_eq!(
            screen.contains("Operation details"),
            width >= 90,
            "{screen}"
        );
        assert_eq!(screen.matches("[Active]").count(), 1, "{screen}");
        assert!(screen.contains("Operations [Active]"), "{screen}");
    }
    let wide = (100, 18);
    let narrow = (78, 18);
    render(&app, wide.0, wide.1);
    press(&mut app, KeyCode::Char('/'), wide);
    for c in "preparing".chars() {
        press(&mut app, KeyCode::Char(c), wide);
    }
    press(&mut app, KeyCode::Enter, wide);
    press(&mut app, KeyCode::PageDown, wide);
    let before = render(&app, wide.0, wide.1);
    let selected = before
        .lines()
        .find_map(|line| line.split_once("› "))
        .unwrap()
        .1[..8]
        .to_string();
    press(&mut app, KeyCode::Tab, wide);
    press(&mut app, KeyCode::Down, wide);
    press(&mut app, KeyCode::Char('y'), wide);
    let details = render(&app, wide.0, wide.1);
    assert_eq!(details.matches("[Active]").count(), 1, "{details}");
    assert!(details.contains("Operation details [Active]"));
    assert!(details.contains(&format!("› {selected}")));
    let compact = render(&app, narrow.0, narrow.1);
    assert!(
        compact.contains("Operation details [Active]") && !compact.contains("Operations [Active]")
    );
    press(&mut app, KeyCode::Esc, narrow);
    let queue = render(&app, narrow.0, narrow.1);
    assert!(queue.contains("Operations [Active]") && queue.contains(&format!("› {selected}")));
    assert!(queue.contains("preparing") && !queue.contains("Operation details"));
    press(&mut app, KeyCode::Enter, narrow);
    assert!(render(&app, narrow.0, narrow.1).contains("Operation details [Active]"));
    let grown = render(&app, 110, 18);
    assert!(
        grown.contains("Operation details [Active]") && grown.contains(&format!("› {selected}"))
    );
}
