//! Acceptance checks: hints describe only the actual foreground interaction.
use super::{
    app::{App, Tab},
    batch_ui::Editor,
    hints,
    tests::render,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(app: &mut App, code: KeyCode) {
    assert!(
        app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
            .is_none()
    );
}

#[test]
fn footer_fits_context_and_more_only_when_actions_overflow() {
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.tools_view.declare();
    app.tools_view.declaration.fields[0] = "retained".into();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, w, h);
        let (shown, hidden) = super::workbench::strip_actions(&app, w);
        assert!(shown.len() <= 8);
        assert_eq!(
            screen.lines().last().unwrap().contains("F2 More"),
            !hidden.is_empty()
        );
        assert_eq!(shown.iter().any(|a| a.key == "F2"), !hidden.is_empty());
        assert!(shown.iter().any(|a| a.key == "Enter"));
        assert!(shown.iter().any(|a| a.key == "Esc"));
        if hidden.is_empty() {
            assert!(
                app.key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE), w, h)
                    .is_none()
            );
            assert!(app.more.is_none());
        } else {
            assert!(
                app.key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE), w, h)
                    .is_none()
            );
            let menu = app.more.as_ref().expect("overflow opens More");
            for action in &hidden {
                assert!(
                    menu.items
                        .iter()
                        .any(|item| item.action.label == action.label)
                );
            }
            assert!(
                app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), w, h)
                    .is_none()
            );
        }
        assert_eq!(app.tools_view.declaration.fields[0], "retained");
    }
}

#[test]
fn declaration_footer_describes_fields_and_review_not_underlying_tools_shortcuts() {
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.tools_view.declare();
    key(&mut app, KeyCode::Tab);
    for c in "cspqr".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.tools_view.declaration.fields[0], "cspqr");
    let keys = hints::keys(&app);
    assert!(keys.contains(&("Enter", "Review declaration")));
    assert!(
        !keys.iter().any(|(key, _)| *key == "F3"),
        "Tools has no alternate metadata mode"
    );
    assert!(!keys.iter().any(|(k, _)| *k == "Tab"));
    assert!(!keys.iter().any(|(k, _)| ["c", "s", "p", "r"].contains(k)));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, w, h);
        let footer = screen.lines().last().unwrap();
        assert!(
            footer.contains("Review") && footer.contains("Help"),
            "{screen}"
        );
        assert!(!footer.contains("Inspect tools") && !footer.contains("Snapshots"));
    }
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.tools_view.declaration.focus, 1);
    assert_eq!(app.tools_view.declaration.fields[0], "cspqr");
}

#[test]
fn help_footer_precedes_retained_forms_menus_errors_and_running_work() {
    for state in 0..4 {
        let mut app = App::new(false);
        app.tab = Tab::Tools;
        app.tools_view.declare();
        app.tools_view.declaration.fields[0] = "retained draft".into();
        match state {
            0 => (),
            1 => {
                super::tests::click_places(&mut app, 80, 24);
            }
            2 => app.error = Some("synthetic unavailable metadata".into()),
            _ => app.busy = true,
        }
        key(&mut app, KeyCode::F(1));
        assert!(app.help.is_some());
        assert_eq!(hints::keys(&app), [("Esc", "Back")]);
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let screen = render(&app, w, h);
            let footer = screen.lines().last().unwrap();
            assert!(
                footer.contains("Back") && !footer.contains("More"),
                "{screen}"
            );
            assert!(!footer.contains("Review") && !footer.contains("Retry"));
        }
        key(&mut app, KeyCode::PageDown);
        assert!(app.help.as_ref().unwrap().scroll > 0);
        key(&mut app, KeyCode::Home);
        assert_eq!(app.help.as_ref().unwrap().scroll, 0);
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.tools_view.declaration.fields[0], "retained draft");
        assert!(app.help.is_some());
    }
}

#[test]
fn batch_metadata_does_not_advertise_an_inert_enter_or_hidden_review() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let paths = [home.join(".config/one")].into_iter().collect();
    let mut app = App::new(false);
    app.batch_editor = Some(Editor::new(&paths, &home));
    assert!(hints::keys(&app).contains(&("Enter", "Review all")));
    app.batch_editor.as_mut().unwrap().details = true;
    let keys = hints::keys(&app);
    assert!(!keys.iter().any(|(k, _)| *k == "Enter"));
    assert!(keys.contains(&("F3", "Back to draft")));
    key(&mut app, KeyCode::PageDown);
    assert!(app.batch_editor.as_ref().unwrap().scroll > 0);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.batch_editor.as_ref().unwrap().scroll, 0);
    let before = app.batch_editor.as_ref().unwrap().drafts[0].id.clone();
    key(&mut app, KeyCode::Enter);
    assert!(app.batch_editor.as_ref().unwrap().details);
    assert_eq!(app.batch_editor.as_ref().unwrap().drafts[0].id, before);
    assert!(home.read_dir().unwrap().next().is_none());
}
