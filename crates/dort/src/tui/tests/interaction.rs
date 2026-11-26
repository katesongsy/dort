//! Page/dialog semantics, direct primary actions and trusted-chrome boundaries.
use super::{
    actions::{self, Task},
    app::{App, Tab},
    components as ui,
    input::Target,
    shell::{Form, Overlay, ProfilePicker},
    tests::render,
    views,
};
use crate::services::{CheckReport, ProfileChoices};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    style::Modifier,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
fn key(app: &mut App, code: KeyCode, w: u16, h: u16) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), w, h)
}
fn click(app: &mut App, w: u16, h: u16, want: impl Fn(Target) -> bool) -> Option<Task> {
    render(app, w, h);
    let (x, y) = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .find(|&(x, y)| app.hits.target(x, y).is_some_and(&want))
        .expect("visible typed control");
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        w,
        h,
    )
}
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    bin: PathBuf,
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        for (name, body) in [
            ("fake-provider", "printf '{}';"),
            ("brew", "printf '{\"formulae\":[]}';"),
            ("dpkg-query", ":;"),
        ] {
            let p = bin.join(name);
            fs::write(
                &p,
                format!("#!/bin/sh\nprintf Q >> \"$HOME/calls\"\n{body}\n"),
            )
            .unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nprovider='brew'\npackage='demo'\n").unwrap();
        let lock = dort_core::state::WriterLock::acquire(&host).unwrap();
        dort_core::local::RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        Self {
            _temp: temp,
            host,
            bin,
            repo,
        }
    }
    fn app(&self, dry: bool) -> App {
        let mut app = App::new(dry);
        app.tab = Tab::Tools;
        app.repository = Some(self.repo.clone());
        app.complete(Ok(actions::perform_scoped(
            Task::Refresh,
            &self.host,
            Some(self.repo.clone()),
            &[],
            |_| Ok(()),
        )
        .unwrap()));
        app
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
}
#[test]
fn list_enter_opens_details_and_only_explicit_review_approves_checks_once() {
    let f = Fixture::new();
    let mut app = f.app(false);
    assert!(key(&mut app, KeyCode::Enter, 80, 24).is_none());
    assert!(app.tools_view.declared_details && f.calls().is_empty());
    let Some(Task::ToolsCheckPrepare(mut input)) = key(&mut app, KeyCode::Char('C'), 80, 24) else {
        panic!("named check action")
    };
    input.search_dirs = vec![f.bin.clone()];
    input.overrides.clear();
    let out = actions::perform_scoped(
        Task::ToolsCheckPrepare(input),
        &f.host,
        Some(f.repo.clone()),
        &[],
        |_| Ok(()),
    )
    .unwrap();
    app.complete(Ok(out));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let s = render(&app, w, h);
        assert!(
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("change caches"),
            "{s}"
        );
        assert!(!s.contains("Snapshot") && !s.contains("\\u{b7}"));
    }
    assert!(f.calls().is_empty());
    let task = super::tests::approve_ready(&mut app, 80, 24).unwrap();
    assert!(matches!(task, Task::ToolsCheck(_)));
    app.start(false);
    assert!(key(&mut app, KeyCode::Enter, 80, 24).is_none());
    app.complete(Ok(actions::perform_scoped(
        task,
        &f.host,
        Some(f.repo.clone()),
        &[],
        |_| Ok(()),
    )
    .unwrap()));
    assert_eq!(f.calls(), "Q");
    assert!(key(&mut app, KeyCode::Enter, 80, 24).is_none());
    assert_eq!(f.calls(), "Q");
    let mut dry = f.app(true);
    assert!(key(&mut dry, KeyCode::Enter, 48, 14).is_none());
    assert!(dry.tools_view.declared_details);
}
#[test]
fn trusted_separators_render_but_program_path_status_metadata_stays_escaped() {
    let f = Fixture::new();
    let path = f.bin.join("provider·\u{202e}\u{1b}");
    fs::copy(f.bin.join("fake-provider"), &path).unwrap();
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.tools_view.settings_open = true;
    app.tools_view.settings.fields[0] = path.display().to_string();
    app.status = "Provider metadata: · \u{202e}\u{1b}".to_owned().into();
    let s = render(&app, 320, 40);
    assert!(s.contains("Tool settings"));
    assert!(s.contains("provider\\u{b7}"));
    assert!(
        f.calls().is_empty(),
        "Displaying settings never executes programs"
    );
    assert!(s.contains("\\u{b7}") && s.contains("\\u{202e}") && s.contains("\\u{1b}"));
    assert!(!s.contains('\u{202e}') && !s.contains('\u{1b}'));
    app.tab = Tab::Overview; // Ordinary status still uses the escaped metadata boundary.
    let s = render(&app, 320, 40);
    assert!(s.contains("\\u{b7}") && s.contains("\\u{202e}") && s.contains("\\u{1b}"));
    app.status = "Trusted · separator".into();
    let s = render(&app, 80, 24);
    assert!(s.contains("Trusted · separator"));
    app.appearance =
        super::appearance::Appearance::classify(Some("vt100"), None, None, Some("C"), None);
    let s = render(&app, 320, 40);
    assert!(s.contains("Trusted | separator"));
    app.status = "Provider metadata: · \u{202e}\u{1b}".to_owned().into();
    let s = render(&app, 320, 40);
    assert!(s.contains("\\u{202e}") && s.contains("\\u{b7}"));
    assert_eq!(ui::chrome_clip("界界界", 4), "...");
}
#[test]
fn workflow_pages_keep_field_text_focus_until_the_form_is_left() {
    for size in [(48, 14), (80, 24), (140, 40)] {
        for (index, form) in [
            Form::Choose,
            Form::Init,
            Form::Adopt,
            Form::Check,
            Form::KeepId,
            Form::RemoveId,
        ]
        .into_iter()
        .enumerate()
        {
            let mut app = App::new(false);
            app.shell.overlay = Some(Overlay::Form(form));
            super::tests::focus_form(&mut app);
            app.shell.init_draft = "retained".into();
            assert!(app.navigation_available() && app.dialog_title().is_none());
            assert!(render(&app, size.0, size.1).starts_with("Dort /"));
            assert!(app.page_title().is_some());
            key(&mut app, KeyCode::Char('q'), size.0, size.1);
            assert!(!app.quit);
            let path = app.shell.init_draft.clone();
            let focused = app.shell.path_focused;
            key(&mut app, KeyCode::F(1), size.0, size.1);
            assert!(app.help.is_some());
            key(&mut app, KeyCode::Esc, size.0, size.1);
            assert!(matches!(app.shell.overlay, Some(Overlay::Form(f)) if f == form));
            assert_eq!(app.shell.init_draft, path);
            assert_eq!(app.shell.path_focused, focused);
            key(&mut app, KeyCode::Esc, size.0, size.1); // Stop editing.
            key(&mut app, KeyCode::Esc, size.0, size.1); // Leave the form.
            assert!(app.shell.overlay.is_none(), "{index} {size:?}");
            key(&mut app, KeyCode::Char('o'), size.0, size.1);
            assert!(app.tab == Tab::Overview);
            assert_eq!(app.shell.init_draft, path);
        }
        let mut app = App::new(false);
        app.tab = Tab::Tools;
        app.tools_view.declaration_open = true;
        app.tools_view.declaration.fields[0] = "retained".into();
        super::tests::focus_form(&mut app);
        key(&mut app, KeyCode::Char('q'), size.0, size.1);
        assert_eq!(app.tools_view.declaration.fields[0], "retainedq");
        for _ in 0..30 {
            key(&mut app, KeyCode::Tab, size.0, size.1);
            assert!(app.tools_view.declaration.focus < 12);
        }
        let focus = app.tools_view.declaration.focus;
        key(&mut app, KeyCode::F(1), size.0, size.1);
        assert!(app.help.is_some());
        key(&mut app, KeyCode::Esc, size.0, size.1);
        assert_eq!(app.tools_view.declaration.focus, focus);
        assert_eq!(app.tools_view.declaration.fields[0], "retainedq");
    }
}
fn profile_app() -> App {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Profiles(Box::new(ProfilePicker {
        problem: None,
        standalone: false,
        selected: 0,
        requested: vec![],
        report: Box::new(CheckReport {
            manifest: "/synthetic/profile.toml".into(),
            schema: 1,
            active_profiles: ["base".into()].into(),
            selected_tools: 0,
            selected_files: 0,
            tool_dependency_order: vec![],
            unsupported_capabilities: vec![],
            filesystem_and_tools_inspected: false,
            choices: ProfileChoices {
                known: ["base".into(), "work".into()].into(),
                defaults: vec![],
                platform: vec![],
                local: vec![],
                mandatory: ["base".into()].into(),
            },
        }),
    })));
    app
}
#[test]
fn dialogs_dim_the_background_and_dismiss_outside_without_forwarding_clicks() {
    for size in [(48, 14), (80, 24), (140, 40)] {
        for which in 0..6 {
            let mut app = if which == 1 {
                profile_app()
            } else {
                App::new(false)
            };
            match which {
                0 => app.shell.overlay = Some(Overlay::Repositories { selected: 0 }),
                1 => app.shell.picker_details = true,
                2 => {
                    key(&mut app, KeyCode::F(2), size.0, size.1);
                    key(&mut app, KeyCode::F(3), size.0, size.1);
                }
                3 => app.shell.overlay = Some(Overlay::About),
                4 => {
                    key(&mut app, KeyCode::F(1), size.0, size.1);
                }
                5 => {
                    app.tab = Tab::Tools;
                    app.tools_view.declare();
                    super::tests::focus_form(&mut app);
                    app.tools_view.declaration.focus = 4;
                    assert!(key(&mut app, KeyCode::Left, size.0, size.1).is_none());
                    assert!(app.choice.is_some());
                    key(&mut app, KeyCode::F(3), size.0, size.1);
                }
                _ => unreachable!(),
            }
            assert!(app.dialog_title().is_some() && !app.navigation_available());
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
            terminal.draw(|f| views::draw(f, &app)).unwrap();
            assert!(
                terminal.backend().buffer()[(1, 0)]
                    .modifier
                    .contains(Modifier::DIM)
            );
            assert!(render(&app, size.0, size.1).contains(app.dialog_title().unwrap()));
            render(&app, size.0, size.1);
            assert!(matches!(app.hits.target(0, 0), Some(Target::DismissDialog)));

            let task = app.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                },
                size.0,
                size.1,
            );
            assert!(task.is_none() && !app.quit);
            if which == 5 {
                assert!(app.tools_view.declaration_open);
            }
            assert!(
                app.dialog_title().is_none(),
                "dialog close at {:?}, case {which}",
                size
            );
        }
    }
}
#[test]
fn picker_rows_toggle_without_applying_and_metadata_has_no_background_caret() {
    for size in [(48, 14), (80, 24), (140, 40)] {
        let mut app = profile_app();
        click(&mut app, size.0, size.1, |t| {
            matches!(t, Target::Picker(1, true))
        });
        assert!(app.requested_profiles.is_empty());
        let Some(Overlay::Profiles(p)) = &app.shell.overlay else {
            panic!("profile dialog retained")
        };
        assert_eq!(p.requested, ["work"]);
        for _ in 0..10 {
            key(&mut app, KeyCode::Down, size.0, size.1);
        }
        assert!(matches!(&app.shell.overlay,Some(Overlay::Profiles(p)) if p.selected<2));
        key(&mut app, KeyCode::F(3), size.0, size.1);
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal.draw(|f| views::draw(f, &app)).unwrap();
        assert!(!terminal.backend().cursor_visible());
        assert!(key(&mut app, KeyCode::Enter, size.0, size.1).is_none());
        key(&mut app, KeyCode::Esc, size.0, size.1);
        assert!(matches!(
            key(&mut app, KeyCode::Enter, size.0, size.1),
            Some(Task::Profiles { apply: true, .. })
        ));
    }
}
#[test]
fn direct_field_submission_is_preview_only_and_busy_navigation_explains_joined_wait() {
    let mut incomplete = App::new(false);
    incomplete.shell.overlay = Some(Overlay::Form(Form::Adopt));
    incomplete.shell.adoption.target = "~/.ordinary".into();
    assert!(key(&mut incomplete, KeyCode::Enter, 48, 14).is_none());
    assert_eq!(incomplete.shell.adoption.focused, 1);
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Init));
    super::tests::focus_form(&mut app);
    app.shell.init_draft = "/synthetic/new-repo".into();
    let f = Fixture::new();
    app.git = Ok(f.bin.join("fake-provider"));
    assert!(matches!(
        key(&mut app, KeyCode::Enter, 80, 24),
        Some(Task::Init { .. })
    ));
    assert!(app.preview.is_none());
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Check));
    super::tests::focus_form(&mut app);
    app.shell.manifest_draft = "/synthetic/dort.toml".into();
    assert!(matches!(
        key(&mut app, KeyCode::Enter, 80, 24),
        Some(Task::Check { .. })
    ));
    app.shell.overlay = None;
    app.start(true);
    render(&app, 110, 30);
    let (column, row) = super::tests::target_position(&app, 110, 30, |target| {
        matches!(target, Target::NavigationBlocked)
    });
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        110,
        30,
    );
    assert!(app.busy && app.interaction_notice.is_some());
    key(&mut app, KeyCode::Char('q'), 80, 24);
    assert!(app.busy && app.exit_requested && !app.quit);
}
