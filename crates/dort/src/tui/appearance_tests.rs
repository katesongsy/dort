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
            for label in ["2 Files", "Blocked", "Review apply", "\\u{e9}"] {
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
            let active = locate(&compatible, "2 Files");
            assert!(
                compatible.backend().buffer()[active]
                    .modifier
                    .contains(Modifier::REVERSED | Modifier::BOLD)
            );
            let button = locate(&compatible, "Review apply");
            assert!(matches!(
                app.hits.target(button.0, button.1),
                Some(Target::Action(_))
            ));
            assert!(matches!(
                app.mouse(
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: button.0,
                        row: button.1,
                        modifiers: KeyModifiers::NONE
                    },
                    w,
                    h
                ),
                Some(Task::Files(dort_core::planner::Request::Apply))
            ));
            // The displayed escaped ID must never replace the actual request ID.
            let task = key(&mut app, KeyCode::Char('d'), w, h);
            assert!(
                matches!(task,Some(Task::Files(dort_core::planner::Request::Unmanage { ref id, .. })) if id == "é")
            );
            assert!(app.confirmation.is_none());
        }
    }
}
#[test]
fn compatible_forms_keep_caret_tab_and_mouse_controls_at_all_sizes() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        key(&mut app, KeyCode::Char('o'), w, h);
        for c in "~/.editor".chars() {
            key(&mut app, KeyCode::Char(c), w, h);
        }
        let mut original = draw(&app, w, h);
        let caret = original.get_cursor_position().unwrap();
        app.appearance = appearance(true, true);
        let mut compatible = draw(&app, w, h);
        assert_eq!(compatible.get_cursor_position().unwrap(), caret);
        check_cells(&compatible, true, true);
        locate(&compatible, "TARGET | Focus");
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
        for destination in '1'..='5' {
            key(&mut app, KeyCode::Char(destination), w, h);
            check_cells(&draw(&app, w, h), true, true);
            key(&mut app, KeyCode::Tab, w, h);
            assert!(
                app.tab
                    == match destination {
                        '1' => Tab::Overview,
                        '2' => Tab::Files,
                        '3' => Tab::Recovery,
                        '4' => Tab::Checks,
                        _ => Tab::Git,
                    }
            );
            key(&mut app, KeyCode::Char('?'), w, h);
            check_cells(&draw(&app, w, h), true, true);
            key(&mut app, KeyCode::Esc, w, h);
        }
        for shortcut in [' ', 'p', 'n', 'v', 't'] {
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
    assert_eq!(
        (selected.fg, selected.bg),
        (Some(Color::Black), Some(Color::Cyan))
    );
    // Explicit xterm-like ANSI assumptions, not a claim about user-customized
    // palettes. Reversal swaps the pair but does not change its contrast ratio.
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
        assert!(contrast([0.0; 3], [0.0, 0.8, 0.8]) >= 4.5);
        let mut app = fixture();
        app.appearance = appearance(true, true);
        check_cells(&draw(&app, 80, 24), true, true);
    }
}
