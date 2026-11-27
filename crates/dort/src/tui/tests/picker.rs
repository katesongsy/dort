//! Typed picker controls and keyboard-only compact metadata reachability.
use super::{
    actions::{Data, ManagedRow, Output, Task},
    app::{App, Tab},
    shell::{Form, Overlay, ProfilePicker},
    tests::render,
    views,
};
use crate::services::{CheckReport, ProfileChoices};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Modifier},
};

fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 48, 14)
}
fn long_path() -> String {
    format!(
        "/synthetic/{}/end-raw-q?\\n-path",
        (0..32)
            .map(|n| format!("segment-{n:02}/"))
            .collect::<String>()
    )
}
fn profiles(standalone: bool) -> App {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Profiles(Box::new(ProfilePicker {
        problem: None,
        standalone,
        selected: 0,
        requested: vec![],
        report: Box::new(CheckReport {
            manifest: long_path().into(),
            schema: 1,
            active_profiles: [
                "base".into(),
                "default".into(),
                "local".into(),
                "platform".into(),
            ]
            .into(),
            selected_tools: 0,
            selected_files: 0,
            tool_dependency_order: vec![],
            unsupported_capabilities: vec![],
            filesystem_and_tools_inspected: false,
            choices: ProfileChoices {
                known: [
                    "base".into(),
                    "default".into(),
                    "extra-q?".into(),
                    "local".into(),
                    "platform".into(),
                ]
                .into(),
                defaults: vec!["default".into()],
                platform: vec!["platform".into()],
                local: vec!["local".into()],
                mandatory: [
                    "base".into(),
                    "default".into(),
                    "local".into(),
                    "platform".into(),
                ]
                .into(),
            },
        }),
    })));
    app
}
// Stitch overlapping cell viewports while advancing using real keys, including
// the renderer's final-page clamp. Wrapped paths must reconstruct without gaps;
// direct offset assignment would hide oversized keyboard steps at the floor.
fn paged_cells(app: &mut App, modal: bool) -> String {
    if modal && !app.shell.picker_details {
        key(app, KeyCode::F(3));
    }
    let mut joined = String::new();
    for _ in 0..120 {
        let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
        terminal.draw(|f| views::draw(f, app)).unwrap();
        let b = terminal.backend().buffer();
        let title = if modal {
            "Selection details"
        } else if app.error.is_some() {
            "Read failed"
        } else {
            "Details"
        };
        let y = (0..14)
            .find(|y| {
                (0..48)
                    .map(|x| b[(x, *y)].symbol())
                    .collect::<String>()
                    .contains(title)
            })
            .expect("reading surface");
        let mut viewport = String::new();
        // Top-only panes use the full content width; the error banner is pinned.
        let pinned_rows = u16::from(app.error.is_some());
        let (left, right) = if modal {
            let left = (0..48)
                .find(|x| b[(*x, y)].symbol() == "│")
                .expect("dialog edge");
            let right = (0..48).rev().find(|x| b[(*x, y)].symbol() == "│").unwrap();
            (left + 1, right)
        } else {
            (0, 48)
        };
        for row in y + 1 + pinned_rows..13 {
            if modal && b[(left - 1, row + 1)].symbol() == "└" {
                break;
            }
            for x in left..right {
                viewport.push_str(b[(x, row)].symbol());
            }
        }
        let overlap = (0..=viewport.len().min(joined.len()))
            .rev()
            .find(|n| viewport.is_char_boundary(*n) && joined.ends_with(&viewport[..*n]))
            .unwrap();
        joined.push_str(&viewport[overlap..]);
        assert!(key(app, KeyCode::PageDown).is_none());
    }
    let offset = if modal {
        app.shell.scroll
    } else {
        app.details_scroll
    };
    assert!(key(app, KeyCode::PageUp).is_none());
    assert_eq!(
        if modal {
            app.shell.scroll
        } else {
            app.details_scroll
        },
        offset - 2 // Expanded metadata scrolls by overlapping two-row steps.
    );
    assert!(key(app, KeyCode::Home).is_none());
    assert_eq!(
        if modal {
            app.shell.scroll
        } else {
            app.details_scroll
        },
        0
    );
    if modal {
        key(app, KeyCode::F(3));
    }
    joined
}
#[test]
fn profile_table_distinguishes_membership_focus_and_apply_without_changing_scope() {
    let mut app = profiles(false);
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| views::draw(f, &app)).unwrap();
        let b = terminal.backend().buffer();
        let i = b.content.iter().position(|c| c.symbol() == "›").unwrap();
        assert_eq!(b.content[i].fg, Color::Reset);
        assert_eq!(b.content[i].bg, Color::Reset);
        assert!(
            b.content[i]
                .modifier
                .contains(Modifier::BOLD | Modifier::REVERSED)
        );
        let screen = render(&app, w, h);
        assert!(super::tests::has_action(&app, "Apply additions"));
        for label in ["PROFILE", "MEMBERSHIP", "Mandatory"] {
            assert!(screen.contains(label));
        }
    }
    key(&mut app, KeyCode::Char(' ')); // mandatory base is immutable
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' ')); // default is immutable too
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' ')); // optional addition
    let Some(Overlay::Profiles(p)) = &app.shell.overlay else {
        panic!("picker")
    };
    assert_eq!(p.requested, ["extra-q?"]);
    assert!(app.requested_profiles.is_empty());
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    assert!(super::tests::has_action(&app, "Apply additions"));
    let Some(Task::Profiles {
        manifest,
        requested,
        apply,
    }) = key(&mut app, KeyCode::Enter)
    else {
        panic!("validation task")
    };
    assert!(apply && manifest.is_none());
    assert_eq!(requested, ["extra-q?"]);
    assert!(app.requested_profiles.is_empty()); // task alone cannot apply
}
#[test]
fn mouse_profile_checkboxes_preserve_mandatory_union_and_separate_apply() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app = profiles(false);
    let click = |app: &mut App, label: &str| {
        let screen = render(app, 80, 24);
        let (x, y) = screen
            .lines()
            .enumerate()
            .find_map(|(y, row)| {
                row.find(label)
                    .map(|i| (row[..i].chars().count() as u16, y as u16))
            })
            .expect("visible synthetic control");
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            80,
            24,
        )
    };
    assert!(click(&mut app, "base").is_none());
    let Some(Overlay::Profiles(picker)) = &app.shell.overlay else {
        panic!("picker");
    };
    assert!(picker.requested.is_empty());
    assert!(click(&mut app, "extra-q?").is_none());
    assert!(app.requested_profiles.is_empty());
    assert!(
        matches!(click(&mut app, "Enter Apply additions"), Some(Task::Profiles { requested, apply: true, manifest: None }) if requested == ["extra-q?"])
    );
    assert!(app.requested_profiles.is_empty()); // read-only validation must finish first
}
#[test]
fn repository_picker_preserves_raw_indices_and_retained_create_adopt_drafts() {
    let path = long_path();
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        repository: Some("/explicit/session".into()),
        known_roots: vec![path.clone().into()],
        ..Default::default()
    })));
    key(&mut app, KeyCode::Char('p'));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, w, h);
        assert!(screen.contains("REPOSITORY") && screen.contains("CHOICE"));
        assert!(screen.contains("› Enter exact repository path"));
        assert!(
            super::workbench::actions(&app)
                .iter()
                .any(|a| matches!(a.target, super::input::Target::Picker(3, true)))
        );
    }
    key(&mut app, KeyCode::Down);
    assert!(matches!(
        key(&mut app, KeyCode::Enter),
        Some(Task::Select(None))
    ));
    // Selection indices still map directly to the old raw known-root vector.
    key(&mut app, KeyCode::Down);
    let Some(Task::Select(Some(root))) = key(&mut app, KeyCode::Enter) else {
        panic!("known root")
    };
    assert_eq!(root.to_str(), Some(path.as_str()));
    render(&app, 48, 14);
    assert_eq!(app.shell.scroll, 0);
    let text = paged_cells(&mut app, true);
    assert!(text.contains(&super::components::inline(&path)));
    assert!(text.contains("explicit/session"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('m'));
    super::tests::focus_form(&mut app);
    app.shell.adoption.target = "~/.keep-draft".into();
    key(&mut app, KeyCode::Char('p')); // text in target, not navigation
    // Direct nonprinting Choose and Create actions retain the adoption parent.
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        48,
        14,
    );
    assert!(super::tests::has_action(&app, "Create/attach"));
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            80,
            24
        )
        .is_none()
    );
    assert!(matches!(app.shell.overlay, Some(Overlay::Form(Form::Init))));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Form(Form::Adopt))
    ));
    assert_eq!(app.shell.adoption.target, "~/.keep-draftp");
}
#[test]
fn picker_details_start_at_first_row_and_all_contributors_are_key_reachable() {
    for standalone in [false, true] {
        let mut app = profiles(standalone);
        key(&mut app, KeyCode::F(3));
        assert!(render(&app, 48, 14).contains("Temporary profiles | Never persisted"));
        assert_eq!(app.shell.scroll, 0);
        let text = paged_cells(&mut app, true);
        for expected in [
            super::components::inline(&long_path()).as_str(),
            "Defaults",
            "default",
            "Platform",
            "Local",
            "Requested additions",
            "Effective union",
            "base",
            "Known profiles",
        ] {
            assert!(text.contains(expected), "reachable {expected}");
        }
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::PageUp);
        assert_eq!(app.shell.scroll, 0); // Home resets inline details; controls stay visible.
        key(&mut app, KeyCode::Esc);
        assert_eq!(
            matches!(app.shell.overlay, Some(Overlay::Form(Form::Check))),
            standalone
        );
        assert!(app.requested_profiles.is_empty() && app.shell.check_profiles.is_empty());
    }
    let mut app = App::new(false);
    key(&mut app, KeyCode::Char('p'));
    assert!(paged_cells(&mut app, true).contains("Not loaded; known roots unknown"));
}
#[test]
fn repository_availability_never_turns_an_unknown_registry_into_zero() {
    use super::actions::Overview;
    use dort_core::{
        context::HostContext,
        platform::{Architecture, Platform},
    };
    for available in [false, true] {
        let mut app = App::new(false);
        app.complete(Ok(Output::Data(Data {
            overview: Some(Overview {
                host: HostContext::new(
                    "/synthetic/home".into(),
                    None,
                    None,
                    Platform::Linux,
                    Architecture::X86_64,
                )
                .unwrap(),
                registered: None,
                registry_count: available.then_some(0),
                configuration_error: (!available)
                    .then(|| "Invalid TOML; manifest source text is not displayed.".into()),
                files: None,
            }),
            ..Default::default()
        })));
        key(&mut app, KeyCode::Char('p'));
        let text = paged_cells(&mut app, true);
        assert_eq!(text.contains("No known roots"), available);
        assert_eq!(
            text.contains("Unavailable; known roots unknown"),
            !available
        );
        if !available {
            assert!(text.contains("Invalid TOML"));
        }
    }
}
#[test]
fn ordinary_compact_details_have_overlapping_keyboard_pages_including_busy_help_and_errors() {
    let path = long_path();
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(Data {
        files: vec![("editor".into(), "not parsed".into())],
        managed: vec![ManagedRow {
            id: "editor".into(),
            mode: dort_core::model::FileMode::Link,
            source: Some(path.clone().into()),
            target: "/synthetic/home/.editor".into(),
            state: "Blocked",
            source_kind: "Regular file",
            target_kind: "Directory",
            blockers: vec![],
        }],
        ..Default::default()
    })));
    app.tab = Tab::Files;
    key(&mut app, KeyCode::Enter);
    assert!(paged_cells(&mut app, false).contains(&super::components::inline(&path)));
    app.key(
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        48,
        14,
    );
    let first = render(&app, 48, 14);
    assert!(first.contains("Managed file details"));
    app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 48, 14);
    assert_ne!(render(&app, 48, 14), first);
    app.busy = true;
    app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 48, 14);
    assert!(render(&app, 48, 14).contains("Managed file details"));
    app.busy = false;
    key(&mut app, KeyCode::Esc);
    app.complete(Err(format!("metadata error {path}")));
    assert!(paged_cells(&mut app, false).contains(&super::components::inline(&path)));
}

#[test]
fn picker_splits_keep_one_active_owner_and_compact_back_keeps_the_row() {
    let mut repositories = App::new(false);
    repositories.shell.overlay = Some(Overlay::Repositories { selected: 1 });
    let mut ignored = App::new(false);
    ignored.tab = Tab::Files;
    ignored.shell.overlay = Some(Overlay::Ignored {
        paths: vec!["/synthetic/first".into(), "/synthetic/second".into()],
        selected: 0,
    });
    for (mut app, queue, reader) in [
        (profiles(false), "Repository profiles", "Selection details"),
        (repositories, "Repository choices", "Selection details"),
        (ignored, "Ignored paths", "Ignored path details"),
    ] {
        let wide = (120, 26);
        let press = |app: &mut App, code| {
            assert!(
                app.key(KeyEvent::new(code, KeyModifiers::NONE), wide.0, wide.1)
                    .is_none()
            );
        };
        let before = render(&app, wide.0, wide.1);
        assert!(
            before.contains(&format!("{queue} [Active]")) && before.contains(reader),
            "{before}"
        );
        let row = |screen: &str| {
            screen
                .lines()
                .find_map(|line| line.split_once("› "))
                .map(|(_, value)| value.split_whitespace().next().unwrap().to_owned())
        };
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('y'));
        let focused = render(&app, wide.0, wide.1);
        assert_eq!(focused.matches("[Active]").count(), 1, "{focused}");
        assert!(focused.contains(&format!("{reader} [Active]")), "{focused}");
        assert_eq!(row(&before), row(&focused));
        let compact = render(&app, 48, 14);
        assert!(
            compact.contains(&format!("{reader} [Active]")) && row(&compact).is_none(),
            "{compact}"
        );
        assert!(key(&mut app, KeyCode::Esc).is_none());
        let queue_screen = render(&app, 48, 14);
        assert!(
            queue_screen.contains(&format!("{queue} [Active]")) && !queue_screen.contains(reader),
            "{queue_screen}"
        );
        assert_eq!(row(&before), row(&queue_screen));
    }
}
