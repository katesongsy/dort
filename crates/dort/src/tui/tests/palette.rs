//! Context ordering, search focus, typed dispatch and compact prerequisite inspection.
use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    components as ui,
    palette::{self, Group},
    shell::{Action, Form, Overlay},
    tests::render,
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Modifier},
};
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 48, 14)
}
fn text(app: &mut App, value: &str) {
    for c in value.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}
fn current(app: &App) -> (&str, usize, bool, &[palette::Entry]) {
    let Some(Overlay::Actions {
        query,
        selected,
        searching,
        entries,
    }) = &app.shell.overlay
    else {
        panic!("Actions")
    };
    (query, *selected, *searching, entries)
}
#[test]
fn context_groups_order_one_complete_action_set_and_keep_risk_separate() {
    let tabs = [
        Tab::Overview,
        Tab::Files,
        Tab::Recovery,
        Tab::Checks,
        Tab::Git,
    ];
    let baseline = palette::entries(Tab::Overview, false, &[]);
    for tab in tabs {
        for discovering in [false, true] {
            let mut app = App::new(false);
            app.tab = tab;
            app.discovering = discovering;
            let rows = palette::entries(tab, discovering, &app.page_actions());
            assert!(rows.windows(2).all(|p| p[0].group <= p[1].group));
            assert_eq!(rows.len(), baseline.len());
            for entry in &baseline {
                assert_eq!(rows.iter().filter(|r| r.action == entry.action).count(), 1);
            }
            assert!(rows.iter().any(|e| e.action == Action::StandaloneProfiles));
            for row in &rows {
                if matches!(
                    row.action,
                    Action::Apply
                        | Action::Keep
                        | Action::Remove
                        | Action::Rollback
                        | Action::Resume
                        | Action::Abandon
                        | Action::KeepId
                        | Action::RemoveId
                        | Action::BatchAdopt
                        | Action::ApplyMarked
                        | Action::KeepMarked
                        | Action::RemoveMarked
                ) {
                    assert!(row.group == Group::Preview);
                }
                if row.action == Action::GitPatch {
                    assert!(row.group == Group::Content);
                }
            }
            for action in [
                Action::BatchAdopt,
                Action::ApplyMarked,
                Action::KeepMarked,
                Action::RemoveMarked,
                Action::ToggleMark,
                Action::ClearMarks,
                Action::ExpandAll,
                Action::CollapseAll,
            ] {
                assert_eq!(rows.iter().filter(|r| r.action == action).count(), 1);
            }
            let preferred = app.page_actions();
            assert!(rows[0].action == preferred[0]);
            let results = palette::matches(&rows, "STANDALONE MANIFEST");
            assert_eq!(results.len(), 2); // standalone check and its separate additions
        }
    }
}
#[test]
fn search_first_has_real_caret_semantic_selection_and_literal_shortcut_input() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    assert!(key(&mut app, KeyCode::Char(';')).is_none());
    assert!(current(&app).2 && app.text_entry());
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| views::draw(f, &app)).unwrap();
        let b = terminal.backend().buffer();
        let screen = render(&app, w, h);
        for label in ["Search by label", "GROUP", "ACTION", "Item"] {
            assert!(screen.contains(label));
        }
        let arrow = b.content.iter().position(|c| c.symbol() == ">").unwrap();
        assert_eq!(b.content[arrow].bg, Color::Cyan);
        assert!(
            b.content[arrow]
                .modifier
                .contains(Modifier::BOLD | Modifier::REVERSED)
        );
        assert_eq!(
            terminal.get_cursor_position().unwrap().x,
            (w - w.min(88)) / 2 + 1
        );
    }
    text(&mut app, "q?/");
    assert_eq!(current(&app).0, "q?/");
    assert!(!app.quit && app.tab == Tab::Files);
    assert!(render(&app, 48, 14).contains("No matching actions"));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    key(&mut app, KeyCode::Esc); // leave input, retain literal query
    assert!(!current(&app).2 && current(&app).0 == "q?/");
    key(&mut app, KeyCode::Char('/'));
    assert!(current(&app).2);
    app.key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        48,
        14,
    );
    assert_eq!(current(&app).0, "");
    key(&mut app, KeyCode::Down);
    let selected = current(&app).1;
    assert!(!current(&app).2 && selected == 1);
    key(&mut app, KeyCode::Char('/')); // search focus without changing row
    key(&mut app, KeyCode::PageDown);
    assert_eq!(current(&app).1, selected);
    assert_eq!(app.shell.scroll, 1);
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(app.shell.overlay.is_none() && app.tab == Tab::Files);
}
#[test]
fn disabled_actions_stay_open_and_full_reasons_are_keyboard_reachable_without_dispatch() {
    let mut app = App::new(false);
    key(&mut app, KeyCode::Char(';'));
    text(&mut app, "View patch content");
    let reason = app.action_reason(Action::GitPatch).unwrap();
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(matches!(app.shell.overlay, Some(Overlay::Actions { .. })));
    assert_eq!(app.status, reason);
    assert!(app.preview.is_none() && app.confirmation.is_none() && !app.git_view.sensitive());
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.shell.scroll, 1);
    assert!(render(&app, 48, 14).contains("Actions | No work while browsing"));
    let mut joined = String::new();
    for _ in 0..60 {
        let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
        terminal.draw(|f| views::draw(f, &app)).unwrap();
        let b = terminal.backend().buffer();
        let mut viewport = String::new();
        for y in 4..11 {
            for x in 1..47 {
                viewport.push_str(b[(x, y)].symbol());
            }
        }
        let overlap = (0..=viewport.len().min(joined.len()))
            .rev()
            .find(|n| joined.ends_with(&viewport[..*n]))
            .unwrap();
        joined.push_str(&viewport[overlap..]);
        assert!(key(&mut app, KeyCode::PageDown).is_none());
    }
    assert!(joined.contains(reason));
    assert!(joined.contains("Literal view patch is separately required"));
    assert!(joined.contains(Action::GitPatch.label()));
    key(&mut app, KeyCode::PageUp);
    key(&mut app, KeyCode::Home);
    assert!(render(&app, 48, 14).contains("ACTION"));
    assert_eq!(current(&app).0, "View patch content");
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.git_view.metadata.is_none());
}
#[test]
fn filtered_entry_identity_dispatches_a_draft_not_a_filtered_file_or_approval() {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        repository: Some("/synthetic/root".into()),
        files: vec![("raw-q?".into(), "metadata only".into())],
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app.requested_profiles = vec!["session-addition".into()];
    app.inventory.managed.query = "no-matches".into();
    key(&mut app, KeyCode::Char(';'));
    text(&mut app, "KEEP LOCAL FILE: ENTER EXACT");
    let (query, selected, _, entries) = current(&app);
    let matches = palette::matches(entries, query);
    assert!(matches[selected].action == Action::KeepId);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Form(Form::KeepId))
    ));
    text(&mut app, "raw-q?");
    assert!(key(&mut app, KeyCode::Enter).is_none());
    let Some(Task::Files(dort_core::planner::Request::Unmanage { id, mode })) =
        key(&mut app, KeyCode::Enter)
    else {
        panic!("real exact-ID preview task")
    };
    assert_eq!(id, "raw-q?");
    assert!(matches!(mode, dort_core::planner::UnmanageMode::Keep));
    assert!(app.preview.is_none() && app.confirmation.is_none());
    assert_eq!(app.requested_profiles, ["session-addition"]);
    assert_eq!(app.inventory.managed.query, "no-matches");
    assert_eq!(
        app.data
            .as_ref()
            .unwrap()
            .repository
            .as_ref()
            .unwrap()
            .to_str(),
        Some("/synthetic/root")
    );
    // Browsing/disabling never converts an action label into an ID or a request.
    assert_eq!(ui::inline("raw-q?"), id);
}
