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
    let mut joined = String::new();
    for _ in 0..120 {
        let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
        terminal.draw(|f| views::draw(f, app)).unwrap();
        let b = terminal.backend().buffer();
        let y = (0..14)
            .find(|y| {
                (0..48)
                    .map(|x| b[(x, *y)].symbol())
                    .collect::<String>()
                    .contains("┌")
            })
            .unwrap();
        let mut viewport = String::new();
        for row in y + 1..14 {
            if b[(0, row)].symbol() == "└" {
                break;
            }
            for x in 1..47 {
                viewport.push_str(b[(x, row)].symbol());
            }
        }
        let overlap = (0..=viewport.len().min(joined.len()))
            .rev()
            .find(|n| joined.ends_with(&viewport[..*n]))
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
        offset - 2
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
    joined
}
#[test]
fn profile_table_distinguishes_membership_focus_and_apply_without_changing_scope() {
    let mut app = profiles(false);
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| views::draw(f, &app)).unwrap();
        let b = terminal.backend().buffer();
        let i = b.content.iter().position(|c| c.symbol() == ">").unwrap();
        assert_eq!(b.content[i].bg, Color::Cyan);
        assert!(
            b.content[i]
                .modifier
                .contains(Modifier::BOLD | Modifier::REVERSED)
        );
        let screen = render(&app, w, h);
        for label in ["PROFILE", "MEMBERSHIP", "Mandatory", "Apply additions"] {
            assert!(screen.contains(label));
        }
    }
    key(&mut app, KeyCode::Enter); // mandatory base is immutable
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter); // default is immutable too
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
    assert!(render(&app, 48, 14).contains("> [Apply additions"));
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
        matches!(click(&mut app, "[Apply additions"), Some(Task::Profiles { requested, apply: true, manifest: None }) if requested == ["extra-q?"])
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
        assert!(screen.contains("> Enter exact repository path"));
        assert!(screen.contains("Create/attach"));
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
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.shell.scroll, 1);
    let text = paged_cells(&mut app, true);
    assert!(text.contains(&super::components::inline(&path)));
    assert!(text.contains("explicit/session"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('o'));
    app.shell.adoption.target = "~/.keep-draft".into();
    key(&mut app, KeyCode::Char('p')); // text in target, not navigation
    // Use the actual Choose button to retain adoption return intent.
    app.shell.adoption.focused = 4;
    key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    assert!(render(&app, 48, 14).contains("> [Create/attach"));
    assert!(key(&mut app, KeyCode::Enter).is_none());
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
        key(&mut app, KeyCode::PageDown);
        assert_eq!(app.shell.scroll, 1);
        assert!(render(&app, 48, 14).contains("Temporary profiles | Never persisted"));
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
        assert_eq!(app.shell.scroll, 0); // controls, not an offset-one trap
        key(&mut app, KeyCode::Esc);
        assert_eq!(
            matches!(app.shell.overlay, Some(Overlay::Form(Form::Check))),
            standalone
        );
        assert!(app.requested_profiles.is_empty() && app.shell.check_profiles.is_empty());
    }
    let mut app = App::new(false);
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::PageDown);
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
                    Platform::Ubuntu,
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
        key(&mut app, KeyCode::PageDown);
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
            source: path.clone().into(),
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
    key(&mut app, KeyCode::Char('?'));
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.shell.scroll, 2);
    app.busy = true;
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.shell.scroll, 4);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.shell.scroll, 0);
    app.busy = false;
    key(&mut app, KeyCode::Esc);
    app.complete(Err(format!("metadata error {path}")));
    assert!(paged_cells(&mut app, false).contains(&super::components::inline(&path)));
}
