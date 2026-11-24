//! Synthetic cells establish rendering contracts, not physical theme acceptance.
use super::{
    actions::{Data, ManagedRow, Output, Task},
    app::{App, Tab},
    appearance::Appearance,
    input::Target,
    views,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    style::{Color, Modifier},
};
fn appearance(ascii: bool, mono: bool) -> Appearance {
    Appearance::classify(
        Some("xterm"),
        Some(if ascii { "C" } else { "C.UTF-8" }),
        None,
        None,
        mono.then_some("1"),
    )
}
fn fixture() -> App {
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        repository: Some("/synthetic/root".into()),
        files: vec![("é".into(), "synthetic metadata".into())],
        managed: vec![ManagedRow {
            id: "é".into(),
            source: "/synthetic/source".into(),
            target: "/synthetic/target".into(),
            state: "Blocked",
            source_kind: "Regular",
            target_kind: "Directory",
            blockers: vec!["synthetic blocker".into()],
        }],
        ..Default::default()
    })));
    app.tab = Tab::Files;
    app
}
fn draw(app: &App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|frame| views::draw(frame, app)).unwrap();
    terminal
}
fn locate(terminal: &Terminal<TestBackend>, needle: &str) -> (u16, u16) {
    let b = terminal.backend().buffer();
    for y in 0..b.area.height {
        let row: String = (0..b.area.width).map(|x| b[(x, y)].symbol()).collect();
        if let Some(byte) = row.find(needle) {
            return (row[..byte].chars().count() as u16, y);
        }
    }
    panic!("missing synthetic label {needle}")
}
fn check_cells(t: &Terminal<TestBackend>, ascii: bool, mono: bool) {
    for cell in &t.backend().buffer().content {
        if ascii {
            assert!(cell.symbol().is_ascii(), "unsupported decoration");
        }
        if mono {
            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
            assert!(!cell.modifier.contains(Modifier::DIM));
        }
    }
}
fn key(app: &mut App, code: KeyCode, w: u16, h: u16) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), w, h)
}
#[test]
fn compatible_cells_preserve_geometry_raw_mouse_identity_and_textual_states() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for (ascii, mono) in [(true, true), (true, false), (false, true)] {
            let mut app = fixture();
            let original = draw(&app, w, h);
            let original_hits: Vec<_> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| {
                    format!(
                        "{:?}",
                        app.hits
                            .target(x, y)
                            .map(|target| std::mem::discriminant(&target))
                    )
                })
                .collect();
            app.appearance = appearance(ascii, mono);
            let compatible = draw(&app, w, h);
            check_cells(&compatible, ascii, mono);
            for label in ["Files / Managed", "Blocked", "F2 More", "\\u{e9}"] {
                assert_eq!(locate(&original, label), locate(&compatible, label));
            }
            let mut index = 0;
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(
                        format!(
                            "{:?}",
                            app.hits
                                .target(x, y)
                                .map(|target| std::mem::discriminant(&target))
                        ),
                        original_hits[index]
                    );
                    index += 1;
                }
            }
            assert!(
                (0..h).any(|y| (0..w).any(|x| {
                    compatible.backend().buffer()[(x, y)]
                        .modifier
                        .contains(Modifier::REVERSED | Modifier::BOLD)
                        && matches!(app.hits.target(x, y), Some(Target::Row(_)))
                })),
                "the selected row, not the location or inspector, owns inverse emphasis"
            );
            let button = locate(&compatible, "F2 More");
            assert!(matches!(
                app.hits.target(button.0, button.1),
                Some(Target::More)
            ));
            assert!(
                app.mouse(
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: button.0,
                        row: button.1,
                        modifiers: KeyModifiers::NONE
                    },
                    w,
                    h
                )
                .is_none()
            );
            let index = app
                .more
                .as_ref()
                .unwrap()
                .items
                .iter()
                .position(|item| {
                    matches!(
                        item.action.target,
                        Target::Action(super::shell::Action::Apply)
                    )
                })
                .expect("review action");
            key(&mut app, KeyCode::Home, w, h);
            for _ in 0..index {
                key(&mut app, KeyCode::Down, w, h);
            }
            assert!(matches!(
                key(&mut app, KeyCode::Enter, w, h),
                Some(Task::Files(dort_core::planner::Request::Apply))
            ));
            // The displayed escaped ID must never replace the actual request ID.
            let task = app.activate(super::shell::Action::Remove);
            assert!(
                matches!(task,Some(Task::Files(dort_core::planner::Request::Unmanage { ref id, .. })) if id == "é")
            );
            assert!(app.confirmation.is_none());
        }
    }
}
#[test]
fn signal_tools_chrome_and_focus_keep_their_geometry_without_color() {
    let fixture = crate::services::declared_tools_tests::Fixture::new();
    for (w, h) in [(48, 14), (140, 40)] {
        let mut app = super::tools_tests::app(&fixture, false);
        app.appearance =
            appearance(false, false).with_color_depth(Some("xterm-256color"), Some("truecolor"));
        let color = draw(&app, w, h);
        let cells = color.backend().buffer();
        let brand = &cells[(0, 0)];
        let breadcrumb = &cells[(7, 0)];
        let leaf = &cells[(15, 0)];
        let footer = &cells[(w - 1, h - 1)];
        assert_eq!(locate(&color, "Dort / Tools / Declared tools"), (0, 0));
        assert_eq!(brand.bg, Color::Rgb(0x16, 0x24, 0x2e));
        assert_eq!(brand.bg, footer.bg);
        assert_eq!(brand.bg, leaf.bg);
        assert_ne!(brand.fg, leaf.fg);
        assert_ne!(breadcrumb.fg, leaf.fg);
        assert!(brand.modifier.contains(Modifier::BOLD));
        assert!(leaf.modifier.contains(Modifier::BOLD));
        assert!(!breadcrumb.modifier.contains(Modifier::BOLD));

        let primary = locate(&color, "Enter");
        let secondary = locate(&color, "F1 Help");
        let tab = locate(&color, "1 Declared");
        let marker = locate(&color, "› a");
        let row = locate(&color, "Not checked");
        let body = locate(&color, "Check declared tools;");
        let mut focused = vec![primary, tab, row];
        if w >= 100 {
            let rail = locate(&color, "Tools t");
            assert_eq!(rail, (2, 3));
            assert_eq!(cells[rail].fg, brand.bg);
            assert!(matches!(
                app.hits.target(rail.0, rail.1),
                Some(Target::Destination(Tab::Tools))
            ));
            focused.push(rail);
        }
        assert_eq!(primary, (1, h - 1));
        assert_eq!(cells[primary].fg, brand.fg);
        assert_eq!(cells[marker].bg, brand.fg);
        assert_eq!(cells[marker].fg, cells[row].fg);
        assert_ne!(cells[row].fg, cells[primary].fg);
        assert_eq!(cells[secondary].bg, footer.bg);
        assert_ne!(
            cells[secondary].fg,
            cells[(secondary.0 + 3, secondary.1)].fg
        );
        assert!(!cells[secondary].modifier.contains(Modifier::REVERSED));
        assert!(cells[secondary].modifier.contains(Modifier::BOLD));
        for pos in [body, (row.0, row.1 + 1)] {
            assert_eq!((cells[pos].fg, cells[pos].bg), (Color::Reset, Color::Reset));
            assert!(!cells[pos].modifier.contains(Modifier::REVERSED));
        }
        let target = |(x, y)| {
            app.hits
                .target(x, y)
                .map(|target| std::mem::discriminant(&target))
        };
        let hits = focused.iter().map(|&pos| target(pos)).collect::<Vec<_>>();
        assert!(hits.iter().all(Option::is_some));

        for term in ["xterm-256color", "xterm"] {
            app.appearance = appearance(false, false).with_color_depth(Some(term), None);
            let fallback = draw(&app, w, h);
            let fallback_cells = fallback.backend().buffer();
            for &pos in &focused {
                let cell = &fallback_cells[pos];
                assert_eq!(cell.symbol(), cells[pos].symbol());
                assert!(cell.modifier.contains(Modifier::REVERSED));
                if term == "xterm-256color" {
                    assert!(
                        matches!((cell.fg, cell.bg), (Color::Indexed(fg), Color::Indexed(bg)) if fg != bg)
                    );
                } else {
                    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
                }
            }
        }

        app.appearance = appearance(false, true);
        let mono = draw(&app, w, h);
        check_cells(&mono, false, true);
        let mono_cells = mono.backend().buffer();
        assert!(
            cells
                .content
                .iter()
                .map(|c| c.symbol())
                .eq(mono_cells.content.iter().map(|c| c.symbol()))
        );
        for (pos, hit) in focused.into_iter().zip(hits) {
            assert_ne!(cells[pos].fg, cells[pos].bg);
            assert!(cells[pos].modifier.contains(Modifier::REVERSED));
            assert!(mono_cells[pos].modifier.contains(Modifier::REVERSED));
            assert_eq!(
                app.hits
                    .target(pos.0, pos.1)
                    .map(|target| std::mem::discriminant(&target)),
                hit
            );
        }
    }
}

#[test]
fn signal_check_review_keeps_warning_and_foreground_action_distinct() {
    let fixture = crate::services::declared_tools_tests::Fixture::new();
    let mut app = super::tools_tests::app(&fixture, false);
    let (w, h) = (48, 14);
    let Some(Task::ToolsCheckPrepare(mut inputs)) = key(&mut app, KeyCode::Char('C'), w, h) else {
        panic!("Check installed must open a review");
    };
    inputs.search_dirs = vec![fixture.bin.clone()];
    super::tools_tests::run(&fixture, &mut app, Task::ToolsCheckPrepare(inputs));
    key(&mut app, KeyCode::End, w, h);
    let color = draw(&app, w, h);
    let cells = color.backend().buffer();
    let warning = locate(&color, "Checks can run package-manager");
    let primary = locate(&color, "y Check");
    let rule = locate(&color, "──");
    assert_eq!(
        (cells[warning].fg, cells[warning].bg),
        (Color::Yellow, Color::Reset)
    );
    assert!(cells[warning].modifier.contains(Modifier::BOLD));
    assert_eq!(
        (cells[rule].fg, cells[rule].bg),
        (Color::Reset, Color::Reset)
    );
    assert!(cells[rule].modifier.contains(Modifier::DIM));
    assert_eq!(primary, (1, h - 1));
    assert_eq!(cells[primary].fg, cells[(0, 0)].fg);
    assert!(
        cells[primary]
            .modifier
            .contains(Modifier::BOLD | Modifier::REVERSED)
    );
    assert!(!cells[primary].modifier.contains(Modifier::DIM));
    assert!(fixture.calls().is_empty());

    app.appearance = appearance(false, true);
    let mono = draw(&app, w, h);
    let mono_cells = mono.backend().buffer();
    check_cells(&mono, false, true);
    assert_eq!(locate(&mono, "Checks can run package-manager"), warning);
    assert!(mono_cells[warning].modifier.contains(Modifier::BOLD));
    assert!(!mono_cells[warning].modifier.contains(Modifier::REVERSED));
    assert_eq!(locate(&mono, "y Check"), primary);
    assert!(
        mono_cells[primary]
            .modifier
            .contains(Modifier::BOLD | Modifier::REVERSED)
    );
}

#[test]
fn compatible_forms_keep_caret_tab_and_mouse_controls_at_all_sizes() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        key(&mut app, KeyCode::Char('m'), w, h);
        super::tests::focus_form(&mut app);
        for c in "~/.editor".chars() {
            key(&mut app, KeyCode::Char(c), w, h);
        }
        let mut original = draw(&app, w, h);
        let caret = original.get_cursor_position().unwrap();
        app.appearance = appearance(true, true);
        let mut compatible = draw(&app, w, h);
        assert_eq!(compatible.get_cursor_position().unwrap(), caret);
        check_cells(&compatible, true, true);
        locate(&compatible, "TARGET");
        assert!(compatible.backend().cursor_visible());
        if w == 48 {
            key(&mut app, KeyCode::Tab, w, h);
            compatible = draw(&app, w, h);
        }
        let (x, y) = locate(&compatible, "ID");
        assert!(
            app.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE
                },
                w,
                h
            )
            .is_none()
        );
        assert_eq!(app.shell.adoption.focused, 1);
        key(&mut app, KeyCode::Char('q'), w, h);
        key(&mut app, KeyCode::Tab, w, h);
        assert_eq!(app.shell.adoption.id, "q");
        assert_eq!(app.shell.adoption.focused, 2);
        for _ in 0..5 {
            key(&mut app, KeyCode::Tab, w, h);
            check_cells(&draw(&app, w, h), true, true);
        }
    }
}
#[test]
fn every_destination_and_common_overlays_use_supported_ascii_decoration() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = fixture();
        app.appearance = appearance(true, true);
        for destination in [
            Tab::Overview,
            Tab::Files,
            Tab::Recovery,
            Tab::Checks,
            Tab::Git,
        ] {
            super::tests::destination(&mut app, destination, w, h);
            check_cells(&draw(&app, w, h), true, true);
            key(&mut app, KeyCode::Tab, w, h);
            assert!(app.tab == destination);
            key(&mut app, KeyCode::Char('?'), w, h);
            check_cells(&draw(&app, w, h), true, true);
            key(&mut app, KeyCode::Esc, w, h);
        }
        for shortcut in [' ', 'p', 'n', 'v', 'T'] {
            key(&mut app, KeyCode::Char(shortcut), w, h);
            check_cells(&draw(&app, w, h), true, true);
            key(&mut app, KeyCode::Esc, w, h);
            key(&mut app, KeyCode::Esc, w, h);
        }
        app.complete(Err("synthetic metadata failure".into()));
        check_cells(&draw(&app, w, h), true, true);
    }
}

#[test]
fn default_canvas_and_selected_pair_have_synthetic_light_dark_contrast() {
    use super::components;
    assert_eq!(components::muted(), ratatui::style::Style::default());
    let selected = components::selected();
    assert_eq!((selected.fg, selected.bg), (None, None));
    assert!(
        selected
            .add_modifier
            .contains(Modifier::BOLD | Modifier::REVERSED)
    );
    // Explicit native foreground/background assumptions, not a claim about
    // user-customized palettes. Inversion keeps their contrast ratio.
    fn luminance(rgb: [f64; 3]) -> f64 {
        let linear = rgb.map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    }
    fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    for (fg, bg) in [([0.0; 3], [1.0; 3]), ([1.0; 3], [0.0; 3])] {
        assert!(contrast(fg, bg) >= 4.5);
        assert!(contrast(bg, fg) >= 4.5);
        let mut app = fixture();
        app.appearance = appearance(true, true);
        check_cells(&draw(&app, 80, 24), true, true);
    }
}
