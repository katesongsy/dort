//! Direct-key/mouse viewport checks over synthetic metadata, with no service execution.
use super::{
    actions::{Data, ManagedRow, Output},
    app::{App, Tab},
    git::{Metadata, Report},
    tests::render,
};
use dort_core::{
    git::{Status, StatusEntry},
    journal::{Diagnosis, Phase},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(app: &mut App, code: KeyCode, size: (u16, u16)) {
    assert!(
        app.key(KeyEvent::new(code, KeyModifiers::NONE), size.0, size.1)
            .is_none(),
        "metadata browsing must not dispatch a task"
    );
}
fn long_path(prefix: &str) -> String {
    (0..100).map(|i| format!("{prefix}-{i:03}/")).collect()
}
fn fixture(tab: Tab) -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        files: vec![
            ("first".into(), "unused".into()),
            ("second".into(), "unused".into()),
        ],
        managed: ["first", "second"]
            .into_iter()
            .map(|id| ManagedRow {
                id: id.into(),
                source: format!("/synthetic/{}/source-end", long_path("source")).into(),
                target: format!("/synthetic/{id}").into(),
                state: "Blocked",
                source_kind: "Regular",
                target_kind: "Missing",
                blockers: vec!["metadata-only fixture".into()],
            })
            .collect(),
        history: ['a', 'b']
            .into_iter()
            .map(|id| Diagnosis {
                repository_id: id.to_string().repeat(64),
                transaction_id: id.to_string().repeat(64),
                journal_schema: Some(3),
                phase: Some(Phase::Running),
                valid_checkpoints: 3,
                problem: None,
            })
            .collect(),
        ..Default::default()
    })));
    app.tab = tab;
    app.git_view.accept(
        Report::Summary(Metadata::Status(Status {
            schema: 1,
            read_only: true,
            branch: vec!["branch.head main".into()],
            entries: ["first", "second"]
                .into_iter()
                .map(|id| StatusEntry {
                    kind: "renamed".into(),
                    xy: Some("R.".into()),
                    path: id.into(),
                    original_path: Some(format!("{}/original-end", long_path("original"))),
                })
                .collect(),
        })),
        true,
    );
    app
}

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
fn mouse(
    app: &mut App,
    kind: MouseEventKind,
    x: u16,
    y: u16,
    size: (u16, u16),
) -> Option<super::actions::Task> {
    render(app, size.0, size.1);
    app.mouse(
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        size.0,
        size.1,
    )
}
fn at(app: &App, needle: &str, size: (u16, u16)) -> (u16, u16) {
    let text = render(app, size.0, size.1);
    for (y, row) in text.lines().enumerate() {
        if let Some(i) = row.find(needle) {
            return (row[..i].chars().count() as u16, y as u16);
        }
    }
    panic!("missing synthetic label {needle}");
}
#[test]
fn direct_keys_select_rows_scroll_details_and_ignore_tab_without_region_focus() {
    for tab in [Tab::Files, Tab::Recovery, Tab::Git] {
        let size = (110, 24);
        let mut app = fixture(tab);
        press(&mut app, KeyCode::Down, size);
        let first = render(&app, size.0, size.1);
        press(&mut app, KeyCode::PageDown, size);
        let scrolled = render(&app, size.0, size.1);
        assert_ne!(first, scrolled);
        assert!(scrolled.contains(if tab == Tab::Recovery {
            "> bbbbbbbb"
        } else {
            if tab == Tab::Files {
                "> [ ] second"
            } else {
                "> second"
            }
        }));
        press(&mut app, KeyCode::Tab, size);
        press(&mut app, KeyCode::BackTab, size);
        assert_eq!(scrolled, render(&app, size.0, size.1));
        press(&mut app, KeyCode::Home, size);
        assert_eq!(first, render(&app, size.0, size.1));
        press(&mut app, KeyCode::Up, size);
        assert!(
            render(&app, size.0, size.1).contains(if tab == Tab::Recovery {
                "> aaaaaaaa"
            } else {
                if tab == Tab::Files {
                    "> [ ] first"
                } else {
                    "> first"
                }
            })
        );
        press(&mut app, KeyCode::Right, size);
        assert!(app.tab == tab.next());
        press(&mut app, KeyCode::Left, size);
        assert!(app.tab == tab);
        assert!(!app.git_view.sensitive() && app.confirmation.is_none());
    }
}
#[test]
fn clicks_and_wheels_use_hovered_rows_and_details_with_filtered_raw_identity() {
    let size = (110, 24);
    for tab in [Tab::Files, Tab::Recovery, Tab::Git] {
        let mut app = fixture(tab);
        let needle = if tab == Tab::Recovery {
            "bbbbbbbb"
        } else {
            "second"
        };
        let (x, y) = at(&app, needle, size);
        assert!(
            mouse(
                &mut app,
                MouseEventKind::Down(MouseButton::Left),
                x,
                y,
                size
            )
            .is_none()
        );
        assert!(
            render(&app, size.0, size.1).contains(if tab == Tab::Recovery {
                "> bbbbbbbb"
            } else {
                if tab == Tab::Files {
                    "> [ ] second"
                } else {
                    "> second"
                }
            })
        );
        let before = render(&app, size.0, size.1);
        assert!(mouse(&mut app, MouseEventKind::ScrollDown, 100, 10, size).is_none());
        assert_ne!(before, render(&app, size.0, size.1));
        assert!(mouse(&mut app, MouseEventKind::ScrollUp, 10, y, size).is_none());
        assert!(
            render(&app, size.0, size.1).contains(if tab == Tab::Recovery {
                "> aaaaaaaa"
            } else {
                if tab == Tab::Files {
                    "> [ ] first"
                } else {
                    "> first"
                }
            })
        );
        let (x, y) = at(
            &app,
            if tab == Tab::Files {
                "/ Search"
            } else {
                "/ Filter"
            },
            size,
        );
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size,
        );
        for c in needle.chars() {
            press(&mut app, KeyCode::Char(c), size);
        }
        press(&mut app, KeyCode::Enter, size);
        let (x, y) = at(&app, needle, size);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size,
        );
        if tab != Tab::Git {
            assert_eq!(app.selected, 1);
        }
        let (x, y) = at(&app, "Checks ▾", size);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size,
        );
        assert!(
            app.navigation.menu.is_some(),
            "menu should open: files={} recovery={} git={} git_text={} overlay={} busy={}",
            app.tab == Tab::Files,
            app.tab == Tab::Recovery,
            app.tab == Tab::Git,
            app.git_view.text_entry(),
            app.shell.overlay.is_some(),
            app.busy
        );
        let (x, y) = at(&app, "Manifest checks", size);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size,
        );
        assert!(app.tab == Tab::Checks);
    }
}
#[test]
fn hit_rows_follow_rendered_table_offset_and_stale_geometry_is_inert() {
    let mut app = fixture(Tab::Files);
    let data = app.data.as_mut().unwrap();
    data.files = (0..80)
        .map(|i| (format!("row-{i:02}"), "metadata".into()))
        .collect();
    data.managed.clear();
    app.selected = 79;
    let size = (48, 14);
    let (x, y) = at(&app, "row-78", size);
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        size,
    );
    assert_eq!(app.selected, 78);
    render(&app, size.0, size.1);
    app.resize(80, 24);
    assert!(
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE
            },
            80,
            24
        )
        .is_none()
    );
    assert_eq!(app.selected, 78);
    press(&mut app, KeyCode::Enter, (80, 24));
    app.resize(48, 14);
    assert!(render(&app, 48, 14).contains("Esc list"));
    press(&mut app, KeyCode::Esc, (48, 14));
    press(&mut app, KeyCode::Down, (48, 14));
    assert_eq!(app.selected, 79);
}
#[test]
fn mouse_form_controls_and_palette_actions_never_bypass_typed_approval_or_disclosure() {
    let size = (80, 24);
    let mut app = fixture(Tab::Files);
    press(&mut app, KeyCode::Char('o'), size);
    let (x, y) = at(&app, "ID", size);
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        size,
    );
    assert_eq!(app.shell.adoption.focused, 1);
    press(&mut app, KeyCode::Char('q'), size);
    press(&mut app, KeyCode::Tab, size);
    assert_eq!(app.shell.adoption.focused, 2);
    assert_eq!(app.shell.adoption.id, "q");
    press(&mut app, KeyCode::Esc, size);
    press(&mut app, KeyCode::Char(';'), size);
    for c in "delete managed link".chars() {
        press(&mut app, KeyCode::Char(c), size);
    }
    let (x, y) = at(&app, "Delete managed link (", size);
    let task = mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        size,
    );
    assert!(
        matches!(task,Some(super::actions::Task::Files(dort_core::planner::Request::Unmanage{ref id,..})) if id=="first")
    );
    assert!(app.confirmation.is_none());
    app.git_view.choose(super::git::Comparison::Unstaged);
    app.git_view.accept(
        Report::Summary(Metadata::Diff(dort_core::git::Diff {
            schema: 1,
            read_only: true,
            staged: false,
            patch: None,
            entries: vec![],
        })),
        true,
    );
    super::tests::destination(&mut app, super::app::Tab::Git, size.0, size.1);
    press(&mut app, KeyCode::Char('w'), size);
    assert!(app.git_view.sensitive());
    press(&mut app, KeyCode::Tab, size);
    assert!(
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            20,
            8,
            size
        )
        .is_none()
    );
    press(&mut app, KeyCode::Enter, size);
    assert!(app.git_view.sensitive());
    press(&mut app, KeyCode::Esc, size);
    assert!(!app.git_view.sensitive());
}
#[test]
fn mouse_buttons_use_raw_picker_paths_and_modal_hits_do_not_reach_destination_tabs() {
    use super::actions::Task;
    use super::shell::{Form, Overlay};
    let size = (80, 24);
    let mut app = fixture(Tab::Files);
    app.data.as_mut().unwrap().known_roots = vec!["/synthetic/raw-root-end".into()];
    press(&mut app, KeyCode::Char('p'), size);
    let (x, y) = at(&app, "raw-root-end", size);
    assert!(
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size
        )
        .is_none()
    );
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Repositories { selected: 2 })
    ));
    let task = app.key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        size.0,
        size.1,
    );
    assert!(
        matches!(task, Some(Task::Select(Some(ref root))) if root.to_str() == Some("/synthetic/raw-root-end"))
    );
    press(&mut app, KeyCode::Esc, size);
    press(&mut app, KeyCode::Char('n'), size);
    let (x, y) = at(&app, "Git ▾", size);
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        size,
    );
    assert!(app.tab == Tab::Files && matches!(app.shell.overlay, Some(Overlay::Form(Form::Init))));
    press(&mut app, KeyCode::Esc, size);
    press(&mut app, KeyCode::Char(';'), size);
    // Recovery commands now stay in Recovery, rather than the Files action list.
    app.shell.overlay = None;
    app.tab = Tab::Recovery;
    press(&mut app, KeyCode::Char(';'), size);
    for c in "rollback".chars() {
        press(&mut app, KeyCode::Char(c), size);
    }
    let (x, y) = at(&app, "Preview rollback", size);
    assert!(
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size
        )
        .is_none()
    );
    assert!(matches!(app.shell.overlay, Some(Overlay::Actions { .. })));
    assert!(render(&app, size.0, size.1).contains("Rollback unavailable"));
    press(&mut app, KeyCode::Esc, size); // clicked results, no hidden input focus
    app.start(false);
    let (x, y) = at(&app, "Checks ▾", size);
    assert!(
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            y,
            size
        )
        .is_none()
    );
    assert!(app.tab == Tab::Recovery && app.busy && app.navigation.menu.is_none());
    press(&mut app, KeyCode::Right, size); // task-time destination navigation
    assert!(app.tab == Tab::Checks && app.busy);
}
#[test]
fn wide_full_paths_and_ids_are_reachable_in_cells_using_only_detail_page_keys() {
    for tab in [Tab::Files, Tab::Recovery, Tab::Git] {
        let mut app = fixture(tab);
        let expected = if tab == Tab::Files {
            format!(
                "{:<13}/synthetic/{}/source-end",
                "Source",
                long_path("source")
            )
        } else if tab == Tab::Recovery {
            format!("Transaction ID: {}", "a".repeat(64))
        } else {
            format!(
                "Original path: Some(\"{}/original-end\")",
                long_path("original")
            )
        };
        let size = (110, 24);
        press(&mut app, KeyCode::Enter, size);
        let mut cells: Vec<String> = Vec::new();
        for _ in 0..80 {
            let text = render(&app, size.0, size.1);
            let rows: Vec<_> = text.lines().collect();
            let top = rows
                .iter()
                .position(|line| {
                    (line.contains("Selected item")
                        || line.contains("Transaction details")
                        || line.contains("Git details"))
                        && line.contains('┌')
                })
                .unwrap();
            let viewport: Vec<String> = rows[top + 1..]
                .iter()
                .take_while(|row| row.contains('│'))
                .map(|row| row.split('│').nth(3).unwrap().to_owned())
                .collect();
            // Stitch actual cell viewports, including the last clamped/overlapping page.
            let overlap = (0..=cells.len().min(viewport.len()))
                .rev()
                .find(|&n| cells[cells.len() - n..] == viewport[..n])
                .unwrap();
            cells.extend(viewport.into_iter().skip(overlap));
            press(&mut app, KeyCode::PageDown, size);
        }
        assert!(
            cells.concat().contains(&expected),
            "full escaped metadata must remain reachable at the wide threshold"
        );
        press(&mut app, KeyCode::Home, size);
        assert_eq!(app.details_scroll, 0);
    }
}
