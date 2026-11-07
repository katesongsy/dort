//! Menus own navigation; contextual operations and approvals remain separate.
use super::{
    actions::Task,
    app::{App, Tab},
    input::Target,
    navigation::{self, Item},
    shell::{Action, Form, Overlay},
    tests::render,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
fn key(app: &mut App, code: KeyCode, width: u16, height: u16) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), width, height)
}
fn click(app: &mut App, width: u16, height: u16, want: impl Fn(Target) -> bool) -> Option<Task> {
    render(app, width, height);
    let (x, y) = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .find(|&(x, y)| app.hits.target(x, y).is_some_and(&want))
        .expect("visible typed menu control");
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        width,
        height,
    )
}
#[test]
fn navigation_hints_are_consolidated_and_numeric_destination_aliases_are_inert() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        let screen = render(&app, w, h);
        let rows: Vec<_> = screen.lines().collect();
        assert!(!rows[..2].join(" ").contains("F10"));
        let footer = rows.last().unwrap();
        for label in ["F10", "Menu", "F1", "Help", ";", "Actions"] {
            assert!(footer.contains(label), "{label} at {w}");
        }
        assert_eq!(screen.matches("F1").count(), 2); // F1 and F10, one each.
        for digit in '1'..='5' {
            assert!(key(&mut app, KeyCode::Char(digit), w, h).is_none());
            assert!(app.tab == Tab::Overview && app.shell.overlay.is_none());
        }
        key(&mut app, KeyCode::F(10), w, h);
        if w < 80 {
            key(&mut app, KeyCode::Enter, w, h);
        }
        let screen = render(&app, w, h);
        assert!(!screen.contains("Arrows move"));
        assert!(!screen.contains("Esc close"));
        assert!(!screen.contains("F1 Help"));
        assert!(!screen.contains("[1]"));
    }
}
#[test]
fn review_report_has_hierarchy_and_preserves_escaped_metadata() {
    use super::components as ui;
    let text = ui::review_text(
        "Review\nEffects:\n- Create link\nPath: odd\\u{1b}name\nBLOCKED: unsafe parent",
    );
    assert_eq!(text.lines[0].style, ui::heading());
    assert_eq!(text.lines[1].style, ui::heading());
    assert_eq!(text.lines[2].spans[0].style, ui::accent());
    assert_eq!(text.lines[3].spans[0].style, ui::heading());
    assert!(text.lines[3].to_string().contains("\\u{1b}"));
    assert_eq!(text.lines[4].style, ui::danger());
}
#[test]
fn menus_are_keyboard_and_mouse_accessible_at_every_supported_size() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.tab = Tab::Files;
        let screen = render(&app, w, h);
        assert!(screen.contains("F10") && screen.contains("Files / Managed"));
        key(&mut app, KeyCode::F(10), w, h);
        assert!(app.navigation.menu.is_some());
        if w < 80 {
            key(&mut app, KeyCode::Enter, w, h);
        }
        let screen = render(&app, w, h);
        for label in ["Managed", "Discover [f]", "Ignored paths [U]"] {
            assert!(screen.contains(label), "{label} at {w}");
        }
        key(&mut app, KeyCode::Down, w, h);
        let before = app.navigation.menu.unwrap();
        key(&mut app, KeyCode::F(1), w, h);
        assert_eq!(app.help.as_ref().unwrap().topic.title(), "Navigation menus");
        key(&mut app, KeyCode::Enter, w, h);
        assert!(app.help.is_some() && app.tab == Tab::Files);
        key(&mut app, KeyCode::Esc, w, h);
        let after = app.navigation.menu.unwrap();
        assert_eq!(
            (before.group, before.selected, before.expanded),
            (after.group, after.selected, after.expanded)
        );
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_none());
        click(&mut app, w, h, |t| {
            matches!(t, Target::MenuOpen | Target::MenuGroup(2))
        });
        if w < 80 {
            click(&mut app, w, h, |t| matches!(t, Target::MenuGroup(2)));
        }
        let task = click(&mut app, w, h, |t| matches!(t, Target::MenuItem(2)));
        assert!(matches!(task, Some(Task::IgnoredPaths)));
        assert!(app.navigation.menu.is_none());
    }
}
#[test]
fn every_menu_group_fits_at_floor_and_navigating_has_no_write_approval() {
    for group in 0..navigation::GROUPS.len() {
        let mut app = App::new(false);
        app.open_menu(group, 48);
        app.navigation.menu.as_mut().unwrap().expanded = true;
        let screen = render(&app, 48, 14);
        for entry in navigation::entries(group) {
            assert!(screen.contains(entry.label), "{}", entry.label);
        }
        assert!(app.preview.is_none() && app.confirmation.is_none());
    }
    let mut app = App::new(false);
    assert!(matches!(
        app.menu_item(Item::Action(Action::Discover)),
        Some(Task::Discover)
    ));
    assert!(app.tab == Tab::Files && app.discovering && app.preview.is_none());
    app.menu_item(Item::Action(Action::Destination(Tab::Files)));
    assert!(!app.discovering);
    app.menu_item(Item::Manifest);
    assert!(app.tab == Tab::Checks && !app.showing_diagnostics);
    assert!(matches!(
        app.menu_item(Item::Action(Action::Diagnostics)),
        Some(Task::Diagnostics)
    ));
    assert!(app.showing_diagnostics);
}
#[test]
fn action_bars_and_search_exclude_navigation_in_every_view() {
    for tab in [
        Tab::Overview,
        Tab::Files,
        Tab::Recovery,
        Tab::Checks,
        Tab::Git,
    ] {
        for discovering in [false, true] {
            let mut app = App::new(false);
            app.tab = tab;
            app.discovering = discovering;
            for entry in super::context::entries(&app) {
                assert!(!navigation::owns(entry.action));
            }
            for action in app.page_actions() {
                assert!(!navigation::owns(action));
            }
        }
    }
    let mut app = App::new(false);
    app.tab = Tab::Checks;
    app.showing_diagnostics = true;
    assert!(app.page_actions() == vec![Action::Diagnostics]); // refresh of this context, not a transition
}
#[test]
fn menu_cancel_retains_filter_focus_selection_and_per_destination_scroll() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.selected = 7;
    app.details_scroll = 11;
    app.inventory.managed.query = "literal q?/".into();
    app.inventory.managed.editing = true;
    key(&mut app, KeyCode::F(10), 80, 24);
    key(&mut app, KeyCode::Esc, 80, 24);
    assert_eq!(app.selected, 7);
    assert_eq!(app.inventory.managed.query, "literal q?/");
    assert!(app.inventory.managed.editing);
    app.menu_item(Item::Action(Action::Destination(Tab::Recovery)));
    app.selected = 3;
    app.details_scroll = 5;
    app.menu_item(Item::Action(Action::Destination(Tab::Files)));
    assert_eq!((app.selected, app.details_scroll), (7, 11));
    app.menu_item(Item::Action(Action::Destination(Tab::Recovery)));
    assert_eq!((app.selected, app.details_scroll), (3, 5));
}
#[test]
fn menu_focus_owns_help_quit_caret_and_outside_clicks_without_editing_search() {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.tab = Tab::Files;
        app.inventory.managed.query = "retained".into();
        app.inventory.managed.editing = true;
        key(&mut app, KeyCode::F(10), w, h);
        if w < 80 {
            key(&mut app, KeyCode::Enter, w, h);
        }
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| super::views::draw(f, &app)).unwrap();
        let caret = terminal.get_cursor_position().unwrap();
        let cell = &terminal.backend().buffer()[(caret.x, caret.y)];
        assert_eq!(cell.symbol(), ">");
        assert_eq!(cell.bg, Color::Cyan);
        assert!(cell.modifier.contains(Modifier::REVERSED | Modifier::BOLD));
        key(&mut app, KeyCode::Char('?'), w, h);
        assert_eq!(app.help.as_ref().unwrap().topic.title(), "Navigation menus");
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_some());
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_none() && app.inventory.managed.editing);
        assert_eq!(app.inventory.managed.query, "retained");
        key(&mut app, KeyCode::Char('?'), w, h);
        assert_eq!(app.inventory.managed.query, "retained?");
        key(&mut app, KeyCode::F(10), w, h);
        key(&mut app, KeyCode::Char('q'), w, h);
        assert!(app.quit);
    }
    let mut app = App::new(false);
    app.tab = Tab::Files;
    app.complete(Ok(super::actions::Output::Data(super::actions::Data {
        repository: Some("/synthetic/menu-repository".into()),
        ..Default::default()
    })));
    render(&app, 80, 24);
    let (x, y) = (0..24)
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x, y), Some(Target::Action(Action::Apply))))
        .unwrap();
    app.open_menu(2, 80);
    render(&app, 80, 24);
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
    assert!(app.navigation.menu.is_none() && app.preview.is_none() && app.confirmation.is_none());
}
#[test]
fn forms_tasks_errors_and_git_scope_input_cannot_be_bypassed_by_menus() {
    for form in [
        Form::Init,
        Form::Choose,
        Form::Adopt,
        Form::KeepId,
        Form::RemoveId,
        Form::Check,
    ] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(form));
        key(&mut app, KeyCode::F(10), 80, 24);
        assert!(app.navigation.menu.is_none() && app.shell.overlay.is_some());
    }
    let mut app = App::new(false);
    app.start(false);
    key(&mut app, KeyCode::F(10), 80, 24);
    assert!(app.navigation.menu.is_none());
    key(&mut app, KeyCode::Char('q'), 80, 24);
    assert!(app.exit_requested && !app.quit);
    let mut app = App::new(false);
    app.error = Some("metadata failure".into());
    key(&mut app, KeyCode::F(10), 80, 24);
    assert!(app.navigation.menu.is_none());
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = "retained Git scope".into();
    key(&mut app, KeyCode::F(10), 80, 24);
    assert!(app.navigation.menu.is_none());
    assert_eq!(app.git_view.draft, "retained Git scope");
    app.git_view.editing_root = false;
    key(&mut app, KeyCode::F(10), 80, 24);
    assert!(app.navigation.menu.is_some());
    app.resize(47, 13);
    assert!(app.navigation.menu.is_none());
}
