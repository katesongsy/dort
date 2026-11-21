//! Menus own navigation; contextual operations and approvals remain separate.
#[path = "places_traversal.rs"]
mod traversal;
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
#[test]
fn ctrl_g_opens_and_closes_navigation_without_losing_a_draft() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(Form::Adopt));
        super::tests::focus_form(&mut app);
        app.shell.adoption.target = "retained draft".into();
        render(&app, w, h);
        let shortcut = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(app.key(shortcut, w, h).is_none());
        assert!(app.navigation.menu.is_some());
        assert_eq!(app.shell.adoption.target, "retained draft");
        assert!(app.key(shortcut, w, h).is_none());
        assert!(app.navigation.menu.is_none());
        assert_eq!(app.shell.adoption.target, "retained draft");
        assert!(matches!(
            app.shell.overlay,
            Some(Overlay::Form(Form::Adopt))
        ));
    }
    let mut app = App::new(false);
    app.start(false);
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            80,
            24
        )
        .is_none()
    );
    assert!(app.navigation.menu.is_none() && app.interaction_notice.is_some());
    let mut app = App::new(false);
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            47,
            13
        )
        .is_none()
    );
    assert!(app.navigation.menu.is_none());
}

#[test]
fn f10_neither_opens_nor_closes_places() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        render(&app, w, h);
        key(&mut app, KeyCode::F(10), w, h);
        assert!(app.navigation.menu.is_none() && app.tab == Tab::Overview);
        super::tests::click_places(&mut app, w, h);
        assert!(app.navigation.menu.is_some());
        key(&mut app, KeyCode::F(10), w, h);
        assert!(app.navigation.menu.is_some());
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_none());
    }
}
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
        assert!(rows[0].contains("Menu"));
        for label in ["F2", "More", "F1", "Help"] {
            assert!(footer.contains(label), "{label} at {w}");
        }
        assert_eq!(screen.matches("F1").count(), 1);
        assert_eq!(screen.matches("F2").count(), 1);
        for digit in '1'..='5' {
            assert!(key(&mut app, KeyCode::Char(digit), w, h).is_none());
            assert!(app.tab == Tab::Overview && app.shell.overlay.is_none());
        }
        super::tests::click_places(&mut app, w, h);
        let screen = render(&app, w, h);
        assert!(screen.contains("Left/Right destination"));
        assert!(!screen.contains("Esc close"));
        assert!(screen.lines().last().unwrap().contains("Help"));
        assert!(!screen.lines().last().unwrap().contains("More"));
        assert!(!screen.contains("[1]"));
    }
}
#[test]
fn tools_navigation_has_only_places_and_forms_keep_their_parent_drafts() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.repository = Some("/synthetic/repo".into());
        app.tab = Tab::Tools;
        let screen = render(&app, w, h);
        assert!(!screen.contains("3 Check") && !screen.contains("Snapshots"));
        assert_eq!(super::navigation::local_catalog(6).len(), 3);
        for label in ["1 Declared", "2 Installed", "3 History"] {
            assert!(screen.contains(label), "{screen}");
        }
        let entries = super::navigation::entries(6);
        assert_eq!(
            entries.iter().map(|e| e.label).collect::<Vec<_>>(),
            [
                "Declared tools [1]",
                "Installed tools [2]",
                "Installation history [3]"
            ]
        );
        app.open_menu(6, w);
        key(&mut app, KeyCode::Right, w, h);
        key(&mut app, KeyCode::Right, w, h);
        let task = click(&mut app, w, h, |t| matches!(t, Target::MenuItem(2)));
        assert!(matches!(task, Some(Task::ToolAttempts)));
        assert!(app.tools_view.install_open && app.navigation.menu.is_none());
        app.tools_view.declare();
        render(&app, w, h);
        assert!((0..h).all(|y| {
            (0..w).all(|x| !matches!(app.hits.target(x, y), Some(Target::LocalDestination(_, _))))
        }));
        app.shell.overlay = Some(Overlay::Form(Form::Adopt));
        super::tests::focus_form(&mut app);
        app.shell.adoption.target = "retained local draft".into();
        app.open_menu(2, w);
        key(&mut app, KeyCode::Right, w, h);
        assert!(app.navigation.menu.is_some());
        assert_eq!(app.shell.adoption.target, "retained local draft");
        key(&mut app, KeyCode::Esc, w, h);
        assert_eq!(app.shell.adoption.target, "retained local draft");
    }
}

#[test]
fn plain_section_shortcuts_are_visible_and_respect_dialog_busy_and_small_guards() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for (letter, tab) in [
            ('o', Tab::Overview),
            ('f', Tab::Files),
            ('t', Tab::Tools),
            ('r', Tab::Recovery),
            ('c', Tab::Checks),
            ('g', Tab::Git),
        ] {
            let mut app = App::new(false);
            app.shell.adoption.target = "retained 123".into();
            app.key(
                KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE),
                w,
                h,
            );
            assert!(app.tab == tab && app.shell.overlay.is_none());
            assert_eq!(app.shell.adoption.target, "retained 123");
            assert!(app.preview.is_none() && app.confirmation.is_none());
        }
        let mut app = App::new(false);
        app.open_menu(0, w);
        let screen = render(&app, w, h);
        for label in navigation::SECTION_HINTS {
            assert!(screen.contains(label), "{label}: {screen}");
        }
        key(&mut app, KeyCode::F(1), w, h);
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), w, h);
        assert!(app.tab == Tab::Overview && app.help.is_some() && app.interaction_notice.is_some());
        let mut app = App::new(false);
        app.start(false);
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), w, h);
        assert!(app.tab == Tab::Overview && app.busy && app.interaction_notice.is_some());
        let mut app = App::new(false);
        app.open_menu(0, w);
        assert_eq!(app.navigation.menu.unwrap().group, 0);
    }
    let mut app = App::new(false);
    app.key(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        47,
        13,
    );
    assert!(app.tab == Tab::Overview);
}

#[test]
fn menu_and_wide_rail_share_the_visible_section_order() {
    assert_eq!(
        navigation::entry_order(6),
        vec![0, 1, 2],
        "Tools destinations are declared tools, installed tools and installation history"
    );
    let expected = [
        "Overview", "Files", "Tools", "Recovery", "Checks", "Git", "Session",
    ];
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.open_menu(0, w);
        render(&app, w, h);
        let mut rows: Vec<_> = (0..navigation::GROUPS.len())
            .map(|group| {
                let (_, y) = super::tests::target_position(
                    &app,
                    w,
                    h,
                    |t| matches!(t, Target::MenuGroup(id) if id == group),
                );
                (y, navigation::GROUPS[group])
            })
            .collect();
        rows.sort_by_key(|(y, _)| *y);
        assert_eq!(
            rows.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
            expected
        );
    }
    let app = App::new(false);
    render(&app, 140, 40);
    let mut rows: Vec<_> = [
        Tab::Overview,
        Tab::Files,
        Tab::Tools,
        Tab::Recovery,
        Tab::Checks,
        Tab::Git,
    ]
    .into_iter()
    .map(|tab| {
        let (_, y) = super::tests::target_position(
            &app,
            140,
            40,
            |t| matches!(t, Target::Destination(id) if id == tab),
        );
        (y, navigation::section_hint(tab).split(' ').next().unwrap())
    })
    .collect();
    rows.sort_by_key(|(y, _)| *y);
    assert_eq!(
        rows.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
        expected[..6]
    );
}

#[test]
fn section_letters_and_contextual_uppercase_remain_literal_in_foreground_fields() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(Form::Adopt));
        super::tests::focus_form(&mut app);
        for letter in "oftrcgOFTRCP123".chars() {
            app.key(
                KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE),
                w,
                h,
            );
        }
        assert_eq!(app.shell.adoption.target, "oftrcgOFTRCP123");
        assert!(
            app.tab == Tab::Overview
                && matches!(app.shell.overlay, Some(Overlay::Form(Form::Adopt)))
        );
        key(&mut app, KeyCode::Esc, w, h);
        app.key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT), w, h);
        assert!(matches!(
            app.shell.overlay,
            Some(Overlay::Form(Form::Adopt))
        ));
    }
}

#[test]
fn numeric_destinations_are_section_local_and_remain_text_in_inputs() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.repository = Some("/synthetic/repo".into());
        app.tab = Tab::Tools;
        assert!(matches!(
            app.key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE), w, h),
            Some(Task::ToolsInstalledPrepare(_))
        ));
        assert!(app.tools_view.installed_open && app.tools_view.installed.waiting);
        assert!(
            app.key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), w, h)
                .is_none()
        );
        app.tools_view.leave();
        assert!(matches!(
            app.key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), w, h),
            Some(Task::ToolAttempts)
        ));
        assert!(app.tools_view.install_open && !app.tools_view.declared.check_waiting);
        app.tools_view.declare();
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), w, h);
        let before = app.tools_view.declaration.fields[0].clone();
        app.key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE), w, h);
        assert_eq!(app.tools_view.declaration.fields[0], before + "4");
        assert!(!app.tools_view.install_open);
        app.open_menu(6, w);
        assert!(
            app.key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE), w, h)
                .is_none()
        );
        assert!(app.navigation.menu.is_some(), "Tools has no numbered tasks");
        let mut app = App::new(false);
        app.tab = Tab::Files;
        assert!(matches!(
            app.key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE), w, h),
            Some(Task::Discover)
        ));
        assert!(app.tab == Tab::Files && app.discovering && !app.tools_view.install_open);
        app.inventory.candidates.editing = true;
        app.key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), w, h);
        assert!(app.inventory.candidates.query.ends_with('3') && app.discovering);
        app.shell.overlay = Some(Overlay::Form(Form::Adopt));
        super::tests::focus_form(&mut app);
        app.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE), w, h);
        assert!(app.shell.adoption.target.ends_with('1'));
    }
}

#[test]
fn local_destination_overflow_is_an_explicit_menu_hit() {
    use super::components::navigation::{self as component, Destination};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let app = App::new(false);
    let mut terminal = Terminal::new(TestBackend::new(28, 4)).unwrap();
    terminal
        .draw(|frame| {
            app.hits.begin(frame.area());
            component::local(
                frame,
                Rect::new(0, 0, 28, 1),
                &[
                    Destination {
                        label: "Inventory",
                        selected: false,
                        target: Target::LocalDestination(6, 0),
                    },
                    Destination {
                        label: "Snapshots",
                        selected: true,
                        target: Target::LocalDestination(6, 3),
                    },
                    Destination {
                        label: "Reconcile",
                        selected: false,
                        target: Target::LocalDestination(6, 5),
                    },
                ],
                Target::MenuGroup(6),
                &app.hits,
            );
        })
        .unwrap();
    let _ = super::tests::target_position(&app, 28, 4, |t| matches!(t, Target::MenuGroup(6)));
    let _ =
        super::tests::target_position(&app, 28, 4, |t| matches!(t, Target::LocalDestination(6, 3)));
}

#[test]
fn review_report_has_hierarchy_and_preserves_escaped_metadata() {
    use super::components as ui;
    let text = ui::review_text(
        "Review\nEffects:\n- Create link\nPath: odd\\u{1b}name\nBLOCKED: unsafe parent",
    );
    assert_eq!(text.lines[0].style, ui::heading());
    assert_eq!(text.lines[1].style, ui::heading());
    assert_eq!(text.lines[2].spans[0].style, ui::muted());
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
        assert!(screen.contains("Menu") && screen.contains("Files / Managed"));
        super::tests::click_places(&mut app, w, h);
        assert!(app.navigation.menu.is_some());
        for label in ["Managed", "Discover", "Ignored paths"] {
            let screen = render(&app, w, h);
            assert!(screen.contains(label), "{label} at {w}");
            key(&mut app, KeyCode::Right, w, h);
        }
        key(&mut app, KeyCode::Down, w, h);
        let before = app.navigation.menu.unwrap();
        key(&mut app, KeyCode::F(1), w, h);
        assert_eq!(app.help.as_ref().unwrap().topic.title(), "Menu");
        key(&mut app, KeyCode::Enter, w, h);
        assert!(app.help.is_some() && app.tab == Tab::Files);
        key(&mut app, KeyCode::Esc, w, h);
        let after = app.navigation.menu.unwrap();
        assert_eq!(
            (before.group, before.selected),
            (after.group, after.selected)
        );
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_none());
        click(&mut app, w, h, |t| {
            matches!(t, Target::MenuOpen | Target::MenuGroup(2))
        });
        click(&mut app, w, h, |t| matches!(t, Target::MenuGroup(2)));
        key(&mut app, KeyCode::End, w, h);
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
        app.navigation.menu.as_mut().unwrap().selected = 0;
        for (index, entry) in navigation::entries(group).iter().enumerate() {
            app.navigation.menu.as_mut().unwrap().selected = index;
            let screen = render(&app, 48, 14);
            let label = navigation::local_catalog(group)
                .iter()
                .find(|(raw, _)| *raw == index)
                .map_or_else(
                    || entry.label.split(" [").next().unwrap(),
                    |(_, short)| *short,
                );
            assert!(screen.contains(label), "{label}: {screen}");
            let _ = super::tests::target_position(
                &app,
                48,
                14,
                |t| matches!(t, Target::MenuItem(i) if i == index),
            );
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
    super::tests::click_places(&mut app, 80, 24);
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
        super::tests::click_places(&mut app, w, h);
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| super::views::draw(f, &app)).unwrap();
        assert!(!terminal.backend().cursor_visible());
        let position =
            super::tests::target_position(&app, w, h, |t| matches!(t, Target::MenuGroup(2)));
        let cell = &terminal.backend().buffer()[position];
        assert_eq!(cell.symbol(), "›");
        assert_eq!(cell.bg, Color::Reset);
        assert!(cell.modifier.contains(Modifier::REVERSED | Modifier::BOLD));
        key(&mut app, KeyCode::Char('?'), w, h);
        assert_eq!(app.help.as_ref().unwrap().topic.title(), "Menu");
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_some());
        key(&mut app, KeyCode::Esc, w, h);
        assert!(app.navigation.menu.is_none() && app.inventory.managed.editing);
        assert_eq!(app.inventory.managed.query, "retained");
        key(&mut app, KeyCode::Char('?'), w, h);
        assert_eq!(app.inventory.managed.query, "retained?");
        super::tests::click_places(&mut app, w, h);
        key(&mut app, KeyCode::Char('q'), w, h);
        assert!(app.quit);
    }
    let mut app = App::new(false);
    app.tab = Tab::Overview;
    app.complete(Ok(super::actions::Output::Data(super::actions::Data {
        repository: Some("/synthetic/menu-repository".into()),
        ..Default::default()
    })));
    render(&app, 80, 24);
    let (x, y) = (0..24)
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x, y), Some(Target::Action(Action::Init))))
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
    assert!(
        app.navigation.menu.is_none()
            && app.shell.overlay.is_none()
            && app.preview.is_none()
            && app.confirmation.is_none()
    );
    assert!(app.tab == Tab::Overview);
}
#[test]
fn pages_keep_menus_while_tasks_retain_the_joined_navigation_guard() {
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
        super::tests::focus_form(&mut app);
        super::tests::click_places(&mut app, 80, 24);
        assert!(app.navigation.menu.is_some() && app.shell.overlay.is_some());
        key(&mut app, KeyCode::Esc, 80, 24);
        assert!(app.shell.overlay.is_some());
    }
    let mut app = App::new(false);
    app.start(false);
    super::tests::click_places(&mut app, 80, 24);
    assert!(app.navigation.menu.is_none());
    key(&mut app, KeyCode::Char('q'), 80, 24);
    assert!(app.exit_requested && !app.quit);
    let mut app = App::new(false);
    app.error = Some("metadata failure".into());
    super::tests::click_places(&mut app, 80, 24);
    assert!(app.navigation.menu.is_some());
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = "retained Git scope".into();
    super::tests::click_places(&mut app, 80, 24);
    assert!(app.navigation.menu.is_some());
    assert_eq!(app.git_view.draft, "retained Git scope");
    key(&mut app, KeyCode::Esc, 80, 24);
    app.git_view.editing_root = false;
    super::tests::click_places(&mut app, 80, 24);
    assert!(app.navigation.menu.is_some());
    app.resize(47, 13);
    assert!(app.navigation.menu.is_none());
}
