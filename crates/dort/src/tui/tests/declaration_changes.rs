use super::{
    actions::{self, Output, Task},
    app::{App, Tab},
    shell::Action,
    tests::render,
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
fn key(app: &mut App, code: KeyCode, width: u16, height: u16) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), width, height)
}
fn perform(f: &Fixture, app: &mut App, task: Task) {
    let writing = matches!(task, Task::Execute(_));
    app.start(writing);
    let output =
        Ok(actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(())).unwrap());
    if let Some(refresh) = app.complete(output) {
        assert!(matches!(refresh, Task::Refresh));
        let output =
            actions::perform_scoped(refresh, &f.host, Some(f.repo.clone()), &[], |_| Ok(()));
        app.complete(output.map_err(|e| actions::diagnostic(&e)));
    }
}
fn app(f: &Fixture, dry: bool) -> App {
    let mut app = App::new(dry);
    app.repository = Some(f.repo.clone());
    perform(f, &mut app, Task::Refresh);
    app.activate(Action::Destination(Tab::Tools));
    app
}
#[test]
fn selected_declaration_edit_remove_use_one_exact_approval_and_refresh_list() {
    for (width, height) in [(48, 14), (80, 24), (140, 40)] {
        let f = Fixture::new();
        // Installed package markers must survive both metadata changes.
        std::fs::write(f.host.home.join("a"), "installed").unwrap();
        let mut app = app(&f, false);
        assert!(key(&mut app, KeyCode::Char('e'), width, height).is_none());
        assert!(app.tools_view.declaration_open);
        let screen = render(&app, width, height);
        assert!(screen.contains("Edit tool"), "{screen}");
        let draft = &mut app.tools_view.declaration;
        assert_eq!(draft.fields[0], "a");
        assert_eq!(draft.fields[6], "");
        assert!(!draft.controls().contains(&0));
        draft.fields[6] = "latest".into();
        // Menus retain the edited draft.
        key(&mut app, KeyCode::F(1), width, height);
        key(&mut app, KeyCode::Esc, width, height);
        assert_eq!(app.tools_view.declaration.fields[6], "latest");
        let task = key(&mut app, KeyCode::Enter, width, height).unwrap();
        assert!(matches!(task, Task::ToolDeclaration(_)));
        perform(&f, &mut app, task);
        let report = &app.preview.as_ref().unwrap().text;
        assert!(
            report.contains("Edit tool declaration")
                && report.contains("Default")
                && report.contains("latest")
        );
        assert!(f.calls().is_empty());
        let task = key(&mut app, KeyCode::Enter, width, height).unwrap();
        assert!(app.confirmation.is_none() && app.preview.is_none());
        perform(&f, &mut app, task);
        key(&mut app, KeyCode::Enter, width, height);
        assert!(app.tools_view.landing());
        let task = key(&mut app, KeyCode::Char('d'), width, height).unwrap();
        perform(&f, &mut app, task);
        assert!(
            app.preview
                .as_ref()
                .unwrap()
                .text
                .contains("Remove tool declaration")
        );
        let task = key(&mut app, KeyCode::Enter, width, height).unwrap();
        perform(&f, &mut app, task);
        key(&mut app, KeyCode::Enter, width, height);
        let text = std::fs::read_to_string(f.repo.join("dort.toml")).unwrap();
        assert!(!text.contains("id='a'") && text.contains("id='b'"));
        assert!(app.tools_view.landing());
        assert!(f.host.home.join("a").exists() && f.calls().is_empty());
        assert!(
            dort_core::journal::diagnose(&f.host)
                .unwrap()
                .iter()
                .all(|d| d.journal_schema == Some(6))
        );
        app.activate(Action::Destination(Tab::Recovery));
        app.selected = app
            .data
            .as_ref()
            .unwrap()
            .history
            .iter()
            .position(|d| {
                dort_core::recovery::preview(&f.host, &d.repository_id, &d.transaction_id).is_ok()
            })
            .unwrap();
        assert!(app.page_actions().contains(&Action::Rollback));
        let task = app.activate(Action::Rollback).unwrap();
        perform(&f, &mut app, task);
        assert!(app.preview.is_some());
        let task = key(&mut app, KeyCode::Enter, width, height).unwrap();
        perform(&f, &mut app, task);
        assert!(
            std::fs::read_to_string(f.repo.join("dort.toml"))
                .unwrap()
                .contains("latest")
        );
        assert!(f.host.home.join("a").exists() && f.calls().is_empty());
    }
}
#[test]
fn removal_dry_run_and_shrink_cannot_approve_and_back_drops_preview() {
    for dry in [true, false] {
        let f = Fixture::new();
        let original = std::fs::read(f.repo.join("dort.toml")).unwrap();
        let mut app = app(&f, dry);
        let task = key(&mut app, KeyCode::Char('d'), 80, 24).unwrap();
        perform(&f, &mut app, task);
        assert!(app.preview.is_some());
        if dry {
            assert!(key(&mut app, KeyCode::Enter, 80, 24).is_none());
        } else {
            app.resize(20, 5);
            assert!(key(&mut app, KeyCode::Enter, 20, 5).is_none());
            app.resize(80, 24);
        }
        key(&mut app, KeyCode::Esc, 80, 24);
        assert!(app.preview.is_none());
        assert_eq!(std::fs::read(f.repo.join("dort.toml")).unwrap(), original);
        assert!(f.calls().is_empty());
    }
}
#[test]
fn referenced_removal_is_actionable_error_without_preview_or_commands() {
    let f = Fixture::new();
    let path = f.repo.join("dort.toml");
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("binary='b'", "binary='b'\nrequires=['a']"),
    )
    .unwrap();
    let mut app = app(&f, false);
    let task = key(&mut app, KeyCode::Char('d'), 80, 24).unwrap();
    let output = actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(()));
    assert!(output.is_err());
    app.complete(output.map_err(|e| actions::diagnostic(&e)));
    assert!(app.preview.is_none() && f.calls().is_empty());
    assert!(matches!(
        actions::perform_scoped(
            Task::Refresh,
            &f.host,
            Some(f.repo.clone()),
            &[],
            |_| Ok(())
        ),
        Ok(Output::Data(_))
    ));
}
