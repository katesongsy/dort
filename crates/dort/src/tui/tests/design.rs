//! Cell-geometry and semantic style checks, using synthetic metadata only.
use super::{
    actions::{Data, ManagedRow, Output},
    app::{App, Tab},
    components as ui,
    shell::{Form, Overlay},
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
fn assert_style(buffer: &Buffer, text: &str, color: Color, emphasis: Modifier) {
    let (x, y) = locate(buffer, text);
    for col in x..x + text.len() as u16 {
        let cell = &buffer[(col, y)];
        assert_eq!((cell.fg, cell.modifier), (color, emphasis), "{text}");
    }
}
fn report_title(buffer: &Buffer) -> (u16, u16) {
    for y in 0..buffer.area.height {
        if (0..buffer.area.width).any(|x| buffer[(x, y)].symbol() == "─")
            && let Some(x) = (0..buffer.area.width).find(|&x| buffer[(x, y)].fg == Color::Cyan)
        {
            return (x, y);
        }
    }
    panic!("missing accented report title");
}
fn synthetic_host() -> dort_core::context::HostContext {
    dort_core::context::HostContext::new(
        "/synthetic/home".into(),
        None,
        None,
        dort_core::platform::Platform::Linux,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap()
}
fn inspect_all(app: &mut App, w: u16, h: u16) -> String {
    if app.tab == Tab::Overview && !app.overview_details {
        app.key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE), w, h);
    }
    let mut text = String::new();
    let mut wrapped_rows = String::new();
    for offset in 0..120 {
        app.details_scroll = offset;
        text.push_str(&render(app, w, h));
        text.push('\n');
        // Reconstruct consecutive rendered cell rows, including paths longer than
        // the entire compact viewport. No model/report strings feed this check.
        let b = buffer(app, w, h);
        let (left, title_y) = report_title(&b);
        for x in left..w {
            wrapped_rows.push_str(b[(x, title_y + 1)].symbol());
        }
        // At the clamped end, the final wrapped lines never become the first
        // viewport row. Inspect those actual cells too, not model strings.
        for y in title_y + 1..h - 1 {
            for x in left..w {
                text.push_str(b[(x, y)].symbol());
            }
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
            source: Some("/synthetic/repository/files/editor".into()),
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
        let active = locate(&b, "Managed");
        assert!(b[active].modifier.contains(Modifier::BOLD));
        assert!(!b[active].modifier.contains(Modifier::REVERSED)); // Location is a fact, not selection.
        locate(&b, "STATE");
        let (_, row_y) = locate(&b, "› [ ] editor");
        let row: String = (0..w).map(|x| b[(x, row_y)].symbol()).collect();
        let byte = row
            .find("Blocked")
            .expect("selected row keeps its status column");
        let status = (row[..byte].chars().count() as u16, row_y);
        assert!(
            b[status]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD),
            "selected row at {w}x{h}"
        );
        assert_eq!(
            b[status].fg,
            Color::Yellow,
            "Blocked keeps its independent attention fact"
        );
        let title = report_title(&b);
        assert_eq!(b[title].fg, Color::Cyan);
        assert!(super::tests::has_action(&app, "Apply"));
        // Chrome occupies two normal rows or three compact rows, not nine.
        assert!(locate(&b, "Files / Managed").1 <= 3);
        assert!(!render(&app, w, h).contains("fallback must not be parsed"));
    }
}
#[test]
fn file_details_show_selected_blocker_before_file_types_and_old_result() {
    let mut app = files_app();
    app.result = Some("Earlier operation finished".into());
    let (w, h) = (80, 40);
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), w, h);
    let b = buffer(&app, w, h);
    assert_style(&b, "Status:", Color::Reset, Modifier::BOLD);
    assert_style(&b, "Reason:", Color::Reset, Modifier::BOLD);
    assert_style(&b, "synthetic blocker", Color::Red, Modifier::BOLD);
    assert_style(&b, "File types", Color::Cyan, Modifier::BOLD);
    assert!(locate(&b, "Status:").1 < locate(&b, "synthetic blocker").1);
    assert!(locate(&b, "synthetic blocker").1 < locate(&b, "File types").1);
    assert!(locate(&b, "File types").1 < locate(&b, "Earlier operation finished").1);
    assert!(app.preview.is_none());
}
#[test]
fn routine_feedback_uses_default_text_above_the_keys() {
    let mut app = App::new(false);
    app.status = "Check finished.".into();
    let screen = buffer(&app, 80, 24);
    let at = locate(&screen, "Check finished.");
    assert_eq!(at.1, 22);
    assert_eq!(screen[at].fg, ratatui::style::Color::Reset);
}

#[test]
fn input_has_a_real_visible_caret_and_field_button_hierarchy() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    super::tests::focus_form(&mut app);
    app.shell.adoption.target = "~/.editor".into();
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let b = t.backend().buffer();
        let (x, y) = locate(b, "~/.editor");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 9, y).into());
        let label = locate(t.backend().buffer(), "TARGET");
        assert!(
            t.backend().buffer()[label]
                .modifier
                .contains(Modifier::BOLD)
        );
    }
    for _ in 0..super::shell::AdoptionDraft::FIELDS {
        key(&mut app, KeyCode::Tab);
    }
    assert_eq!(app.shell.adoption.focused, 0);
    let b = buffer(&app, 48, 14);
    let action = locate(&b, "Enter Review");
    assert!(
        b[action]
            .modifier
            .contains(Modifier::REVERSED | Modifier::BOLD)
    );
    assert_eq!(b[action].fg, b[(0, 0)].fg);
    assert_ne!(b[action].fg, b[action].bg);
}
#[test]
fn arrows_and_tab_do_not_change_destinations() {
    let mut app = files_app();
    key(&mut app, KeyCode::Tab);
    assert!(app.tab == Tab::Files);
    key(&mut app, KeyCode::Right);
    assert!(app.tab == Tab::Files);
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::Left);
    assert!(app.tab == Tab::Files && app.preview.is_none());
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
            "Apply plan: Blocked",
            "Blocked writes: 0",
            "Effective profiles:",
        ] {
            assert!(details.contains(value), "inspectable {value} at {w}x{h}");
        }
    }
    let b = buffer(&app, 100, 40);
    assert_style(
        &b,
        "Writing requires registration.",
        Color::Red,
        Modifier::BOLD,
    );
    assert_style(
        &b,
        "Attach this repository.",
        Color::Reset,
        Modifier::empty(),
    );
    assert!(locate(&b, "Attach this repository.").1 < locate(&b, "Selected by:").1);
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
        assert!(details.contains("File counts unavailable."));
        assert!(details.contains("Registration: Unknown"));
        assert!(details.contains("session-root-end"));
        assert!(!details.contains("Welcome | manage local files"));
        assert!(details.contains("Blocked writes: Unknown"));
        assert!(!details.contains("Blocked writes: 0"));
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
                assert!(initial.contains("Registry"));
                let card = buffer(&app, w, h);
                let (x, y) = locate(&card, "Registry");
                let expected = if registry_ok { "0" } else { "Unknown" };
                let value: String = (x - 1..x - 1 + expected.len() as u16)
                    .map(|col| card[(col, y + 1)].symbol())
                    .collect();
                assert_eq!(
                    value, expected,
                    "Independent registry availability card at {w}x{h}"
                );
                let details = inspect_all(&mut app, w, h);
                assert!(details.contains(if registry_ok {
                    "Registered repositories: 0"
                } else {
                    "Repository list unavailable"
                }));
                assert!(details.contains(if history_ok {
                    "Blocked writes: 0"
                } else {
                    "Blocked writes: Unknown"
                }));
                for value in [
                    "Session file root:",
                    "Git executable:",
                    "Limits",
                    "Files shows live state",
                    "synthetic/home",
                ] {
                    assert!(details.contains(value), "inspectable {value} at {w}x{h}");
                }
                let b = buffer(&app, w, h);
                let focus = report_title(&b);
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
        assert_eq!(b[valid].fg, Color::Green); // A validated manifest is a narrow success fact.
        assert!(!b[valid].modifier.contains(Modifier::REVERSED)); // Success fact is not a focused control.
        let details = inspect_all(&mut app, w, h);
        for value in [
            "manifest-end.toml",
            "Defaults:",
            "Platform:",
            "Local:",
            "Extra profiles:",
            "first-tool",
            "second-tool",
            "Unavailable:",
            "tool inventory",
        ] {
            assert!(details.contains(value), "inspectable {value} at {w}x{h}");
        }
        assert!(!details.contains("unrelated-current-session"));
    }
    let b = buffer(&app, 80, 36);
    assert_style(&b, "Outcome:", Color::Reset, Modifier::BOLD);
    assert_style(
        &b,
        "Declarations are valid",
        Color::Green,
        Modifier::empty(),
    );
    assert_style(
        &b,
        "tool inventory and installation",
        Color::Yellow,
        Modifier::BOLD,
    );
    assert!(locate(&b, "Scope:").1 < locate(&b, "Manifest and profiles").1);
    assert!(locate(&b, "Unavailable:").1 < locate(&b, "Format:").1);
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
        locate(&b, "Blocked writes: 2");
        for column in ["OP ID", "TYPE", "STATE", "STATUS"] {
            locate(&b, column);
        }
        let selected = locate(&b, "› bbbbbbbb");
        let text: String = (0..w).map(|x| b[(x, selected.1)].symbol()).collect();
        let byte = text.find("Preparing").unwrap();
        let phase = (text[..byte].chars().count() as u16, selected.1);
        assert!(
            b[phase]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(b[phase].fg, Color::Reset); // Neutral inverse does not recolor phase data.
        assert!(
            b[phase]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        let integrity = locate(&b, "Damaged");
        assert_eq!(b[integrity].fg, Color::Red);
        let focus = report_title(&b);
        assert_eq!(b[focus].fg, Color::Cyan);
        assert_eq!(app.selected, 1);
        if w >= 140 {
            locate(&b, "Operation details");
        }
    }
    app.history_search.editing = true;
    app.history_search.query = format!("q?/{}\u{1b}needle", "é".repeat(50));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| views::draw(f, &app)).unwrap();
        let (x, y) = locate(t.backend().buffer(), "needle");
        assert_eq!(t.get_cursor_position().unwrap(), (x + 6, y).into());
        locate(t.backend().buffer(), "Filter history");
        let screen = render(&app, w, h);
        assert!(!screen.contains('\u{1b}') && !screen.contains('é'));
        assert!(screen.contains("Blocked writes: 2"));
    }
}
#[test]
fn recovery_details_distinguish_sections_fields_and_status() {
    use dort_core::journal::Phase;
    for (phase, problem, status, color, emphasis) in [
        (
            Phase::Completed,
            None,
            "[Finished]",
            Color::Green,
            Modifier::empty(),
        ),
        (
            Phase::Running,
            None,
            "[Blocks writes]",
            Color::Red,
            Modifier::BOLD,
        ),
        (
            Phase::Running,
            Some("corrupt tail"),
            "[Damaged]",
            Color::Red,
            Modifier::BOLD,
        ),
    ] {
        let mut app = App::new(false);
        app.tab = Tab::Recovery;
        app.complete(Ok(Output::History {
            rows: vec![history_row(Some(1), Some(phase), problem, 'a')],
            error: None,
        }));
        key(&mut app, KeyCode::Enter);
        let b = buffer(&app, 80, 40);
        locate(&b, "Operation details");
        assert_style(&b, status, color, emphasis);
        for section in [
            "File adoption",
            "Recovery options",
            "What recovery keeps",
            "Operation record",
        ] {
            assert_style(&b, section, Color::Cyan, Modifier::BOLD);
        }
        locate(&b, &format!("Repository ID: {}", "a".repeat(64)));
        assert_style(&b, "Repository ID:", Color::Reset, Modifier::BOLD);
        assert_style(&b, &"a".repeat(64), Color::Reset, Modifier::empty());
        assert_style(
            &b,
            "Rollback restores files only when they still match.",
            Color::Yellow,
            Modifier::BOLD,
        );
        if phase == Phase::Completed {
            let task_y = locate(&b, "Rollback: Available for review").1;
            assert!(task_y < locate(&b, "Repository ID:").1);
            assert!(task_y < locate(&b, "Saved steps:").1);
            assert_style(&b, "Rollback:", Color::Reset, Modifier::BOLD);
            assert_style(&b, "Available for review", Color::Reset, Modifier::empty());
        }
    }
}
#[test]
fn compact_recovery_shows_option_before_identifiers_and_keeps_it_during_scroll() {
    use dort_core::journal::Phase;
    let mut app = App::new(false);
    app.tab = Tab::Recovery;
    app.complete(Ok(Output::History {
        rows: vec![history_row(Some(3), Some(Phase::Preparing), None, 'a')],
        error: None,
    }));
    let (w, h) = (48, 14);
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), w, h)
            .is_none()
    );
    let first = render(&app, w, h);
    assert!(first.contains("Repository setup") && first.contains("[Blocks writes]"));
    assert!(first.contains("Abandon: Available for review"));
    assert!(!first.contains("Repository ID:"));
    for _ in 0..20 {
        assert!(
            app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), w, h)
                .is_none()
        );
        if render(&app, w, h).contains("Saved steps: 7") {
            break;
        }
    }
    assert!(render(&app, w, h).contains("Saved steps: 7"));
    assert!(
        app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), w, h)
            .is_none()
    );
    assert_eq!(render(&app, w, h), first);
}
#[test]
fn shared_reports_distinguish_sections_fields_and_values() {
    let mut terminal = Terminal::new(TestBackend::new(80, 14)).unwrap();
    terminal
        .draw(|frame| {
            let mut text = ui::review_text(
                "Repository: /fixture/repo\u{1b}\nEffects:\n- No writes.\nBLOCKED: Writer busy",
            );
            text.lines
                .extend(ui::review_text(r#"{"Details":{"Problem":"not recorded"}}"#).lines);
            text.lines
                .extend(ui::review_text("Roll back operation").lines);
            text.lines
                .push(ui::metadata("Operation ID:", "fixture-id\nend"));
            ui::report(frame, frame.area(), "Review", text, 0, true);
        })
        .unwrap();
    let b = terminal.backend().buffer();
    locate(b, "Repository: /fixture/repo\\u{1b}");
    locate(b, "Operation ID: fixture-id\\nend");
    for section in ["Effects:", "Details"] {
        assert_style(b, section, Color::Cyan, Modifier::BOLD);
    }
    for label in [
        "Repository:",
        "Problem",
        "Operation ID:",
        "Roll back operation",
    ] {
        assert_style(b, label, Color::Reset, Modifier::BOLD);
    }
    for value in [
        "/fixture/repo\\u{1b}",
        "not recorded",
        "fixture-id\\nend",
        "No writes.",
    ] {
        assert_style(b, value, Color::Reset, Modifier::empty());
    }
    assert_style(b, "BLOCKED: Writer busy", Color::Red, Modifier::BOLD);
}
#[test]
fn shared_collection_keeps_row_targets_on_visible_lines() {
    use super::input::{Hits, Target, Wheel};
    use ratatui::{layout::Constraint, widgets::Row};
    use ui::collection::{Collection, Header, RowTargets, Selection};

    let mut terminal = Terminal::new(TestBackend::new(32, 5)).unwrap();
    let hits = Hits::default();
    terminal
        .draw(|frame| {
            let area = frame.area();
            hits.begin(area);
            Collection::new([Constraint::Min(10)], Header::Heading(["ITEM"]))
                .titled("Records", true)
                .draw(
                    frame,
                    area,
                    [
                        (17, Row::new(["Alpha"]).height(2).bottom_margin(1)),
                        (32, Row::new(["Beta"]).top_margin(1)),
                    ],
                    Selection::at(Some(1)),
                    RowTargets {
                        hits: &hits,
                        wheel: Some(Wheel::Rows),
                        target: Target::Row,
                    },
                );
        })
        .unwrap();
    let b = terminal.backend().buffer();
    for (label, id, expected_y) in [("Alpha", 17, 2), ("Beta", 32, 3)] {
        let (x, y) = locate(b, label);
        assert_eq!(y, expected_y);
        assert!(matches!(hits.target(x, y), Some(Target::Row(found)) if found == id));
        assert!(matches!(hits.scroll(x, y), Some(Wheel::Rows)));
    }
    assert!(hits.target(3, 4).is_none());
}
#[test]
fn pane_titles_keep_spinner_glyphs_and_escape_controls() {
    let mut terminal = Terminal::new(TestBackend::new(24, 3)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                ui::pane(&format!("{} Reading", ui::glyphs::SPINNER[0]), false),
                frame.area(),
            );
        })
        .unwrap();
    let b = terminal.backend().buffer();
    let title: String = (0..b.area.width).map(|x| b[(x, 0)].symbol()).collect();
    assert!(
        title.contains(&format!("{} Reading", ui::glyphs::SPINNER[0])),
        "{title}"
    );
    terminal
        .draw(|frame| {
            frame.render_widget(ui::pane("a\u{1b}b", false), frame.area());
        })
        .unwrap();
    let escaped: String = (0..24)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    assert!(escaped.contains("a\\u{1b}b"), "{escaped}");
}
#[test]
fn report_wraps_trusted_glyphs_at_terminal_cell_boundaries() {
    let mut terminal = Terminal::new(TestBackend::new(4, 4)).unwrap();
    terminal
        .draw(|frame| {
            ui::text_view(frame, frame.area(), "abc界e\u{301}".into(), 0);
        })
        .unwrap();
    let b = terminal.backend().buffer();
    assert_eq!(b[(3, 0)].symbol(), " ");
    assert_eq!(b[(0, 1)].symbol(), "界");
    assert_eq!(b[(2, 1)].symbol(), "e\u{301}");
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
            for offset in 0..80 {
                app.details_scroll = offset;
                all.push_str(&render(&app, w, h));
                let b = buffer(&app, w, h);
                let (x, y) = locate(&b, "Operation details");
                // The title begins two cells after its pane's left border.
                for col in x - 1..w {
                    cells.push_str(b[(col, y + 1)].symbol());
                }
                // Also join each full visible pane, including the final rows
                // below the clamped scroll start at the end of the document.
                for line in y + 1..h - 1 {
                    for col in x - 1..w {
                        all.push_str(b[(col, line)].symbol());
                    }
                }
            }
            all.push_str(&cells);
            for value in [
                "Repository ID: ",
                "Operation ID: ",
                "Saved steps: 7",
                "Integrity:",
                "Recovery checks current paths",
                "Rollback restores files",
                "Private backups stay.",
            ] {
                assert!(all.contains(value), "full typed detail {value} at {w}x{h}");
            }
            let flat: String = all.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(flat.contains(&"a".repeat(64)));
            assert!(!all.contains('\u{1b}'));
            app.details_scroll = u16::MAX;
            let end = render(&app, w, h);
            assert!(end.contains("Blocked writes:"));
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
    app.git_view.accept(metadata);
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
        let selected = locate(&b, "› new-name");
        let row: String = (0..w).map(|x| b[(x, selected.1)].symbol()).collect();
        let byte = row.find("renamed").unwrap();
        let status = (row[..byte].chars().count() as u16, selected.1);
        assert!(
            b[status]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(b[status].fg, Color::Yellow); // Renamed remains an independent attention fact.
        assert!(
            b[status]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(b[locate(&b, "unmerged")].fg, Color::Red);
        assert_eq!(b[report_title(&b)].fg, Color::Cyan);
        if w >= 140 {
            // Rail plus a useful read-only inspector must both fit.
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
        locate(&b, "Git / Staged");
        locate(&b, "› changed-file");
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
    for (w, h) in [(48, 14), (80, 24), (110, 24), (140, 40)] {
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), w, h);
        app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), w, h);
        let first = render(&app, w, h);
        let mut all = String::new();
        let mut consecutive = String::new();
        for _ in 0..60 {
            let b = buffer(&app, w, h);
            let (x, y) = locate(&b, "Git details");
            for line in y + 1..h - 1 {
                for col in x - 1..w {
                    all.push_str(b[(col, line)].symbol());
                }
            }
            // Git's ordinary metadata scroll advances two rows: capture these
            // consecutive rows to verify paths longer than the whole viewport.
            for line in y + 1..y + 3 {
                for col in x - 1..w {
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
            "Git scope | read-only".into(),
            "Untracked contents".into(),
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
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
#[test]
fn git_details_show_selected_path_and_status_before_scope_and_index_code() {
    use dort_core::git::{Status, StatusEntry};
    let mut app = git_app(super::git::Metadata::Status(Status {
        schema: 1,
        read_only: true,
        branch: vec!["branch.head main".into()],
        entries: vec![StatusEntry {
            kind: "unmerged".into(),
            xy: Some("UU".into()),
            path: "conflicting-file".into(),
            original_path: None,
        }],
    }));
    key(&mut app, KeyCode::Enter);
    let b = buffer(&app, 80, 24);
    assert_style(&b, "Path:", Color::Reset, Modifier::BOLD);
    assert_style(&b, "unmerged", Color::Red, Modifier::BOLD);
    assert_style(&b, "Git scope | read-only", Color::Cyan, Modifier::BOLD);
    assert!(locate(&b, "Path:").1 < locate(&b, "Status:").1);
    assert!(locate(&b, "Status:").1 < locate(&b, "Git-only root:").1);
    assert!(locate(&b, "Status:").1 < locate(&b, "XY:").1);
    assert!(app.preview.is_none());
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
        key(&mut app, KeyCode::F(3)); // Scope facts remain inspectable; Enter chooses a missing scope.
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
        locate(t.backend().buffer(), "Filter changes");
        let screen = render(&app, w, h);
        assert!(!screen.contains('é') && !screen.contains('\u{1b}'));
    }
}
#[test]
fn single_path_enter_requests_a_preview_without_mutation() {
    let mut app = App::new(false);
    app.git = Ok("/trusted/git".into());
    key(&mut app, KeyCode::Char('n'));
    app.shell.init_draft = "/synthetic/new-repository".into();
    assert_eq!(app.shell.path_focused, 0);
    assert!(render(&app, 80, 24).contains("Review read-only preview"));
    assert!(matches!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 80, 24),
        Some(super::actions::Task::Init { .. })
    ));
    assert!(app.preview.is_none() && app.confirmation.is_none());
}
