//! Cell-geometry and semantic style checks, using synthetic metadata only.
use super::{
    actions::{Data, ManagedRow, Output},
    app::{App, Tab},
    shell::{Focus, Form, Overlay},
    tests::render,
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Modifier},
};
fn key(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24);
}
fn buffer(app: &App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| views::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn locate(buffer: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        if let Some(byte) = row.find(needle) {
            return (row[..byte].chars().count() as u16, y);
        }
    }
    panic!("missing fixture label: {needle}")
}
fn synthetic_host() -> dort_core::context::HostContext {
    dort_core::context::HostContext::new(
        "/synthetic/home".into(),
        None,
        None,
        dort_core::platform::Platform::Ubuntu,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap()
}
fn inspect_all(app: &mut App, w: u16, h: u16) -> String {
    let mut text = String::new();
    let mut wrapped_rows = String::new();
    for offset in 0..120 {
        app.details_scroll = offset;
        text.push_str(&render(app, w, h));
        text.push('\n');
        // Reconstruct consecutive rendered cell rows, including paths longer than
        // the entire compact viewport. No model/report strings feed this check.
        let b = buffer(app, w, h);
        let (_, title_y) = locate(&b, "Focus");
        for x in 1..w - 1 {
            wrapped_rows.push_str(b[(x, title_y + 1)].symbol());
        }
    }
    text.push_str(&wrapped_rows);
    app.details_scroll = 0;
    text
}
fn files_app() -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        repository: Some("/synthetic/repository".into()),
        files: vec![("editor".into(), "fallback must not be parsed".into())],
        managed: vec![ManagedRow {
            id: "editor".into(),
            source: "/synthetic/repository/files/editor".into(),
            target: "/synthetic/home/.editor".into(),
            state: "Blocked",
            source_kind: "Regular file",
            target_kind: "Directory",
            blockers: vec!["synthetic blocker".into()],
        }],
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app
}
#[test]
fn workbench_has_semantic_styles_and_selected_identity_at_all_supported_sizes() {
    let app = files_app();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        let active = locate(&b, "2 Files");
        assert!(b[active].modifier.contains(Modifier::BOLD));
        assert!(b[active].modifier.contains(Modifier::REVERSED));
        locate(&b, "STATE");
        let (_, row_y) = locate(&b, "> editor");
        let row: String = (0..w).map(|x| b[(x, row_y)].symbol()).collect();
        let byte = row
            .find("Blocked")
            .expect("selected row keeps its status column");
        let status = (row[..byte].chars().count() as u16, row_y);
        assert_eq!(
            b[status].bg,
            Color::Cyan,
            "selected row background at {w}x{h}"
        );
        assert_eq!(
            b[status].fg,
            Color::Black,
            "selected row foreground at {w}x{h}"
        );
        let title = locate(&b, "Focus");
        assert_eq!(b[title].fg, Color::Cyan);
        locate(&b, "Review apply");
        // Chrome occupies two normal rows or three compact rows, not nine.
        assert!(locate(&b, "Files / Managed").1 <= 3);
        assert!(!render(&app, w, h).contains("fallback must not be parsed"));
    }
}
#[test]
fn input_has_a_real_visible_caret_and_field_button_hierarchy() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    app.shell.adoption.target = "~/.editor".into();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let b = t.backend().buffer();
        let (x, y) = locate(b, "~/.editor");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 9, y).into());
        let label = locate(t.backend().buffer(), "TARGET");
        assert_eq!(t.backend().buffer()[label].fg, Color::Cyan);
    }
    app.shell.adoption.focused = 3;
    let b = buffer(&app, 48, 14);
    let action = locate(&b, "> [Review");
    assert!(b[action].modifier.contains(Modifier::REVERSED));
    assert_ne!(b[action].bg, Color::Reset);
}
#[test]
fn arrows_navigate_visible_focused_controls_not_hidden_global_tabs() {
    let mut app = files_app();
    key(&mut app, KeyCode::Right);
    assert!(app.tab == Tab::Files);
    key(&mut app, KeyCode::Tab);
    assert!(app.shell.focus == Focus::Actions);
    key(&mut app, KeyCode::Right);
    assert_eq!(app.action_selected, 1);
    assert!(render(&app, 80, 24).contains("> [Review apply"));
    key(&mut app, KeyCode::Tab);
    assert!(app.shell.focus == Focus::Destinations);
    key(&mut app, KeyCode::Right);
    assert!(app.tab == Tab::Recovery);
}
#[test]
fn typed_overview_cards_keep_unknown_distinct_from_zero_and_metadata_inspectable() {
    use super::actions::{FileCounts, Overview};
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        overview: Some(Overview {
            host: synthetic_host(),
            registered: Some(false),
            registry_count: Some(0),
            configuration_error: None,
            files: Some(FileCounts {
                linked: 2,
                changes: 1,
                blocked: 3,
                plan_blocked: true,
            }),
        }),
        repository: Some("/synthetic/repository/full-root-end".into()),
        profiles: vec!["base".into(), "dev".into()],
        ..Default::default()
    })));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let initial = render(&app, w, h);
        assert!(initial.contains("Unregistered"));
        let b = buffer(&app, w, h);
        let status = locate(&b, "Unregistered");
        assert_eq!(b[status].fg, Color::Yellow);
        assert!(b[status].modifier.contains(Modifier::BOLD));
        let details = inspect_all(&mut app, w, h);
        for value in [
            "full-root-end",
            "Linked: 2",
            "Needs apply: 1",
            "Blocked mappings: 3",
            "Plan blockers: true",
            "Global write blockers: 0",
            "Effective profiles:",
        ] {
            assert!(details.contains(value), "inspectable {value} at {w}x{h}");
        }
    }
    let data = app.data.as_mut().unwrap();
    data.overview.as_mut().unwrap().files = None;
    data.overview.as_mut().unwrap().registered = None;
    data.overview.as_mut().unwrap().registry_count = None;
    data.overview.as_mut().unwrap().configuration_error =
        Some("synthetic configuration failure".into());
    data.repository = None;
    data.history_error = Some("synthetic unavailable".into());
    // Failed observation must not erase the supplied session root or imply
    // its registration was successfully checked and found absent.
    app.repository = Some("/synthetic/session-root-end".into());
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let initial = render(&app, w, h);
        assert!(initial.contains("Unknown"));
        let details = inspect_all(&mut app, w, h);
        assert!(details.contains("File counts unknown, not zero."));
        assert!(details.contains("Registration: Unknown"));
        assert!(details.contains("session-root-end"));
        assert!(!details.contains("Welcome | manage local files"));
        assert!(details.contains("Global write blockers: Unknown"));
        assert!(!details.contains("Global write blockers: 0"));
    }
}
#[test]
fn diagnostic_cards_independently_render_registry_and_history_availability() {
    use super::actions::Diagnostics;
    for registry_ok in [true, false] {
        for history_ok in [true, false] {
            let mut app = App::new(false);
            app.complete(Ok(Output::Diagnostics(Diagnostics {
                host: synthetic_host(),
                registry: if registry_ok {
                    Ok(crate::services::doctor(
                        &synthetic_host(),
                        &dort_core::local::LocalConfig::default(),
                    ))
                } else {
                    Err("Invalid TOML; source text is not displayed.".into())
                },
                history: if history_ok {
                    Ok((0, 0))
                } else {
                    Err("synthetic history failure".into())
                },
            })));
            for (w, h) in [(48, 14), (80, 24), (140, 40)] {
                let initial = render(&app, w, h);
                assert!(
                    initial.contains(if registry_ok {
                        "Registry 0"
                    } else {
                        "Registry Unknown"
                    }) || w >= 80
                );
                let details = inspect_all(&mut app, w, h);
                assert!(details.contains(if registry_ok {
                    "Registered repositories: 0"
                } else {
                    "Registry count and active selection"
                }));
                assert!(details.contains(if history_ok {
                    "Global write blockers: 0"
                } else {
                    "Global write blockers: Unknown"
                }));
                for value in [
                    "Session file root:",
                    "Git executable:",
                    "Capability limits",
                    "Live files/tools not inspected",
                    "synthetic/home",
                ] {
                    assert!(details.contains(value), "inspectable {value} at {w}x{h}");
                }
                let b = buffer(&app, w, h);
                let focus = locate(&b, "Focus");
                assert_eq!(b[focus].fg, Color::Cyan);
            }
        }
    }
}
#[test]
fn checked_dashboard_preserves_result_scope_union_and_limits_through_diagnostics() {
    use crate::services::{CheckReport, ProfileChoices};
    let mut app = App::new(false);
    app.complete(Ok(Output::Checked {
        standalone: true,
        report: Box::new(CheckReport {
            manifest: format!("/synthetic/{}/manifest-end.toml", "long-path/".repeat(15)).into(),
            schema: 1,
            active_profiles: [
                "base".into(),
                "default".into(),
                "platform".into(),
                "local".into(),
                "optional".into(),
            ]
            .into(),
            selected_tools: 2,
            selected_files: 0,
            tool_dependency_order: vec!["first-tool".into(), "second-tool".into()],
            unsupported_capabilities: vec!["tool inventory and installation".into()],
            filesystem_and_tools_inspected: false,
            choices: ProfileChoices {
                known: Default::default(),
                defaults: vec!["default".into()],
                platform: vec!["platform".into()],
                local: vec!["local".into()],
                mandatory: [
                    "base".into(),
                    "default".into(),
                    "platform".into(),
                    "local".into(),
                ]
                .into(),
            },
        }),
    }));
    app.requested_profiles = vec!["unrelated-current-session".into()];
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        locate(&b, "Standalone manifest");
        let valid = locate(&b, "Valid");
        assert_eq!(b[valid].fg, Color::Cyan);
        assert!(b[valid].modifier.contains(Modifier::BOLD));
        let details = inspect_all(&mut app, w, h);
        for value in [
            "manifest-end.toml",
            "Defaults:",
            "Platform:",
            "Local:",
            "Optional additions:",
            "first-tool",
            "second-tool",
            "Not implemented:",
            "tool inventory",
        ] {
            assert!(details.contains(value), "inspectable {value} at {w}x{h}");
        }
        assert!(!details.contains("unrelated-current-session"));
    }
    app.complete(Ok(Output::Diagnostics(super::actions::Diagnostics {
        host: synthetic_host(),
        registry: Err("unavailable".into()),
        history: Ok((0, 0)),
    })));
    assert!(app.check.as_ref().unwrap().1);
    key(&mut app, KeyCode::Char('m'));
    assert!(!app.showing_diagnostics);
    assert!(render(&app, 80, 24).contains("Standalone manifest"));
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
fn history_row(
    schema: Option<u32>,
    phase: Option<dort_core::journal::Phase>,
    problem: Option<&str>,
    id: char,
) -> dort_core::journal::Diagnosis {
    dort_core::journal::Diagnosis {
        repository_id: id.to_string().repeat(64),
        transaction_id: id.to_string().repeat(64),
        journal_schema: schema,
        phase,
        valid_checkpoints: 7,
        problem: problem.map(str::to_owned),
    }
}
#[test]
fn recovery_table_has_typed_columns_selection_and_integrity_styles_at_all_sizes() {
    use dort_core::journal::Phase;
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: vec![
            history_row(None, None, Some("synthetic corrupt tail"), 'a'),
            history_row(Some(3), Some(Phase::Preparing), None, 'b'),
        ],
        error: None,
    }));
    key(&mut app, KeyCode::Down);
    for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        locate(&b, "Blockers: 2");
        for column in ["TX ID", "SCM", "PHASE", "STATE"] {
            locate(&b, column);
        }
        let selected = locate(&b, "> bbbbbbbb");
        let text: String = (0..w).map(|x| b[(x, selected.1)].symbol()).collect();
        let byte = text.find("Preparing").unwrap();
        let phase = (text[..byte].chars().count() as u16, selected.1);
        assert_eq!(b[phase].bg, Color::Cyan);
        assert_eq!(b[phase].fg, Color::Black);
        assert!(
            b[phase]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        let integrity = locate(&b, "INTEGRITY!");
        assert_eq!(b[integrity].fg, Color::Red);
        let focus = locate(&b, "Focus");
        assert_eq!(b[focus].fg, Color::Cyan);
        assert_eq!(app.selected, 1);
        if w >= 110 {
            locate(&b, "Transaction details");
        }
    }
    app.history_search.editing = true;
    app.history_search.query = format!("q?/{}\u{1b}needle", "é".repeat(50));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let (x, y) = locate(t.backend().buffer(), "needle");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 6, y).into());
        locate(t.backend().buffer(), "Filter > ...");
        let screen = render(&app, w, h);
        assert!(!screen.contains('\u{1b}') && !screen.contains('é'));
        assert!(screen.contains("Blockers: 2"));
    }
}
#[test]
fn recovery_details_keep_full_ids_and_guards_inspectable_without_mutating_diagnosis() {
    use dort_core::journal::Phase;
    for (schema, phase, problem) in [
        (Some(1), Some(Phase::Completed), None),
        (Some(2), Some(Phase::RollingBack), None),
        (Some(3), Some(Phase::Preparing), None),
        (Some(3), Some(Phase::Running), None),
        (Some(3), Some(Phase::Completed), None),
        (None, None, Some("unsafe\u{1b}[31m tail")),
    ] {
        let mut app = App::new(false);
        app.tab = Tab::Recovery;
        app.complete(Ok(Output::History {
            rows: vec![history_row(schema, phase, problem, 'a')],
            error: None,
        }));
        let before = serde_json::to_value(&app.data.as_ref().unwrap().history).unwrap();
        key(&mut app, KeyCode::Enter);
        for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
            let mut all = String::new();
            let mut cells = String::new();
            for offset in 0..55 {
                app.details_scroll = offset;
                all.push_str(&render(&app, w, h));
                let b = buffer(&app, w, h);
                let (x, y) = locate(&b, "Transaction details");
                // The title begins two cells after its pane's left border.
                for col in x - 1..w - 1 {
                    cells.push_str(b[(col, y + 1)].symbol());
                }
                // Also join each full visible pane, including the final rows
                // below the clamped scroll start at the end of the document.
                for line in y + 1..h - 5 {
                    for col in x - 1..w - 1 {
                        all.push_str(b[(col, line)].symbol());
                    }
                }
            }
            all.push_str(&cells);
            for value in [
                "Repository ID: ",
                "Transaction ID: ",
                "Validated checkpoints: 7",
                "Integrity:",
                "Blocks new writes:",
                "Core preview independently checks live paths and artifacts.",
                "Schema-1 rollback withdraws its new source into private storage;",
                "no garbage collection.",
            ] {
                assert!(all.contains(value), "full typed detail {value} at {w}x{h}");
            }
            assert!(all.contains(&"a".repeat(64)));
            assert!(!all.contains('\u{1b}'));
            app.details_scroll = u16::MAX;
            let end = render(&app, w, h);
            assert!(end.contains("Blockers:"));
            key(&mut app, KeyCode::Home);
            assert_eq!(app.details_scroll, 0);
        }
        assert_eq!(
            before,
            serde_json::to_value(&app.data.as_ref().unwrap().history).unwrap()
        );
        assert!(app.preview.is_none() && app.confirmation.is_none());
    }
}
fn git_app(metadata: super::git::Metadata) -> App {
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.root = Some("/synthetic/independent-git-root".into());
    app.git_view.follows_files = false;
    if let super::git::Metadata::Diff(diff) = &metadata {
        app.git_view.choose(if diff.staged {
            super::git::Comparison::Staged
        } else {
            super::git::Comparison::Unstaged
        });
    }
    app.git_view
        .accept(super::git::Report::Summary(metadata), true);
    app
}
#[test]
fn git_metadata_tables_have_typed_columns_focus_and_selected_styles_at_all_sizes() {
    use dort_core::git::{Diff, DiffEntry, Status, StatusEntry};
    let mut app = git_app(super::git::Metadata::Status(Status {
        schema: 1,
        read_only: true,
        branch: vec!["branch.head main".into()],
        entries: vec![
            StatusEntry {
                kind: "unmerged".into(),
                xy: Some("UU".into()),
                path: "conflict".into(),
                original_path: None,
            },
            StatusEntry {
                kind: "renamed".into(),
                xy: Some("R.".into()),
                path: "new-name".into(),
                original_path: Some("old-name".into()),
            },
        ],
    }));
    key(&mut app, KeyCode::Down);
    for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        for column in ["PATH", "KIND", "XY"] {
            locate(&b, column);
        }
        let selected = locate(&b, "> new-name");
        let row: String = (0..w).map(|x| b[(x, selected.1)].symbol()).collect();
        let byte = row.find("renamed").unwrap();
        let status = (row[..byte].chars().count() as u16, selected.1);
        assert_eq!(b[status].bg, Color::Cyan);
        assert_eq!(b[status].fg, Color::Black);
        assert!(
            b[status]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(b[locate(&b, "unmerged")].fg, Color::Red);
        assert_eq!(b[locate(&b, "Focus")].fg, Color::Cyan);
        if w >= 110 {
            locate(&b, "Git details");
        }
    }
    let app = git_app(super::git::Metadata::Diff(Diff {
        schema: 1,
        read_only: true,
        staged: true,
        patch: None,
        entries: vec![DiffEntry {
            change: "M".into(),
            old_mode: "100644".into(),
            new_mode: "100755".into(),
            path: "changed-file".into(),
        }],
    }));
    for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
        let b = buffer(&app, w, h);
        for column in ["PATH", "CHANGE", "MODES"] {
            locate(&b, column);
        }
        locate(&b, "100644 -> 100755");
        locate(&b, "Staged (index vs HEAD");
        locate(&b, "> changed-file");
    }
}
#[test]
fn git_metadata_full_paths_branch_and_limits_are_reachable_using_real_scroll_keys() {
    use dort_core::git::{Status, StatusEntry};
    let path = format!("{}path-end\u{202e}", "long-path/".repeat(20));
    let original = format!("{}old\nname", "original/".repeat(20));
    let mut app = git_app(super::git::Metadata::Status(Status {
        schema: 1,
        read_only: true,
        branch: vec!["branch.oid (initial)".into(), "branch.head main".into()],
        entries: vec![StatusEntry {
            kind: "renamed".into(),
            xy: Some("R.".into()),
            path: path.clone(),
            original_path: Some(original.clone()),
        }],
    }));
    app.git_view.root = Some(format!("/synthetic/{}/root-end", "scope/".repeat(30)).into());
    key(&mut app, KeyCode::Enter);
    for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
        key(&mut app, KeyCode::Home);
        let first = render(&app, w, h);
        let mut all = String::new();
        let mut consecutive = String::new();
        for _ in 0..60 {
            let b = buffer(&app, w, h);
            let (x, y) = locate(&b, "Git details");
            for line in y + 1..h - 5 {
                for col in x - 1..w - 1 {
                    all.push_str(b[(col, line)].symbol());
                }
            }
            // Git's ordinary metadata scroll advances two rows: capture these
            // consecutive rows to verify paths longer than the whole viewport.
            for line in y + 1..y + 3 {
                for col in x - 1..w - 1 {
                    consecutive.push_str(b[(col, line)].symbol());
                }
            }
            app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), w, h);
        }
        all.push_str(&consecutive);
        for value in [
            views::safe(&format!("Path: {path:?}")),
            views::safe(&format!("Original path: {:?}", Some(&original))),
            "root-end".into(),
            "independent session choice".into(),
            "branch.oid (initial)".into(),
            "branch.head main".into(),
            "Metadata boundary".into(),
            "No patch content requested.".into(),
        ] {
            assert!(
                all.contains(&value),
                "inspectable typed Git metadata at {w}x{h}: {value}"
            );
        }
        assert!(!all.contains('\u{202e}') && !all.contains('\u{1b}'));
        app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), w, h);
        assert_eq!(first, render(&app, w, h));
    }
    assert!(!app.git_view.sensitive() && app.preview.is_none() && app.confirmation.is_none());
}
#[test]
fn git_metadata_unknown_empty_and_filter_caret_remain_distinct_at_the_floor() {
    use dort_core::git::Status;
    let mut app = git_app(super::git::Metadata::Status(Status {
        schema: 1,
        read_only: true,
        branch: vec![],
        entries: vec![],
    }));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, w, h);
        assert!(screen.contains("0 entries") && screen.contains("No changes"));
        key(&mut app, KeyCode::Enter);
        assert!(render(&app, w, h).contains("No changes in this comparison."));
        key(&mut app, KeyCode::Esc);
    }
    app.git_view.metadata = None;
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, w, h);
        assert!(screen.contains("Unknown") && screen.contains("Summary unavailable/not loaded"));
        assert!(!screen.contains("0 entries") && !screen.contains("No changes"));
        key(&mut app, KeyCode::Enter);
        assert!(render(&app, w, h).contains("Summary unavailable/not loaded"));
        key(&mut app, KeyCode::Esc);
    }
    app.git_view.search.editing = true;
    app.git_view.search.query = format!("q?/{}needle", "é\u{1b}".repeat(30));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let (x, y) = locate(t.backend().buffer(), "needle");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 6, y).into());
        locate(t.backend().buffer(), "Filter > ...");
        let screen = render(&app, w, h);
        assert!(!screen.contains('é') && !screen.contains('\u{1b}'));
    }
}
#[test]
fn single_path_field_cannot_submit_until_the_visible_action_is_activated() {
    let mut app = App::new(false);
    app.git = Ok("/trusted/git".into());
    key(&mut app, KeyCode::Char('n'));
    app.shell.init_draft = "/synthetic/new-repository".into();
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24)
            .is_none()
    );
    assert_eq!(app.shell.path_focused, 1);
    assert!(render(&app, 80, 24).contains("> [Review read-only preview"));
    assert!(matches!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24),
        Some(super::actions::Task::Init { .. })
    ));
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
