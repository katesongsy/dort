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

pub(super) fn target_position(
    app: &App,
    width: u16,
    height: u16,
    target: impl Fn(super::input::Target) -> bool,
) -> (u16, u16) {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .find(|(x, y)| app.hits.target(*x, *y).is_some_and(&target))
        .expect("Actual foreground must expose the typed hit target")
}
pub(super) fn has_action(app: &App, label: &str) -> bool {
    render(app, 80, 24);
    super::workbench::actions(app)
        .iter()
        .any(|a| a.label == label)
}
pub(super) fn more_target(
    app: &mut App,
    target: impl Fn(super::input::Target) -> bool,
) -> Option<super::actions::Task> {
    render(app, 80, 24);
    if super::workbench::strip_actions(app, 80).1.is_empty() {
        use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let (column, row) = target_position(app, 80, 24, &target);
        return app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
            80,
            24,
        );
    }
    assert!(app.key(key(KeyCode::F(2)), 80, 24).is_none());
    let menu = app.more.as_ref().expect("More is a real foreground");
    assert!(menu.items.iter().all(|item| !matches!(
        item.action.target,
        super::input::Target::ConfirmAction
            | super::input::Target::Continue
            | super::input::Target::ToolInstallControl(_, true)
    )));
    let index = menu
        .items
        .iter()
        .position(|item| target(item.action.target) && item.reason.is_none())
        .expect("Enabled contextual task must be discoverable in More");
    for _ in 0..index {
        assert!(app.key(key(KeyCode::Down), 80, 24).is_none());
    }
    app.key(key(KeyCode::Enter), 80, 24)
}
pub(super) fn assert_text_focus(
    terminal: &Terminal<TestBackend>,
    size: (u16, u16),
    cursor: ratatui::layout::Position,
) {
    use ratatui::style::Modifier;
    assert!(
        cursor.x < size.0 && cursor.y > 0 && cursor.y < size.1 - 1,
        "{size:?}: {cursor:?}"
    );
    let buffer = terminal.backend().buffer();
    assert!(
        (0..size.0).any(|x| {
            let cell = &buffer[(x, cursor.y)];
            cell.symbol() == "[" && cell.modifier.contains(Modifier::BOLD)
        }),
        "Focused input boundary missing at {cursor:?}"
    );
    assert!(
        (0..size.0).any(|x| {
            let cell = &buffer[(x, cursor.y - 1)];
            cell.symbol() != " " && cell.modifier.contains(Modifier::BOLD)
        }),
        "Focused field label missing"
    );
}
pub(super) fn assert_primary_action(terminal: &Terminal<TestBackend>) {
    use ratatui::style::{Color, Modifier};
    let buffer = terminal.backend().buffer();
    let footer = &buffer.content[buffer.content.len() - usize::from(buffer.area.width)..];
    let primary = footer
        .windows(5)
        .find(|cells| {
            cells
                .iter()
                .map(|c| c.symbol())
                .eq(["E", "n", "t", "e", "r"])
        })
        .expect("Missing Enter in footer");
    assert!(primary.iter().all(|c| c.fg != Color::Reset
        && c.bg != Color::Reset
        && c.fg != c.bg
        && c.modifier.contains(Modifier::BOLD | Modifier::REVERSED)));
}
pub(super) fn approve_ready(
    app: &mut App,
    width: u16,
    height: u16,
) -> Option<super::actions::Task> {
    if render(app, width, height)
        .lines()
        .last()
        .unwrap()
        .contains("End Read")
    {
        assert!(app.key(key(KeyCode::End), width, height).is_none());
        render(app, width, height);
    }
    app.key(key(KeyCode::Char('y')), width, height)
}
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
pub(super) fn destination(app: &mut App, tab: super::app::Tab, width: u16, height: u16) {
    app.activate(super::shell::Action::Destination(tab));
    app.key(
        KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
        width,
        height,
    );
}
pub(super) fn focus_form(app: &mut App) {
    if app.form_page().is_some() && !app.form_has_focus() {
        assert!(
            app.key(
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::Tab,
                    ratatui::crossterm::event::KeyModifiers::NONE
                ),
                80,
                24
            )
            .is_none()
        );
    }
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
pub(super) fn click_label(
    app: &mut App,
    width: u16,
    height: u16,
    label: &str,
) -> Option<super::actions::Task> {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let screen = render(app, width, height);
    let (column, row) = screen
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find(label)
                .map(|x| (line[..x].chars().count() as u16, y as u16))
        })
        .unwrap_or_else(|| panic!("{label:?} not visible in:\n{screen}"));
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        width,
        height,
    )
}
#[test]
fn compact_forms_and_profile_picker_always_show_focused_control() {
    use super::shell::{Form, Overlay};
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    super::tests::focus_form(&mut app);
    let prefixes = ["TARGET", "ID", "SOURCE"]; // Only fields participate in traversal.
    for (i, prefix) in prefixes.iter().enumerate() {
        app.shell.adoption.focused = i;
        let screen = render(&app, 48, 14);
        assert!(
            screen.contains(prefix) && screen.contains("Review") && screen.contains("F1 Help"),
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
    super::tests::focus_form(&mut app);
    for _ in 0..8 {
        app.key(key(KeyCode::Tab), 48, 14);
        assert_eq!(app.shell.manifest_focused, 0);
        let text = render(&app, 48, 14);
        assert!(
            text.contains("Manifest path")
                && text.contains("Standalone manifest")
                && text.contains("Enter Check")
                && text.contains("F1 Help")
        );
    }
    app.shell.overlay = Some(Overlay::Form(Form::RemoveId));
    super::tests::focus_form(&mut app);
    app.shell.unmanage_focused = 0;
    assert!(render(&app, 48, 14).contains("Exact ID"));
    assert!(render(&app, 48, 14).contains("Review"));
    app.shell.overlay = None;
    app.tab = Tab::Overview;
    app.key(key(KeyCode::Left), 80, 24);
    assert!(app.tab == Tab::Overview);
    app.key(key(KeyCode::Right), 80, 24);
    assert!(app.tab == Tab::Overview);
}

#[test]
fn small_layout_snapshot_disables_approval() {
    let mut app = App::new(false);
    assert_eq!(
        render(&app, 40, 6),
        "Dort: terminal too small\nResize to 48x14; approval disabled.\nq: exit (waits for worker)\n\n\n"
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
        for label in ["Overview", "More", "Help", "DRY RUN"] {
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
    assert!(render(&app, 48, 14).contains("Overview") && app.help.is_some());
    assert_eq!(app.help.as_ref().unwrap().scroll, 0);
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
        assert!(render(&app, width, height).contains("› [ ] file-120"));
        assert_eq!(app.selected, 120);
    }
    app.key(key(KeyCode::Char('p')), 48, 14);
    for _ in 0..201 {
        app.key(key(KeyCode::Down), 48, 14);
    }
    assert!(render(&app, 48, 14).contains("› /repository-199"));
    app.key(key(KeyCode::Esc), 48, 14);
    assert_eq!(app.selected, 120);
    app.key(key(KeyCode::Char('m')), 48, 14);
    app.shell.adoption.target = "q?/".repeat(100);
    app.key(key(KeyCode::PageDown), 48, 14);
    app.key(key(KeyCode::Tab), 48, 14);
    assert_eq!(app.shell.scroll, 0);
    let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
    terminal.draw(|frame| views::draw(frame, &app)).unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    assert_text_focus(&terminal, (48, 14), cursor);
    assert!(render(&app, 48, 14).contains("ID"));
    app.resize(40, 10);
    app.resize(48, 14);
    assert_eq!(app.shell.adoption.target, "q?/".repeat(100));
    assert!(matches!(app.shell.overlay, Some(Overlay::Form(_))));
    app.key(key(KeyCode::Esc), 48, 14); // stop editing
    app.key(key(KeyCode::Esc), 48, 14); // return to list
    assert!(render(&app, 48, 14).contains("› [ ] file-120"));
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
        tools: None,
        tools_source: None,
        overview: None,
        managed: vec![],
        files_error: Some("no repository\n\u{1b}[31m\u{202e}".into()),
        repository: None,
        known_roots: vec![],
        profiles: vec![],
        files: vec![],
        history: vec![],
        history_error: None,
    })));
    app.key(key(KeyCode::F(3)), 80, 24);
    let mut screen = String::new();
    for _ in 0..30 {
        screen.push_str(&render(&app, 80, 24));
        app.key(key(KeyCode::PageDown), 80, 24);
    }
    assert!(screen.contains("DRY RUN"));
    assert!(screen.contains("\\u{1b}[31m\\u{202e}"));
    app.data.as_mut().unwrap().files_error = None;
    destination(&mut app, Tab::Files, 80, 24);
    assert!(app.tab == Tab::Files);
    app.key(key(KeyCode::Down), 80, 24);
    assert_eq!(app.selected, 0);
    assert!(render(&app, 48, 14).contains("No active files."));
}

#[test]
fn shell_focus_help_and_search_restore_destination_without_intercepting_text() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.key(key(KeyCode::Tab), 80, 24);
    assert!(app.preview.is_none() && app.confirmation.is_none());
    app.details_scroll = 3;
    app.key(key(KeyCode::Char('?')), 80, 24);
    assert!(render(&app, 100, 40).contains("Help") && app.dialog_title() == Some("Help"));
    app.key(key(KeyCode::PageDown), 80, 24);
    assert_eq!(app.help.as_ref().unwrap().scroll, 2);
    app.key(key(KeyCode::Esc), 80, 24);
    assert_eq!(app.details_scroll, 3);
    assert!(app.tab == Tab::Files);
    app.key(key(KeyCode::BackTab), 80, 24);
    assert!(app.preview.is_none() && app.confirmation.is_none());
    app.key(key(KeyCode::Char('n')), 80, 24);
    focus_form(&mut app);
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
    assert!(render(&app, 80, 40).contains("Help") && app.help.is_some());
    app.key(key(KeyCode::Esc), 80, 24);
    assert!(app.shell.overlay.is_none());
    assert!(!app.dry_run);
}

#[test]
fn refresh_preserves_selected_identity_and_clamps_if_it_disappears() {
    let data = |ids: &[&str]| Data {
        tools: None,
        tools_source: None,
        overview: None,
        managed: vec![],
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
            .contains("Everything is up to date")
    );
    app.start(false);
    app.complete(Ok(Output::Data(Data {
        tools: None,
        tools_source: None,
        overview: None,
        managed: vec![],
        files_error: None,
        repository: None,
        known_roots: vec![],
        profiles: vec![],
        files: vec![],
        history: vec![],
        history_error: None,
    })));
    assert!(render(&app, 80, 24).contains("Everything is up to date"));
    app.start(false);
    app.complete(Err("later read failure".into()));
    assert!(
        app.result
            .as_ref()
            .unwrap()
            .contains("Everything is up to date")
    );
    assert!(!app.error.as_ref().unwrap().contains("partial effects"));
}
