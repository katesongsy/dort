use super::{
    actions::{self, Task},
    app::{App, Tab},
    tests::render,
    views,
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Modifier},
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    executable: PathBuf,
    host: HostContext,
}
impl Fixture {
    fn new(response: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let executable = root.join("provider");
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf ran >> \"$HOME/calls\"\n{response}\n"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let host = HostContext::new(
            home.clone(),
            None,
            None,
            Platform::Ubuntu,
            Architecture::X86_64,
        )
        .unwrap();
        Self {
            _temp: temp,
            home,
            executable,
            host,
        }
    }
    fn app(&self, dry: bool) -> App {
        let mut app = App::new(dry);
        app.tab = Tab::Tools;
        app.tools_view.fields[0] = self.executable.to_str().unwrap().into();
        app.tools_view.fields[1] = self.home.join("trusted-bin").to_str().unwrap().into();
        app
    }
    fn run(&self, app: &mut App, task: Task) {
        app.start(false);
        let out = actions::perform_scoped(task, &self.host, None, &[], |_| Ok(()))
            .map_err(|e| actions::diagnostic(&e));
        assert!(app.complete(out).is_none());
    }
    fn review(&self, app: &mut App) {
        let task = key(app, KeyCode::Char('r')).unwrap();
        assert!(matches!(task, Task::ToolsPrepare(_)));
        self.run(app, task);
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}

#[test]
fn entry_configuration_and_preview_never_query_and_printable_shortcuts_remain_text() {
    let f = Fixture::new("exit 99");
    let mut app = f.app(false);
    assert!(render(&app, 48, 14).contains("No provider has been queried"));
    key(&mut app, KeyCode::Char('c'));
    app.tools_view.focus = 1;
    type_text(&mut app, "q?;f");
    assert!(!app.quit && app.help.is_none() && app.shell.overlay.is_none());
    assert!(app.tools_view.fields[0].ends_with("q?;f"));
    app.tools_view.fields[0] = f.executable.to_str().unwrap().into();
    app.tools_view.focus = 7;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.preview.is_some());
    assert!(!f.home.join("calls").exists());
    assert!(app.preview.is_none() && app.confirmation.is_none());
    let screen = render(&app, 80, 24);
    assert!(screen.contains("Risk: config/plugins"));
    assert!(screen.contains("Run query"));
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.configuring);
    assert!(!f.home.join("calls").exists());
}

#[test]
fn explicit_activation_executes_once_searches_metadata_and_retry_needs_review() {
    let f = Fixture::new(
        r#"printf '{"formulae":[{"full_name":"ripgrep","installed":[{"version":"14.1.0"}]},{"full_name":"node","installed":[{"version":"22.0.0"}]}]}'"#,
    );
    let mut app = f.app(false);
    f.review(&mut app);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolsInspect(_)));
    assert!(app.tools_view.preview.is_none());
    f.run(&mut app, task);
    assert_eq!(fs::read_to_string(f.home.join("calls")).unwrap(), "ran");
    assert!(render(&app, 80, 24).contains("2 shown / 2 provider claims"));
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "RIPGREP");
    key(&mut app, KeyCode::Enter);
    assert!(render(&app, 48, 14).contains("1 shown / 2 provider claims"));
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.details);
    assert!(render(&app, 80, 24).contains("Package: ripgrep"));
    key(&mut app, KeyCode::Esc);
    f.review(&mut app);
    assert!(app.tools_view.preview.is_some());
    assert_eq!(fs::read_to_string(f.home.join("calls")).unwrap(), "ran");
}

#[test]
fn resize_preview_only_and_scope_changes_cannot_reuse_inspection_confirmation() {
    let f = Fixture::new("exit 99");
    let mut app = f.app(false);
    f.review(&mut app);
    type_text(&mut app, "inspect");
    app.resize(47, 13);
    assert!(app.tools_view.preview.is_none() || app.dry_run);
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(app.tools_view.preview.is_none());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Left);
    assert!(app.tab == Tab::Git && app.tools_view.preview.is_none());
    assert!(!f.home.join("calls").exists());
    let mut app = f.app(true);
    f.review(&mut app);
    type_text(&mut app, "inspect");
    assert!(app.tools_view.preview.is_none() || app.dry_run);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(render(&app, 48, 14).contains("execution disabled"));
    assert!(!f.home.join("calls").exists());
}

#[test]
fn preparation_finishing_after_navigation_discards_preview_and_approval() {
    let f = Fixture::new("exit 99");
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    app.start(false);
    key(&mut app, KeyCode::Left);
    assert!(app.tab == Tab::Git);
    let result = actions::perform_scoped(task, &f.host, None, &[], |_| Ok(()))
        .map_err(|e| actions::diagnostic(&e));
    app.complete(result);
    assert!(app.tools_view.preview.is_none());
    assert!(!f.home.join("calls").exists());
}

#[test]
fn unavailable_is_not_empty_and_never_displays_raw_provider_payload() {
    let f = Fixture::new("printf PRIVATE; printf PRIVATE >&2; exit 7");
    let mut app = f.app(false);
    f.review(&mut app);
    type_text(&mut app, "inspect");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    let text = render(&app, 80, 24);
    assert!(text.contains("UNAVAILABLE"));
    assert!(text.contains("Not a healthy empty"));
    assert!(!text.contains("PRIVATE"));
    assert!(app.tools_view.preview.is_none());
    f.review(&mut app);
    assert!(app.tools_view.preview.is_some());
    assert_eq!(fs::read_to_string(f.home.join("calls")).unwrap(), "ran");
}

#[test]
fn npm_form_exposes_full_explicit_binding_without_repository_or_launcher_discovery() {
    let f = Fixture::new(r#"printf '{"dependencies":{"@scope/pkg":{"version":"1.2.3"}}}'"#);
    let manifest = f.home.parent().unwrap().join("dort.toml");
    fs::write(&manifest, include_str!("../../../examples/dort.toml")).unwrap();
    let mut app = f.app(false);
    key(&mut app, KeyCode::Char('c'));
    for _ in 0..3 {
        key(&mut app, KeyCode::Char(' '));
    }
    assert_eq!(app.tools_view.query, 3);
    app.tools_view.fields[2] = manifest.to_str().unwrap().into();
    app.tools_view.fields[3] = "node".into();
    app.tools_view.fields[4] = f.home.join("npm-cli.js").to_str().unwrap().into();
    app.tools_view.fields[5] = f.home.join("prefix").to_str().unwrap().into();
    app.tools_view.focus = 7;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(
        app.tools_view.scope.contains("npm_binding")
            && app.tools_view.scope.contains("npm-cli.js")
            && app.tools_view.scope.contains("runtime_tool")
    );
    assert!(!f.home.join("calls").exists());
    type_text(&mut app, "inspect");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(
        app.tools_view
            .report
            .as_ref()
            .unwrap()
            .provider
            .description
            .npm_binding
            .as_ref()
            .unwrap()
            .runtime_tool,
        "node"
    );
    assert!(!f.home.join("prefix").exists());
}

#[test]
fn healthy_empty_observation_is_distinct_and_approved_query_is_not_a_write_receipt() {
    let f = Fixture::new(r#"printf '{"formulae":[]}'"#);
    let mut app = f.app(false);
    f.review(&mut app);
    type_text(&mut app, "inspect");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    let activity = super::task_surfaces::Activity::for_task(&task, &app);
    assert!(activity.provider_inspection);
    app.activity = Some(activity);
    app.start(false);
    let busy = render(&app, 80, 24);
    assert!(busy.contains("configuration/cache side effects"));
    key(&mut app, KeyCode::Char('q'));
    assert!(app.exit_requested && !app.quit);
    let result = actions::perform_scoped(task, &f.host, None, &[], |_| Ok(()))
        .map_err(|e| actions::diagnostic(&e));
    app.complete(result);
    assert!(app.quit && app.receipt.is_none());
    app.quit = false;
    assert!(render(&app, 80, 24).contains("no installed packages reported"));
    assert!(!render(&app, 80, 24).contains("UNAVAILABLE"));
}

#[test]
fn compact_wide_forms_keep_focus_and_caret_visible_and_results_have_semantic_selection() {
    let f = Fixture::new(
        r#"printf '{"formulae":[{"full_name":"ripgrep","installed":[{"version":"14.1.0"}]}]}'"#,
    );
    let mut app = f.app(false);
    app.tools_view.configure();
    app.tools_view.query = 3;
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for focus in 0..9 {
            app.tools_view.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| views::draw(frame, &app)).unwrap();
            if focus < 7 {
                let cursor = terminal.get_cursor_position().unwrap();
                assert!(cursor.x < w && cursor.y >= 2 && cursor.y < h - 2);
                assert!(
                    terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .any(|c| c.fg == Color::Cyan)
                );
            } else {
                assert!(
                    terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .any(|c| c.bg == Color::Cyan
                            && c.modifier.contains(Modifier::BOLD | Modifier::REVERSED))
                );
            }
        }
    }
    app.tools_view.query = 0;
    app.tools_view.configuring = false;
    f.review(&mut app);
    type_text(&mut app, "inspect");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
    terminal.draw(|frame| views::draw(frame, &app)).unwrap();
    assert!(
        terminal.backend().buffer().content.iter().any(
            |c| c.bg == Color::Cyan && c.modifier.contains(Modifier::BOLD | Modifier::REVERSED)
        )
    );
}
