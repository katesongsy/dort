//! Visible controls, inline details and context-specific keyboard contracts.
use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    navigation,
    shell::{Action, Form, Overlay},
    tests::render,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
fn key(app: &mut App, code: KeyCode, w: u16, h: u16) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), w, h)
}
#[test]
fn menu_labels_have_unique_working_local_shortcuts_and_keep_guards() {
    for group in 0..navigation::GROUPS.len() {
        let mut codes = Vec::new();
        for entry in navigation::entries(group) {
            let code = navigation::shortcut(entry.item);
            assert!(!codes.contains(&code));
            codes.push(code);
            let label = match code {
                KeyCode::Char(c) => c.to_string(),
                KeyCode::F(n) => format!("F{n}"),
                _ => panic!("explicit shortcut"),
            };
            assert!(entry.label.contains(&format!("[{label}]")));
            let mut app = App::new(false);
            let blocked = matches!(entry.item, navigation::Item::Action(action) if app.action_reason(action).is_some());
            app.open_menu(group, 80);
            let _ = key(&mut app, code, 80, 24);
            assert!(app.preview.is_none() && app.confirmation.is_none());
            if code == KeyCode::F(1) {
                assert!(app.help.is_some() && app.navigation.menu.is_some());
            } else if code == KeyCode::Char('q') {
                assert!(app.quit);
            } else {
                assert_eq!(app.navigation.menu.is_some(), blocked, "{label}");
            }
        }
    }
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.open_menu(4, w);
        app.navigation.menu.as_mut().unwrap().expanded = true;
        assert!(matches!(
            key(&mut app, KeyCode::Char('j'), w, h),
            Some(Task::Diagnostics)
        ));
        assert!(app.tab == Tab::Checks && app.showing_diagnostics);
        app.open_menu(2, w);
        app.navigation.menu.as_mut().unwrap().expanded = true;
        assert!(matches!(
            key(&mut app, KeyCode::Char('U'), w, h),
            Some(Task::IgnoredPaths)
        ));
        assert!(app.preview.is_none() && app.confirmation.is_none());
        app = App::new(true);
        app.open_menu(0, w);
        app.navigation.menu.as_mut().unwrap().expanded = true;
        assert!(key(&mut app, KeyCode::Char('v'), w, h).is_none());
        assert!(app.dry_run && app.navigation.menu.is_some());
    }
}
#[test]
fn form_pages_keep_controls_draft_focus_and_inline_details_at_all_sizes() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(Form::Adopt));
        app.shell.adoption.target = format!("~/{}", "long-segment/".repeat(35));
        let before = app.shell.adoption.target.clone();
        let screen = render(&app, w, h);
        assert!(screen.contains("TARGET") && screen.contains("Draft details"));
        for code in [KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageUp] {
            let old = app.shell.scroll;
            let rows = app.shell.page_rows.get();
            assert!(key(&mut app, code, w, h).is_none());
            assert_eq!(
                app.shell.scroll,
                if code == KeyCode::PageDown {
                    old.saturating_add(rows)
                } else {
                    old.saturating_sub(rows)
                }
            );
            let screen = render(&app, w, h);
            assert!(screen.contains("TARGET") && screen.contains("Draft details"));
            assert_eq!(app.shell.adoption.target, before);
            assert_eq!(app.shell.adoption.focused, 0);
            assert!(app.preview.is_none() && app.confirmation.is_none());
        }
        key(&mut app, KeyCode::Home, w, h);
        assert_eq!(app.shell.scroll, 0);
        assert_eq!(app.shell.adoption.target, before);
    }
}
#[test]
fn compact_list_pages_never_open_details_and_x_does_not_mark_files() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.complete(Ok(Output::Data(Data {
        files: (0..30)
            .map(|n| (format!("entry-{n:02}"), "metadata".into()))
            .collect(),
        ..Default::default()
    })));
    render(&app, 48, 14);
    for code in [KeyCode::PageDown, KeyCode::PageUp, KeyCode::Home] {
        assert!(key(&mut app, code, 48, 14).is_none());
        assert!(!app.inventory.details);
        assert!(render(&app, 48, 14).contains("FILE"));
    }
    key(&mut app, KeyCode::Char('x'), 48, 14);
    assert!(app.inventory.managed_marks.is_empty());
    key(&mut app, KeyCode::Char(' '), 48, 14);
    assert_eq!(app.inventory.managed_marks.len(), 1);
    key(&mut app, KeyCode::Char('x'), 48, 14);
    assert_eq!(app.inventory.managed_marks.len(), 1);
    key(&mut app, KeyCode::Char(' '), 48, 14);
    assert!(app.inventory.managed_marks.is_empty());
    assert!(!app.page_actions().contains(&Action::Inspect));
    key(&mut app, KeyCode::Enter, 48, 14);
    assert!(app.inventory.details);
}
#[test]
fn marked_editor_pages_keep_table_review_controls_and_drafts_inline() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        let paths = [std::path::PathBuf::from(format!(
            "/synthetic/home/{}/config",
            "segment/".repeat(30)
        ))]
        .into_iter()
        .collect();
        app.batch_editor = Some(super::batch_ui::Editor::new(
            &paths,
            std::path::Path::new("/synthetic/home"),
        ));
        let original = app.batch_editor.as_ref().unwrap().drafts[0].target.clone();
        for code in [
            KeyCode::PageDown,
            KeyCode::PageDown,
            KeyCode::PageUp,
            KeyCode::Home,
        ] {
            let screen = render(&app, w, h);
            assert!(
                screen.contains("TARGET")
                    && screen.contains("Review all")
                    && screen.contains("Edit item")
            );
            assert!(key(&mut app, code, w, h).is_none());
            let editor = app.batch_editor.as_ref().unwrap();
            assert!(!editor.item_form && editor.focused == editor.drafts.len() * 3);
            assert_eq!(editor.drafts[0].target, original);
            assert!(app.preview.is_none());
        }
        assert_eq!(app.batch_editor.as_ref().unwrap().scroll, 0);
        key(&mut app, KeyCode::Char('e'), w, h);
        for _ in 0..3 {
            key(&mut app, KeyCode::Tab, w, h);
        }
        let screen = render(&app, w, h);
        assert!(screen.lines().last().unwrap().contains("Done"));
        assert!(key(&mut app, KeyCode::Enter, w, h).is_none());
        assert!(!app.batch_editor.as_ref().unwrap().item_form);
    }
}
#[test]
fn active_task_pages_scroll_activity_instead_of_the_retained_form() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    app.shell.adoption.target = "~/.retained".into();
    app.start(false);
    app.shell.scroll = 9;
    key(&mut app, KeyCode::PageDown, 48, 14);
    assert_eq!(app.details_scroll, 2);
    assert_eq!(app.shell.scroll, 9);
    key(&mut app, KeyCode::Home, 48, 14);
    assert_eq!(app.details_scroll, 0);
    assert_eq!(app.shell.scroll, 9);
    assert_eq!(app.shell.adoption.target, "~/.retained");
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
#[test]
fn paging_copy_describes_scrolling_instead_of_opening_details() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        for tab in [
            Tab::Overview,
            Tab::Files,
            Tab::Recovery,
            Tab::Checks,
            Tab::Git,
        ] {
            app.tab = tab;
            for _ in 0..80 {
                for line in render(&app, w, h).lines() {
                    assert!(
                        !line.contains("PgDn") || line.contains("Scroll"),
                        "ambiguous paging hint in workspace"
                    );
                }
                key(&mut app, KeyCode::PageDown, w, h);
            }
        }
        for form in [
            Form::Init,
            Form::Adopt,
            Form::Check,
            Form::KeepId,
            Form::RemoveId,
        ] {
            app.shell.overlay = Some(Overlay::Form(form));
            key(&mut app, KeyCode::Home, w, h);
            for _ in 0..80 {
                for line in render(&app, w, h).lines() {
                    assert!(
                        !line.contains("PgDn") || line.contains("Scroll"),
                        "ambiguous paging hint in form"
                    );
                }
                key(&mut app, KeyCode::PageDown, w, h);
            }
        }
    }
}
#[test]
fn workspace_toolbar_has_one_row_and_overlay_uses_reclaimed_space() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.complete(Ok(Output::Data(Data {
        repository: Some("/synthetic/repository".into()),
        ..Default::default()
    })));
    let screen = render(&app, 80, 24);
    let rows: Vec<_> = screen.lines().collect();
    let apply = rows.iter().position(|row| row.contains("[Apply")).unwrap();
    assert_eq!(apply, rows.len() - 3); // one action row, status, global hints
    assert!(!screen.contains("[Inspect"));
    key(&mut app, KeyCode::Char(';'), 48, 14);
    let screen = render(&app, 48, 14);
    assert!(screen.lines().nth(2).unwrap().contains("Search by label"));
    assert!(screen.contains("Action details"));
}
