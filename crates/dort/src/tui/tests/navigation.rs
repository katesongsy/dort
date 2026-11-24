//! Files navigation and click semantics use typed rows, never displayed labels.
use super::{
    actions::{Data, Output},
    app::{App, Tab},
    input::Target,
    tests::render,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::{Duration, Instant};

fn app() -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        files: (0..100)
            .map(|i| (format!("/synthetic/.row-{i:03}"), "metadata".into()))
            .collect(),
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app.resize(140, 40);
    render(&app, 140, 40);
    app
}
fn key(app: &mut App, code: KeyCode) {
    assert!(
        app.key(KeyEvent::new(code, KeyModifiers::NONE), 140, 40)
            .is_none()
    );
}
fn ctrl(app: &mut App, c: char) {
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
            140,
            40
        )
        .is_none()
    );
}
#[test]
fn vim_pages_use_the_rendered_viewport_and_filters_keep_text() {
    let mut app = app();
    let page = app.inventory.viewport(false).page();
    assert!(page > 1);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.selected, 1);
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(app.selected, 0);
    ctrl(&mut app, 'f');
    assert_eq!(app.selected, page);
    ctrl(&mut app, 'd');
    assert_eq!(app.selected, page + page / 2);
    ctrl(&mut app, 'u');
    assert_eq!(app.selected, page);
    ctrl(&mut app, 'b');
    assert_eq!(app.selected, 0);
    key(&mut app, KeyCode::Char('G'));
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.selected, 0);
    key(&mut app, KeyCode::Char('G'));
    app.resize(80, 24);
    app.resize(140, 40);
    assert_eq!(app.selected, 99);
    key(&mut app, KeyCode::Char(' '));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    assert!(app.shell.overlay.is_none());
    key(&mut app, KeyCode::Char('/'));
    for c in ['g', 'g', ' ', ';', 'j', 'k'] {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.inventory.managed.query, "gg ;jk");
    assert!(!app.quit);
}
#[test]
fn selected_keep_uses_u_while_jk_and_ctrl_u_keep_their_navigation_meaning() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = app();
        app.selected = 20;
        render(&app, w, h);
        for (code, want) in [(KeyCode::Char('j'), 21), (KeyCode::Char('k'), 20)] {
            assert!(
                app.key(KeyEvent::new(code, KeyModifiers::NONE), w, h)
                    .is_none()
            );
            assert_eq!(app.selected, want);
            assert!(app.preview.is_none());
        }
        let page = app.inventory.viewport(false).page();
        assert!(
            app.key(
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                w,
                h
            )
            .is_none()
        );
        assert_eq!(app.selected, 20usize.saturating_sub((page / 2).max(1)));
        let selected = app.data.as_ref().unwrap().files[app.selected].0.clone();
        let task = app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE), w, h);
        assert!(
            matches!(task, Some(super::actions::Task::Files(dort_core::planner::Request::Unmanage { id, mode: dort_core::planner::UnmanageMode::Keep })) if id == selected)
        );
        assert!(app.preview.is_none());
    }
}
fn row_point(app: &App, raw: usize) -> (u16, u16) {
    (0..40)
        .flat_map(|y| (0..70).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x,y),Some(Target::Row(i)) if i==raw))
        .unwrap()
}
fn click(app: &mut App, point: (u16, u16), at: Instant) {
    assert!(
        app.mouse_at(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: point.0,
                row: point.1,
                modifiers: KeyModifiers::NONE
            },
            140,
            40,
            at
        )
        .is_none()
    );
}
#[test]
fn double_click_marks_once_without_a_draw_and_keyboard_timeout_resize_invalidate() {
    let mut app = app();
    let now = Instant::now();
    let point = row_point(&app, 1);
    click(&mut app, point, now);
    assert_eq!(app.selected, 1);
    assert!(app.inventory.managed_marks.is_empty());
    click(&mut app, point, now + Duration::from_millis(100));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    assert!(app.preview.is_none());
    render(&app, 140, 40);
    let point = row_point(&app, 2);
    click(&mut app, point, now);
    click(&mut app, point, now + Duration::from_millis(401));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    key(&mut app, KeyCode::Char('j'));
    render(&app, 140, 40);
    click(&mut app, point, now + Duration::from_millis(450));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    app.resize(80, 24);
    app.resize(140, 40);
    render(&app, 140, 40);
    click(&mut app, point, now + Duration::from_millis(500));
    assert_eq!(app.inventory.managed_marks.len(), 1);
    click(&mut app, point, now + Duration::from_millis(550));
    assert_eq!(app.inventory.managed_marks.len(), 2);
}

pub(super) fn assert_review_list_navigation(
    app: &mut App,
    is_row: impl Fn(Target, usize) -> bool,
    previous_details: &str,
    last_details: &str,
) {
    use super::tests::target_position;
    let right_contains = |screen: &str, value: &str| {
        screen
            .lines()
            .any(|line| line.chars().skip(58).collect::<String>().contains(value))
    };
    let initial = render(app, 140, 40);
    key(app, KeyCode::PageDown);
    let paged = render(app, 140, 40);
    assert_ne!(paged, initial);
    key(app, KeyCode::PageUp);
    assert_eq!(render(app, 140, 40), initial);
    ctrl(app, 'f');
    assert_eq!(render(app, 140, 40), paged);
    ctrl(app, 'b');
    assert_eq!(render(app, 140, 40), initial);

    key(app, KeyCode::End);
    let end = render(app, 140, 40);
    assert!(right_contains(&end, last_details), "{end}");
    let position = |app: &App, index| target_position(app, 140, 40, |t| is_row(t, index)).1;
    let previous_y = position(app, 78);
    let last_y = position(app, 79);
    assert_eq!(previous_y + 1, last_y);
    key(app, KeyCode::Up);
    let up = render(app, 140, 40);
    assert_eq!(position(app, 78), previous_y);
    assert_eq!(position(app, 79), last_y);
    assert!(right_contains(&up, previous_details), "{up}");
    assert!(!right_contains(&up, last_details), "{up}");
    let active_title = |screen: &str| {
        screen
            .lines()
            .find(|line| line.contains("[Active]"))
            .unwrap()
            .split("[Active]")
            .next()
            .unwrap()
            .trim()
            .to_string()
    };
    let queue_title = active_title(&up);
    key(app, KeyCode::Tab);
    let reading = render(app, 140, 40);
    assert!(right_contains(&reading, "[Active]"), "{reading}");
    assert_eq!(reading.matches("[Active]").count(), 1, "{reading}");
    key(app, KeyCode::Up);
    key(app, KeyCode::Char('y'));
    let moved = render(app, 140, 40);
    assert_eq!(position(app, 78), previous_y);
    assert_eq!(position(app, 79), last_y);
    assert!(right_contains(&moved, previous_details), "{moved}");
    let compact = render(app, 80, 24);
    assert_eq!(compact.matches("[Active]").count(), 1, "{compact}");
    assert_ne!(active_title(&compact), queue_title);
    assert!(
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 80, 24)
            .is_none()
    );
    let returned = render(app, 140, 40);
    assert_eq!(active_title(&returned), queue_title);
    assert!(right_contains(&returned, previous_details), "{returned}");
    assert!(app.preview.is_none());
}

#[test]
fn retained_review_viewports_keep_action_rows_aligned_for_recovery_git_and_ignored_paths() {
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: (0..80)
            .map(|i| dort_core::journal::Diagnosis {
                repository_id: format!("{i:064x}"),
                transaction_id: format!("{:064x}", i + 100),
                journal_schema: Some(2),
                phase: Some(dort_core::journal::Phase::Completed),
                valid_checkpoints: 3,
                problem: None,
            })
            .collect(),
        error: None,
    }));
    assert_review_list_navigation(
        &mut app,
        |t, i| matches!(t, Target::Row(n) if i == n),
        "4e",
        "4f",
    );
    key(&mut app, KeyCode::Char('/'));
    let before = render(&app, 140, 40);
    ctrl(&mut app, 'f');
    ctrl(&mut app, 'b');
    assert_eq!(render(&app, 140, 40), before);

    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view
        .accept(super::git::Metadata::Status(dort_core::git::Status {
            schema: 1,
            read_only: true,
            branch: vec![],
            entries: (0..80)
                .map(|i| dort_core::git::StatusEntry {
                    kind: "renamed".into(),
                    xy: Some("R.".into()),
                    path: format!("file-{i:03}"),
                    original_path: Some(format!("old-{i:03}")),
                })
                .collect(),
        }));
    assert_review_list_navigation(
        &mut app,
        |t, i| matches!(t, Target::Row(n) if i == n),
        "old-078",
        "old-079",
    );

    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.complete(Ok(Output::IgnoredPaths(
        (0..80)
            .map(|i| format!("/synthetic/.ignored-{i:03}").into())
            .collect(),
    )));
    assert_review_list_navigation(
        &mut app,
        |t, i| matches!(t, Target::Picker(n, false) if i == n),
        "ignored-078",
        "ignored-079",
    );
}

#[test]
fn declared_tools_project_the_selected_command_beside_the_list() {
    use dort_core::model::{InstallSpec, Provider, Tool};
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.complete(Ok(Output::Data(Data {
        tools: Some(Ok((0..80)
            .map(|i| Tool {
                id: format!("tool-{i:03}"),
                binary: format!("command-{i:03}"),
                profiles: vec!["base".into()],
                requires: vec![],
                install: InstallSpec {
                    provider: Provider::Brew,
                    package: format!("package-{i:03}"),
                    version: None,
                    runtime_tool: None,
                },
            })
            .collect())),
        ..Default::default()
    })));
    assert_review_list_navigation(
        &mut app,
        |t, i| matches!(t, Target::ToolsRow(n) if i == n),
        "command-078",
        "command-079",
    );
    assert!(!render(&app, 80, 24).contains("command-078"));
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24)
            .is_none()
    );
    assert!(render(&app, 80, 24).contains("command-078"));
}

#[test]
fn files_show_the_contextual_actions_that_their_keys_dispatch() {
    use super::{actions::Task, shell::Action, workbench};
    let mut app = app();
    for (discovering, expected) in [
        (
            false,
            vec![
                (Action::Keep, "u", "Unmanage"),
                (Action::KeepMarked, "U", "Unmanage marked"),
                (Action::ToggleMark, "Space", "Mark/unmark"),
                (Action::Adopt, "m", "Manual adopt"),
                (Action::Apply, "a", "Apply"),
                (Action::ApplyMarked, "A", "Apply marked"),
            ],
        ),
        (
            true,
            vec![
                (Action::AdoptSelected, "a", "Adopt"),
                (Action::BatchAdopt, "A", "Adopt marked"),
                (Action::IgnoreSelected, "i", "Ignore"),
            ],
        ),
    ] {
        app.discovering = discovering;
        render(&app, 140, 40);
        let actions = workbench::actions(&app);
        for (action, key, label) in expected {
            assert!(
                actions.iter().any(
                    |a| matches!(a.target, Target::Action(value) if value == action)
                        && a.key == key
                        && a.label == label
                ),
                "missing {key} {label}"
            );
        }
    }
    app.discovering = false;
    let request = app.key(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        140,
        40,
    );
    assert!(
        matches!(request, Some(Task::Files(dort_core::planner::Request::Unmanage { id, mode: dort_core::planner::UnmanageMode::Remove })) if id == "/synthetic/.row-000")
    );
    assert!(app.preview.is_none());
}
