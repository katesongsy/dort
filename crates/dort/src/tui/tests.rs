use super::{
    actions::{Completion, Data, Output},
    app::{App, Tab},
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

// Invoked only by an isolated PTY child for terminal cleanup checks; never a runtime switch.
#[test]
fn terminal_cleanup_probe() {
    let Ok(mode) = std::env::var("DORT_TEST_TERMINAL_CLEANUP") else {
        return;
    };
    let result = super::terminal_session(|terminal, _| {
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new("cleanup probe"),
                    frame.area(),
                )
            })
            .unwrap();
        match mode.as_str() {
            "panic" => panic!("synthetic terminal panic"),
            "error" => Err(dort_core::Error::Invalid(
                "synthetic terminal I/O failure".into(),
            )),
            _ => Ok(()),
        }
    });
    if mode == "error" {
        assert!(result.is_err());
    } else {
        result.unwrap();
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
pub(super) fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| views::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn compact_forms_and_profile_picker_always_show_focused_control() {
    use super::shell::{Form, Overlay};
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    let prefixes = [
        "TARGET",
        "ID",
        "SOURCE",
        "Review",
        "Choose",
        "Create/attach",
        "Discard",
    ];
    for (i, prefix) in prefixes.iter().enumerate() {
        app.shell.adoption.focused = i;
        let screen = render(&app, 48, 14);
        assert!(
            screen.contains(&format!("> {prefix}")),
            "focused control {i} not visible: {screen}"
        );
    }
    app.shell.adoption.target = "é".repeat(2048);
    app.shell.adoption.focused = 0;
    app.key(key(KeyCode::Char('é')), 80, 24);
    assert_eq!(app.shell.adoption.target.len(), 4096);
    let screen = render(&app, 48, 14);
    assert!(screen.contains("..."));
    app.shell.overlay = Some(Overlay::Form(Form::Check));
    for (i, prefix) in [
        "Manifest path",
        "Check manifest",
        "Temporary profiles",
        "Discard",
    ]
    .iter()
    .enumerate()
    {
        app.shell.manifest_focused = i;
        assert!(render(&app, 48, 14).contains(&format!("> {prefix}")));
    }
    app.shell.overlay = Some(Overlay::Form(Form::RemoveId));
    app.shell.unmanage_focused = 1;
    assert!(render(&app, 48, 14).contains("> Review"));
    app.shell.overlay = None;
    app.tab = Tab::Overview;
    app.key(key(KeyCode::Left), 80, 24);
    assert!(app.tab == Tab::Git);
    app.key(key(KeyCode::Right), 80, 24);
    assert!(app.tab == Tab::Overview);
}

#[test]
fn small_layout_snapshot_disables_approval() {
    let mut app = App::new(false);
    assert_eq!(
        render(&app, 40, 6),
        "Dort: terminal too small\nResize to 48x14; approval disabled.\nq/Ctrl-C: exit (waits for worker)\n\n\n"
    );
    assert!(app.key(key(KeyCode::Enter), 40, 6).is_none());
    assert!(!app.busy);
    app.key(key(KeyCode::Char('q')), 40, 6);
    assert!(app.quit);
}
#[test]
fn compact_navigation_and_report_home_are_readable_without_color() {
    let mut app = App::new(true);
    for (width, height) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, width, height);
        for label in [
            "1 Overview",
            "2 Files",
            "3 Recovery",
            "4 Checks",
            "5 Git",
            "Actions",
            "DRY RUN",
            "Content",
        ] {
            assert!(
                screen.contains(label),
                "missing textual {label} at {width}x{height}"
            );
        }
    }
    app.key(key(KeyCode::Char('?')), 48, 14);
    for _ in 0..100 {
        app.key(key(KeyCode::PageDown), 48, 14);
    }
    app.key(key(KeyCode::Home), 48, 14);
    assert_eq!(app.shell.scroll, 0);
    assert!(render(&app, 48, 14).contains("Space: Actions | / there: search by label"));
    app.key(key(KeyCode::Esc), 48, 14);
    app.complete(Err("long report\n".repeat(100)));
    app.key(key(KeyCode::PageDown), 48, 14);
    app.key(key(KeyCode::Home), 48, 14);
    assert_eq!(app.details_scroll, 0);
    app.error = None;
    app.start(true);
    app.key(key(KeyCode::Char('?')), 48, 14);
    app.key(key(KeyCode::PageDown), 48, 14);
    app.key(key(KeyCode::Home), 48, 14);
    assert_eq!(app.shell.scroll, 0);
    assert!(app.busy && !app.quit);
}

#[test]
fn long_lists_keep_selection_visible_through_resize_and_form_back() {
    use super::shell::Overlay;
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        files: (0..128)
            .map(|i| (format!("file-{i:03}"), format!("Full metadata {i}")))
            .collect(),
        known_roots: (0..200)
            .map(|i| format!("/repository-{i:03}").into())
            .collect(),
        ..Default::default()
    })));
    app.tab = Tab::Files;
    for _ in 0..120 {
        app.key(key(KeyCode::Down), 80, 24);
    }
    for (width, height) in [(48, 14), (80, 24), (140, 40), (48, 14)] {
        app.resize(width, height);
        assert!(render(&app, width, height).contains("> file-120"));
        assert_eq!(app.selected, 120);
    }
    app.key(key(KeyCode::Char('p')), 48, 14);
    for _ in 0..201 {
        app.key(key(KeyCode::Down), 48, 14);
    }
    assert!(render(&app, 48, 14).contains("> \"/repository-199\""));
    app.key(key(KeyCode::Esc), 48, 14);
    assert_eq!(app.selected, 120);
    app.key(key(KeyCode::Char('o')), 48, 14);
    app.shell.adoption.target = "q?/".repeat(100);
    app.key(key(KeyCode::PageDown), 48, 14);
    app.key(key(KeyCode::Tab), 48, 14);
    assert_eq!(app.shell.scroll, 0);
    assert!(render(&app, 48, 14).contains("> ID"));
    app.resize(40, 10);
    app.resize(48, 14);
    assert_eq!(app.shell.adoption.target, "q?/".repeat(100));
    assert!(matches!(app.shell.overlay, Some(Overlay::Form(_))));
    app.key(key(KeyCode::Esc), 48, 14);
    assert!(render(&app, 48, 14).contains("> file-120"));
}

#[test]
fn quit_during_work_waits_for_completion() {
    let mut app = App::new(false);
    app.start(true);
    app.key(key(KeyCode::Char('q')), 80, 24);
    assert!(app.exit_requested);
    assert!(!app.quit);
    assert!(app.key(key(KeyCode::Char('a')), 80, 24).is_none());
    assert!(
        app.complete(Ok(Output::Completed(Completion {
            operation: "test",
            transaction_id: None,
            repository: None
        })))
        .is_none()
    );
    assert!(app.quit);
}
#[test]
fn navigation_and_metadata_escaping() {
    let mut app = App::new(true);
    app.complete(Ok(Output::Data(Data {
        overview: "no repository\n\u{1b}[31m\u{202e}".into(),
        files_error: None,
        repository: None,
        known_roots: vec![],
        profiles: vec![],
        files: vec![],
        history: vec![],
        history_error: None,
    })));
    let screen = render(&app, 80, 24);
    assert!(screen.contains("DRY RUN"));
    assert!(screen.contains("\\u{1b}[31m\\u{202e}"));
    app.key(key(KeyCode::Right), 80, 24);
    assert!(app.tab == Tab::Files);
    app.key(key(KeyCode::Down), 80, 24);
    assert_eq!(app.selected, 0);
    assert!(render(&app, 48, 14).contains("0 active mapping(s)"));
}

#[test]
fn shell_focus_help_and_search_restore_destination_without_intercepting_text() {
    use super::shell::{Focus, Overlay};
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.key(key(KeyCode::Tab), 80, 24);
    assert!(app.shell.focus == Focus::Actions);
    app.key(key(KeyCode::Enter), 80, 24);
    assert!(render(&app, 80, 24).contains("Choose repository (session only)"));
    app.key(key(KeyCode::Esc), 80, 24);
    app.details_scroll = 3;
    app.key(key(KeyCode::Char('?')), 80, 24);
    assert!(render(&app, 100, 40).contains("| Help"));
    app.key(key(KeyCode::PageDown), 80, 24);
    assert_eq!(app.shell.scroll, 4);
    app.key(key(KeyCode::Esc), 80, 24);
    assert_eq!(app.details_scroll, 3);
    assert!(app.tab == Tab::Files && app.shell.focus == Focus::Actions);
    app.key(key(KeyCode::BackTab), 80, 24);
    assert!(app.shell.focus == Focus::Content);
    app.key(key(KeyCode::Char(' ')), 80, 24);
    app.key(key(KeyCode::Char('/')), 80, 24);
    for c in "q?/".chars() {
        app.key(key(KeyCode::Char(c)), 80, 24);
    }
    assert!(!app.quit);
    assert!(matches!(&app.shell.overlay, Some(Overlay::Actions { query, .. }) if query == "q?/"));
    app.key(key(KeyCode::Esc), 80, 24); // leave search, retain query
    app.key(key(KeyCode::Delete), 80, 24);
    assert!(matches!(&app.shell.overlay, Some(Overlay::Actions { query, .. }) if query.is_empty()));
    app.key(key(KeyCode::Esc), 80, 24);
    app.key(key(KeyCode::Char('n')), 80, 24);
    for c in "q?/".chars() {
        app.key(key(KeyCode::Char(c)), 80, 24);
    }
    assert!(!app.quit);
    assert_eq!(app.shell.init_draft, "q?/");
    app.resize(40, 10);
    app.resize(80, 24);
    assert_eq!(app.shell.init_draft, "q?/");
    app.key(key(KeyCode::Esc), 80, 24);
    assert!(app.tab == Tab::Files);
    app.key(key(KeyCode::Char('n')), 80, 24);
    app.key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        80,
        24,
    );
    assert!(app.shell.init_draft.is_empty());
}

#[test]
fn busy_shell_cannot_change_scope_or_toggle_mode_or_start_another_job() {
    let mut app = App::new(false);
    app.start(true);
    for code in [
        KeyCode::Char('n'),
        KeyCode::Char('p'),
        KeyCode::Char('/'),
        KeyCode::Enter,
        KeyCode::Char('a'),
        KeyCode::Char('t'),
        KeyCode::Char('c'),
        KeyCode::Char('v'),
        KeyCode::Char('f'),
        KeyCode::Char('o'),
    ] {
        assert!(app.key(key(code), 80, 24).is_none());
    }
    assert!(app.shell.overlay.is_none() && app.repository.is_none());
    app.key(key(KeyCode::Char('?')), 80, 24);
    assert!(render(&app, 80, 40).contains("| Help"));
    app.key(key(KeyCode::Esc), 80, 24);
    assert!(app.shell.overlay.is_none());
    assert!(!app.dry_run);
}

#[test]
fn refresh_preserves_selected_identity_and_clamps_if_it_disappears() {
    let data = |ids: &[&str]| Data {
        overview: String::new(),
        files_error: None,
        repository: None,
        known_roots: vec![],
        profiles: vec![],
        files: ids
            .iter()
            .map(|id| (id.to_string(), String::new()))
            .collect(),
        history: vec![],
        history_error: None,
    };
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.complete(Ok(Output::Data(data(&["a", "b", "c"]))));
    app.selected = 1;
    app.complete(Ok(Output::Data(data(&["c", "a", "b"]))));
    assert_eq!(app.selected, 2);
    app.complete(Ok(Output::Data(data(&["a"]))));
    assert_eq!(app.selected, 0);
    assert!(app.status.contains("disappeared"));
}

#[test]
fn completion_keeps_noop_result_visible_through_refresh_and_read_failure() {
    let mut app = App::new(false);
    let refresh = app.complete(Ok(Output::Completed(Completion {
        operation: "File operation",
        transaction_id: None,
        repository: None,
    })));
    assert!(matches!(refresh, Some(super::actions::Task::Refresh)));
    assert!(
        app.result
            .as_ref()
            .unwrap()
            .contains("no transaction created")
    );
    app.start(false);
    app.complete(Ok(Output::Data(Data {
        overview: "fresh observations".into(),
        files_error: None,
        repository: None,
        known_roots: vec![],
        profiles: vec![],
        files: vec![],
        history: vec![],
        history_error: None,
    })));
    assert!(render(&app, 80, 24).contains("no transaction created"));
    app.start(false);
    app.complete(Err("later read failure".into()));
    assert!(
        app.result
            .as_ref()
            .unwrap()
            .contains("no transaction created")
    );
    assert!(!app.error.as_ref().unwrap().contains("partial effects"));
}
