//! Context ordering, search focus, typed dispatch and compact prerequisite inspection.
use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    components as ui,
    context::{Effect, Scope},
    palette,
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
fn contextual_catalog_is_unique_and_separates_scope_from_effect() {
    let mut union = Vec::new();
    for tab in [
        Tab::Overview,
        Tab::Files,
        Tab::Recovery,
        Tab::Checks,
        Tab::Git,
    ] {
        for discovering in [false, true] {
            let mut app = App::new(false);
            app.tab = tab;
            app.discovering = discovering;
            let rows = palette::entries(&app);
            assert!(rows.windows(2).all(|p| p[0].scope <= p[1].scope));
            for row in &rows {
                assert_eq!(rows.iter().filter(|e| e.action == row.action).count(), 1);
                if !union.contains(&row.action) {
                    union.push(row.action);
                }
                if row.action == Action::GitPatch {
                    assert!(
                        tab == Tab::Git
                            && row.scope == Scope::View
                            && row.effect == Effect::Disclosure
                    );
                }
                if row.action == Action::IgnoreSelected {
                    assert!(
                        tab == Tab::Files
                            && discovering
                            && row.scope == Scope::Item
                            && row.effect == Effect::Preview
                    );
                }
                if row.action == Action::KeepMarked {
                    assert!(
                        tab == Tab::Files
                            && !discovering
                            && row.scope == Scope::Marked
                            && row.effect == Effect::Preview
                    );
                }
            }
            for global in [
                Action::Choose,
                Action::Init,
                Action::Help,
                Action::Quit,
                Action::Destination(Tab::Files),
            ] {
                assert!(!rows.iter().any(|r| r.action == global));
            }
            assert_eq!(
                rows.iter().any(|r| r.action == Action::StandaloneProfiles),
                tab == Tab::Checks
            );
            assert_eq!(
                rows.iter().any(|r| r.action == Action::Rollback),
                tab == Tab::Recovery
            );
            assert_eq!(
                rows.iter().any(|r| r.action == Action::Remove),
                tab == Tab::Files && !discovering
            );
            assert_eq!(
                palette::matches(&rows, "STANDALONE MANIFEST").len(),
                if tab == Tab::Checks { 2 } else { 0 }
            );
        }
    }
    for group in 0..super::navigation::GROUPS.len() {
        for entry in super::navigation::entries(group) {
            if let super::navigation::Item::Action(action) = entry.item
                && !union.contains(&action)
            {
                union.push(action);
            }
        }
    }
    // Existing numeric shortcuts additionally expose destination-only checks/Git routes.
    for action in [
        Action::Destination(Tab::Checks),
        Action::Destination(Tab::Git),
    ] {
        if !union.contains(&action) {
            union.push(action);
        }
    }
    assert_eq!(union.len(), 47); // Every supported typed command has a home.
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
        for label in ["Search by label", "SCOPE", "ACTION", "Item"] {
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
    app.tab = Tab::Git;
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
