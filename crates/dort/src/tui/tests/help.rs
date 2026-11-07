//! Help is a non-authorizing, exclusive layer over retained typed surfaces.
use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    batch_ui::Editor,
    help::Topic,
    shell::{Form, Overlay},
    tests::render,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn inert(app: &mut App) {
    for code in [
        KeyCode::Enter,
        KeyCode::Char('y'),
        KeyCode::Char('e'),
        KeyCode::Char('s'),
        KeyCode::Char('a'),
        KeyCode::Char('1'),
    ] {
        assert!(key(app, code).is_none());
        assert!(app.help.is_some());
    }
}
#[test]
fn every_view_has_its_own_help_and_preserves_navigation_state() {
    for (tab, title) in [
        (Tab::Overview, "Overview"),
        (Tab::Files, "Managed files"),
        (Tab::Recovery, "Recovery"),
        (Tab::Checks, "Manifest checks"),
        (Tab::Git, "Git inspection"),
    ] {
        let mut app = App::new(false);
        app.tab = tab;
        app.details_scroll = 7;
        key(&mut app, KeyCode::F(1));
        assert_eq!(app.help.as_ref().unwrap().topic.title(), title);
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let s = render(&app, w, h);
            assert!(s.contains(title));
            assert!(!s.contains("[p Scope]")); // no underlying destination body or controls
        }
        inert(&mut app);
        key(&mut app, KeyCode::PageDown);
        assert_eq!(app.help.as_ref().unwrap().scroll, 2);
        key(&mut app, KeyCode::Home);
        assert_eq!(app.help.as_ref().unwrap().scroll, 0);
        key(&mut app, KeyCode::F(1));
        assert!(app.help.is_none() && app.tab == tab && app.details_scroll == 7);
        key(&mut app, KeyCode::Char('?'));
        assert!(app.help.is_some());
        key(&mut app, KeyCode::Esc);
    }
}
#[test]
fn forms_actions_and_search_keep_exact_drafts_scroll_and_focus() {
    for form in [
        Form::Choose,
        Form::Init,
        Form::Adopt,
        Form::KeepId,
        Form::RemoveId,
        Form::Check,
    ] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(form));
        for c in "private-draft-q?/".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        assert!(app.help.is_none() && !app.quit);
        let draft = app.shell.draft(form).to_owned();
        let target = app.shell.adoption.target.clone();
        app.shell.scroll = 3;
        key(&mut app, KeyCode::F(1));
        assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Form(f) if f == form));
        assert!(!render(&app, 140, 40).contains("private-draft"));
        inert(&mut app);
        key(&mut app, KeyCode::Esc);
        assert!(matches!(app.shell.overlay, Some(Overlay::Form(f)) if f == form));
        assert_eq!(app.shell.draft(form), draft);
        assert_eq!(app.shell.adoption.target, target);
        assert_eq!(app.shell.scroll, 3);
    }
    let mut app = App::new(false);
    key(&mut app, KeyCode::Char(';'));
    for c in "q?/".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::F(1));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    assert!(
        matches!(&app.shell.overlay, Some(Overlay::Actions {query, searching:true, selected:0, ..}) if query == "q?/")
    );
    key(&mut app, KeyCode::Esc); // results focus
    key(&mut app, KeyCode::Char('?'));
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Actions));
    key(&mut app, KeyCode::F(1));
    assert!(
        matches!(&app.shell.overlay, Some(Overlay::Actions {query, searching:false, ..}) if query == "q?/")
    );
    app.shell.overlay = None;
    for tab in [Tab::Files, Tab::Recovery, Tab::Git] {
        app.tab = tab;
        key(&mut app, KeyCode::Char('/'));
        for c in "q?/".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::F(1));
        key(&mut app, KeyCode::Esc);
        assert!(app.text_entry() && !app.quit);
        let query = match tab {
            Tab::Files => &app.inventory.managed.query,
            Tab::Recovery => &app.history_search.query,
            _ => &app.git_view.search.query,
        };
        assert_eq!(query, "q?/");
    }
}
#[test]
fn batch_item_help_does_not_edit_submit_or_render_private_drafts() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    let paths = [std::path::PathBuf::from(
        "/synthetic/home/.config/private-draft",
    )]
    .into_iter()
    .collect();
    app.batch_editor = Some(Editor::new(&paths, std::path::Path::new("/synthetic/home")));
    key(&mut app, KeyCode::Char('?')); // Review all is not text
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Batch));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('e'));
    key(&mut app, KeyCode::Char('?')); // target field is text
    let e = app.batch_editor.as_ref().unwrap();
    let (target, focused) = (e.drafts[0].target.clone(), e.focused);
    assert!(target.ends_with('?') && app.help.is_none());
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::BatchItem));
    assert!(!render(&app, 140, 40).contains("private-draft"));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    let e = app.batch_editor.as_ref().unwrap();
    assert!(e.open && e.item_form);
    assert_eq!(e.drafts[0].target, target);
    assert_eq!(e.focused, focused);
}
#[test]
fn help_task_error_completion_and_picker_return_without_scope_changes() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Init));
    app.shell.init_draft = "private-draft".into();
    app.start(true);
    key(&mut app, KeyCode::Char('?')); // background form is not the active text surface
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Task));
    inert(&mut app);
    assert!(app.busy && app.writing);
    app.complete(Ok(Output::Data(Data::default())));
    assert!(app.help.is_some() && !app.busy);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.init_draft, "private-draft");
    app.error = Some("private-cause".into());
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Error));
    assert!(!render(&app, 80, 24).contains("private-cause"));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.error.as_deref(), Some("private-cause"));
    app.error = None;
    app.show_result = true;
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Result));
    key(&mut app, KeyCode::Esc);
    assert!(app.show_result);
    app.show_result = false;
    app.shell.overlay = Some(Overlay::Ignored {
        paths: vec!["private-path".into()],
        selected: 0,
    });
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.help.as_ref().unwrap().topic, Topic::Ignored));
    inert(&mut app);
    let s = render(&app, 80, 24);
    assert!(!s.contains("private-path"));
    // Mouse close is Help-only, even with retained underlying controls.
    assert!(
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 23,
                modifiers: KeyModifiers::NONE
            },
            80,
            24
        )
        .is_none()
    );
    assert!(app.help.is_none());
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Ignored { selected: 0, .. })
    ));
    app.start(true);
    key(&mut app, KeyCode::F(1));
    app.key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        80,
        24,
    );
    assert!(app.exit_requested && !app.quit && app.busy);
}
