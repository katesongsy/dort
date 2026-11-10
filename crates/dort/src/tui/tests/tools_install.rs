use super::{
    actions::{self, Task},
    app::{App, Tab},
    tests::render,
    tools_install::Mode,
    views,
};
use crate::services::{
    reconciliation::{self, Snapshot},
    tools,
};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
    tools::inspection::Query,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::Color,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
    bin: PathBuf,
    executable: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("lookup");
        for p in [&home, &repo, &bin] {
            fs::create_dir(p).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='mise',package='demo',version='1.2.3'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n").unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        drop(lock);
        let executable = root.join("mise");
        fs::write(&executable, "#!/bin/sh\nif [ \"$1\" = ls ]; then\nprintf Q >> \"$HOME/calls\"\nif [ -f \"$HOME/installed\" ]; then printf '{\"demo\":[{\"version\":\"1.2.3\",\"installed\":true}]}'; else printf '{}'; fi\nexit 0\nfi\nprintf I >> \"$HOME/calls\"\nprintf PRIVATE_OUTPUT; printf PRIVATE_ERROR >&2\nprintf installed > \"$HOME/installed\"\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
            bin,
            executable,
        }
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
    fn app(&self, dry: bool) -> App {
        let snapshot = Snapshot {
            id: "s1".into(),
            report: std::sync::Arc::new(
                tools::Request {
                    query: Query::MiseInstalled,
                    executable: self.executable.clone(),
                    search_dirs: vec![self.bin.clone()],
                    manifest: None,
                    runtime_tool: None,
                    npm_cli: None,
                    prefix: None,
                }
                .prepare(&self.host)
                .unwrap()
                .approve()
                .inspect(),
            ),
        };
        let mut app = App::new(dry);
        app.tab = Tab::Tools;
        app.repository = Some(self.repo.clone());
        app.tools_view.snapshots = vec![snapshot];
        let report = reconciliation::Request {
            manifest: self.repo.join("dort.toml"),
            tool_search_dirs: vec![self.bin.clone()],
            profiles: vec![],
        }
        .prepare(&self.host)
        .unwrap()
        .reconcile(
            &app.tools_view.snapshots,
            [("demo".into(), "s1".into())].into(),
        )
        .unwrap();
        app.tools_view.reconciliation.completed(Ok(report));
        app.tools_view.recon_open = true;
        app
    }
    fn run(&self, app: &mut App, task: Task) {
        let writing = matches!(task, Task::ToolInstall(_) | Task::ToolClose(_));
        app.start(writing);
        let result =
            actions::perform_scoped(task, &self.host, Some(self.repo.clone()), &[], |_| Ok(()))
                .map_err(|e| actions::diagnostic(&e));
        assert!(app.complete(result).is_none());
    }
    fn review(&self, app: &mut App) {
        let task = key(app, KeyCode::Char('i')).unwrap();
        assert!(matches!(task, Task::ToolInstallPrepare(_)));
        self.run(app, task);
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
#[test]
fn one_activation_consumes_the_real_install_preview_and_preserves_old_snapshots() {
    let f = Fixture::new();
    let mut app = f.app(false);
    assert!(render(&app, 80, 24).contains("Review exact mise install"));
    f.review(&mut app);
    assert_eq!(f.calls(), "Q");
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let text = render(&app, w, h);
        assert!(text.contains("Install exact version"), "{text}");
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|frame| views::draw(frame, &app)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|c| c.bg == Color::Cyan)
        );
    }
    key(&mut app, KeyCode::Char('d'));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(f.calls(), "Q");
    key(&mut app, KeyCode::Char('d'));
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolInstall(_)));
    assert!(app.tools_view.installation.preview.is_none());
    f.run(&mut app, task);
    assert_eq!(f.calls(), "QQIQ");
    assert!(app.tools_view.reconciliation.result.is_none());
    assert!(!app.tools_view.recon_open);
    assert!(app.receipt.is_none());
    assert_eq!(app.tools_view.snapshots.len(), 1);
    let old = &app.tools_view.snapshots[0].report.provider.observation;
    assert!(
        matches!(old,dort_core::tools::inspection::Observation::Complete{installations} if installations.is_empty())
    );
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(!render(&app, 80, 24).contains("PRIVATE"));
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.run(&mut app, task);
    assert!(render(&app, 80, 24).contains("Observed outcome"));
}
#[test]
fn dry_run_shrink_navigation_and_late_preparation_cannot_approve_installation() {
    for dry in [false, true] {
        let f = Fixture::new();
        let mut app = f.app(dry);
        f.review(&mut app);
        if dry {
            assert!(key(&mut app, KeyCode::Enter).is_none());
        }
        app.resize(47, 13);
        assert!(app.tools_view.installation.preview.is_none());
        app.resize(80, 24);
        assert!(key(&mut app, KeyCode::Enter).is_none());
        assert_eq!(f.calls(), "Q");
    }
    let f = Fixture::new();
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    app.go_tab(Tab::Git);
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(f.calls(), "Q");
}
#[test]
fn malformed_attempt_has_retained_fallback_form_and_one_fresh_closure_activation() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let attempt = root.join("a".repeat(64));
    fs::create_dir(&attempt).unwrap();
    fs::set_permissions(&attempt, fs::Permissions::from_mode(0o700)).unwrap();
    let task = app.tools_view.open_attempts();
    f.run(&mut app, task);
    assert!(render(&app, 48, 14).contains("UNRESOLVED"));
    assert!(key(&mut app, KeyCode::Char('c')).is_none());
    assert!(app.tools_view.installation.mode == Mode::CloseInputs);
    assert_eq!(
        app.tools_view.installation.fields[0],
        f.executable.display().to_string()
    );
    for size in [(48, 14), (80, 24), (140, 40)] {
        for focus in 0..3 {
            app.tools_view.installation.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
            terminal.draw(|frame| views::draw(frame, &app)).unwrap();
            if focus < 2 {
                let cursor = terminal.get_cursor_position().unwrap();
                assert!(cursor.x < size.0 && cursor.y < size.1 - 2);
            }
        }
    }
    app.tools_view.installation.focus = 2;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(f.calls(), "Q");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(f.calls(), "QQ");
    assert!(!app.tools_view.installation.attempts[0].unresolved);
    assert!(app.tools_view.installation.attempts[0].explicitly_closed);
}

#[test]
fn recomputing_with_old_snapshots_cannot_skip_explicit_fresh_inventory_after_installation() {
    let f = Fixture::new();
    let mut app = f.app(false);
    f.review(&mut app);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    key(&mut app, KeyCode::Esc);
    let report = reconciliation::Request {
        manifest: f.repo.join("dort.toml"),
        tool_search_dirs: vec![f.bin.clone()],
        profiles: vec![],
    }
    .prepare(&f.host)
    .unwrap()
    .reconcile(
        &app.tools_view.snapshots,
        [("demo".into(), "s1".into())].into(),
    )
    .unwrap();
    app.tools_view.reconciliation.completed(Ok(report));
    app.tools_view.recon_open = true;
    assert!(render(&app, 48, 14).contains("Collect fresh inventory"));
    assert!(key(&mut app, KeyCode::Char('i')).is_none());
    assert_eq!(f.calls(), "QQIQ");
    assert!(matches!(
        key(&mut app, KeyCode::Char('c')),
        Some(Task::ToolsDetect(_))
    ));
    assert_eq!(f.calls(), "QQIQ");
}

#[test]
fn stale_approval_returns_owned_failure_without_replay_or_file_recovery_receipt() {
    let f = Fixture::new();
    let mut app = f.app(false);
    f.review(&mut app);
    let manifest = f.repo.join("dort.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("# drift\n");
    fs::write(&manifest, text).unwrap();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(f.calls(), "Q");
    assert!(app.tools_view.installation.error.is_some());
    assert!(app.error.is_none() && app.receipt.is_none());
    assert!(app.tools_view.installation.preview.is_none());
    assert!(key(&mut app, KeyCode::Char('r')).is_none());
    assert!(app.tools_view.reconciliation.result.is_none());
}

#[test]
fn unavailable_attempt_storage_is_not_a_healthy_empty_list() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let mut app = f.app(false);
    symlink(&f.bin, f.host.state_dir.join("tools-attempts")).unwrap();
    let task = app.tools_view.open_attempts();
    f.run(&mut app, task);
    let screen = render(&app, 80, 24);
    assert!(screen.contains("Attempts unavailable"));
    assert!(!screen.contains("No installation attempts"));
    assert_eq!(f.calls(), "Q");
}
