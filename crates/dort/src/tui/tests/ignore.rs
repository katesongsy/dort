use super::{
    actions::{self, Output, Task},
    app::{App, Tab},
    shell::Overlay,
    tests::render,
};
use dort_core::{
    context::HostContext,
    discovery_ignore::Rules,
    platform::{Architecture, Platform},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fs;

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap().join("home");
        fs::create_dir_all(home.join(".tool/sub")).unwrap();
        fs::write(home.join(".tool/sub/config"), "ordinary fixture").unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        Self { _temp: temp, host }
    }
    fn finish(&self, app: &mut App, task: Task) -> Option<Task> {
        let output = actions::perform(task, &self.host, None, |_| Ok(())).unwrap();
        app.complete(Ok(output))
    }
    fn app(&self, dry: bool) -> App {
        let mut app = App::new(dry);
        app.tab = Tab::Files;
        app.discovering = true;
        self.finish(&mut app, Task::Discover);
        app
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 100, 30)
}
fn action(app: &mut App, query: &str) -> Task {
    key(app, KeyCode::Char(';'));
    for c in query.chars() {
        key(app, KeyCode::Char(c));
    }
    key(app, KeyCode::Enter).expect("action task")
}
fn approve(app: &mut App) -> Task {
    key(app, KeyCode::Enter);
    assert!(app.confirmation.is_some());
    for c in "yes".chars() {
        key(app, KeyCode::Char(c));
    }
    key(app, KeyCode::Enter).expect("approved task")
}
#[test]
fn ignore_folder_confirm_refresh_manage_and_unignore_are_complete_tui_workflows() {
    let f = Fixture::new();
    let mut app = f.app(false);
    assert!(app.tab == Tab::Files && app.discovering);
    let task = action(&mut app, "Ignore selected");
    f.finish(&mut app, task);
    assert!(!f.host.state_dir.exists());
    assert!(app.preview.as_ref().unwrap().text.contains(".tool"));
    assert!(render(&app, 100, 30).contains("Ignore discovery path"));
    let task = approve(&mut app);
    let refresh = f.finish(&mut app, task).unwrap();
    assert!(matches!(refresh, Task::Discover));
    f.finish(&mut app, refresh);
    assert_eq!(Rules::load(&f.host).unwrap().paths().count(), 1);
    assert!(
        app.discovery
            .as_ref()
            .unwrap()
            .report
            .candidates
            .iter()
            .all(|c| !c.entry.path.starts_with(f.host.home.join(".tool")))
    );
    let task = action(&mut app, "Manage ignored");
    f.finish(&mut app, task);
    assert!(matches!(app.shell.overlay, Some(Overlay::Ignored { .. })));
    assert!(render(&app, 48, 14).contains("Unignore"));
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.finish(&mut app, task);
    assert!(
        app.preview
            .as_ref()
            .unwrap()
            .text
            .contains("Unignore discovery path")
    );
    let task = approve(&mut app);
    let refresh = f.finish(&mut app, task).unwrap();
    f.finish(&mut app, refresh);
    assert_eq!(Rules::load(&f.host).unwrap().paths().count(), 0);
    assert!(
        app.discovery
            .as_ref()
            .unwrap()
            .report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with("sub/config"))
    );
    assert_eq!(
        fs::read_to_string(f.host.home.join(".tool/sub/config")).unwrap(),
        "ordinary fixture"
    );
    // Shared service is also the CLI's discovery path.
    let local = dort_core::local::LocalConfig::default();
    assert!(
        crate::services::discover(&f.host, &local, None)
            .unwrap()
            .report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with("sub/config"))
    );
}
#[test]
fn ignore_preview_cancel_dry_run_and_small_layout_cannot_write() {
    let f = Fixture::new();
    let mut app = f.app(true);
    let task = action(&mut app, "Ignore selected");
    f.finish(&mut app, task);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let mut app = f.app(false);
    let task = action(&mut app, "Ignore selected");
    f.finish(&mut app, task);
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('y'));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_some());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let output = actions::perform(Task::IgnoredPaths, &f.host, None, |_| Ok(())).unwrap();
    assert!(matches!(output, Output::IgnoredPaths(paths) if paths.is_empty()));
}
