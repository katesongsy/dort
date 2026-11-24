use super::{
    actions::Task,
    app::Tab,
    input::Target,
    shell::Action,
    tests::render,
    tools_tests::{app, check, key, run, settings},
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn footer_preserves_context_relevance_then_more_back_help_at_every_size() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    render(&app, 48, 14);
    let (root, _) = super::workbench::strip_actions(&app, 48);
    assert!(
        root.iter().any(|a| a.key == "C"),
        "Check stays visible beside Details"
    );
    assert!(
        !root.iter().any(|a| a.key == "Esc"),
        "A root list has no inert Back action"
    );
    check(&f, &mut app);
    for (width, height) in [(48, 14), (80, 24), (140, 40)] {
        render(&app, width, height);
        let (shown, hidden) = super::workbench::strip_actions(&app, width);
        let keys: Vec<_> = shown.iter().map(|a| a.key).collect();
        assert_eq!(keys.last(), Some(&"F1"));
        if let Some(back) = keys.iter().position(|k| *k == "Esc") {
            assert_eq!(back + 1, keys.len() - 1);
            if let Some(more) = keys.iter().position(|k| *k == "F2") {
                assert!(more < back);
            }
        }
        let ordered: Vec<_> = shown
            .iter()
            .map(|a| a.key)
            .chain(hidden.iter().map(|a| a.key))
            .filter(|k| ["Enter", "i", "C", "e", "a", "d"].contains(k))
            .collect();
        assert_eq!(ordered, ["Enter", "i", "C", "e", "a", "d"]);
        assert_eq!(shown[0].key, "Enter");
        assert!(shown.len() <= 8);
        let hidden_labels: Vec<_> = hidden.iter().map(|a| a.label).collect();
        if !hidden.is_empty() {
            app.key(
                ratatui::crossterm::event::KeyEvent::new(
                    KeyCode::F(2),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
                width,
                height,
            );
            let menu = app.more.as_ref().expect("Same partition opens More");
            for label in hidden_labels {
                assert!(menu.items.iter().any(|item| item.action.label == label));
            }
            key(&mut app, KeyCode::Esc);
        }
    }
    assert_eq!(f.calls(), "Q");
}
#[test]
fn more_can_prepare_check_review_but_never_consumes_approval() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    check(&f, &mut app);
    render(&app, 48, 14);
    app.key(
        ratatui::crossterm::event::KeyEvent::new(
            KeyCode::F(2),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ),
        48,
        14,
    );
    let menu = app.more.as_mut().unwrap();
    menu.selected = menu
        .items
        .iter()
        .position(|item| item.action.label == "Check installed")
        .unwrap();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolsCheckPrepare(_)));
    // Use explicit fixture detection when this newly selected task is prepared.
    let Task::ToolsCheckPrepare(mut inputs) = task else {
        unreachable!()
    };
    inputs.search_dirs = vec![f.bin.clone()];
    run(&f, &mut app, Task::ToolsCheckPrepare(inputs));
    assert_eq!(f.calls(), "Q");
    assert!(app.tools_view.declared.check_preview.is_some());
    let review = render(&app, 48, 14);
    assert!(review.lines().last().unwrap().contains("End Read"));
    assert!(
        app.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), 48, 14)
            .is_none()
    );
    render(&app, 48, 14);
    let (shown, hidden) = super::workbench::strip_actions(&app, 48);
    assert!(shown.iter().any(|a| a.key == "y"));
    assert!(
        !hidden
            .iter()
            .any(|a| matches!(a.target, Target::Action(Action::ToolsCheck)))
    );
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared.check_preview.is_some() && f.calls() == "Q");
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared.check_preview.is_none());
}
#[test]
fn back_returns_to_the_actual_tool_parent_with_retained_drafts() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    settings(&f, &mut app);
    app.tools_view.settings.fields[0] = "retained setting".into();
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared_details);
    let task = key(&mut app, KeyCode::Char('s')).unwrap();
    run(&f, &mut app, task);
    assert_eq!(app.tools_view.settings.fields[0], "retained setting");
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('e'));
    app.tools_view.declaration.fields[1] = "retained-command".into();
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared_details);
    key(&mut app, KeyCode::Char('e'));
    assert_eq!(app.tools_view.declaration.fields[1], "retained-command");
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.landing() && !app.tools_view.declared_details);
    assert!(f.calls().is_empty());
}
#[test]
fn tools_collections_share_numbered_tabs_clicks_and_guarded_keyboard_navigation() {
    use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    for (width, height) in [(48, 14), (80, 24), (140, 40)] {
        let f = Fixture::new();
        let mut app = app(&f, false);
        let text = render(&app, width, height);
        for label in ["1 Declared", "2 Installed", "3 History"] {
            assert!(text.contains(label));
        }
        assert!(super::workbench::actions(&app).iter().all(|action| {
            !matches!(
                action.target,
                Target::Action(Action::ToolsInstalled | Action::ToolAttempts)
            )
        }));
        let position = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .find(|(x, y)| {
                matches!(
                    app.hits.target(*x, *y),
                    Some(Target::LocalDestination(6, 1))
                )
            })
            .expect("shared clickable Installed tab");
        let task = app
            .mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: position.0,
                    row: position.1,
                    modifiers: KeyModifiers::NONE,
                },
                width,
                height,
            )
            .unwrap();
        let Task::ToolsInstalledPrepare(mut inputs) = task else {
            panic!("process-free inspection preparation")
        };
        inputs.search_dirs = vec![f.bin.clone()];
        run(&f, &mut app, Task::ToolsInstalledPrepare(inputs));
        assert!(
            !super::navigation::local_available(&app),
            "review retains its Back path"
        );
        assert!(key(&mut app, KeyCode::Char('3')).is_none());
        assert!(app.tools_view.installed.preview.is_some());
        key(&mut app, KeyCode::Esc);
        assert!(super::navigation::local_available(&app));
        key(&mut app, KeyCode::Char('/'));
        assert!(key(&mut app, KeyCode::Char('3')).is_none());
        assert_eq!(app.tools_view.installed.filter, "3");
        assert!(app.tools_view.installed_open);
        key(&mut app, KeyCode::Esc);
        let history = key(&mut app, KeyCode::Char('3')).unwrap();
        assert!(matches!(history, Task::ToolAttempts));
        run(&f, &mut app, history);
        assert!(super::navigation::local_available(&app));
        assert!(render(&app, width, height).contains("3 History"));
        if let Some(task) = key(&mut app, KeyCode::Char('1')) {
            run(&f, &mut app, task);
        }
        assert!(app.tools_view.at_root());
        key(&mut app, KeyCode::Char('a'));
        assert!(!super::navigation::local_available(&app));
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Char('2'));
        assert_eq!(app.tools_view.declaration.fields[0], "2");
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Esc);
        for letter in ['v', 'h'] {
            assert!(key(&mut app, KeyCode::Char(letter)).is_none());
            assert!(app.tab == Tab::Tools && app.tools_view.at_root());
        }
        assert!(
            f.calls().is_empty(),
            "tabs, filtering and drafts execute no provider"
        );
    }
}

#[test]
fn task_actions_are_not_local_destinations_and_shell_keys_remain_global() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    let labels: Vec<_> = super::navigation::local_catalog(6)
        .iter()
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(labels, ["1 Declared", "2 Installed", "3 History"]);
    key(&mut app, KeyCode::Char('c'));
    assert!(app.tab == Tab::Checks);
    key(&mut app, KeyCode::Char('t'));
    assert!(app.tab == Tab::Tools);
    let task = key(&mut app, KeyCode::Char('C')).unwrap();
    assert!(matches!(task, Task::ToolsCheckPrepare(_)));
    assert!(f.calls().is_empty());
}
