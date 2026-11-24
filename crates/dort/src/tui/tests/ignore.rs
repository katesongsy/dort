use super::{
    actions::{self, Output, Task},
    app::{App, Tab},
    shell::{self, Overlay},
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
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
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
    assert_eq!(query, "Ignore selected");
    app.activate(shell::Action::IgnoreSelected)
        .expect("primary action task")
}
fn approve(app: &mut App) -> Task {
    key(app, KeyCode::Char('y')).expect("one explicit preview approval")
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
    let task = app.activate(shell::Action::ManageIgnored).unwrap();
    f.finish(&mut app, task);
    assert!(matches!(app.shell.overlay, Some(Overlay::Ignored { .. })));
    assert!(super::tests::has_action(&app, "Unignore"));
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
fn visible_ignore_controls_mouse_and_keyboard_work_at_all_supported_sizes() {
    use super::{input::Target, shell::Action};
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let f = Fixture::new();
        let mut app = f.app(false);
        app.resize(w, h);
        let screen = render(&app, w, h);
        assert!(
            screen.contains("F2 More") && screen.contains("F1 Help"),
            "{screen}"
        );
        let (column, row) =
            super::tests::target_position(&app, w, h, |target| matches!(target, Target::More));
        assert!(
            app.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE
                },
                w,
                h
            )
            .is_none()
        );
        render(&app, w, h);
        let index = app
            .more
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|item| matches!(item.action.target, Target::Action(Action::IgnoreSelected)))
            .unwrap();
        let (column, row) = super::tests::target_position(
            &app,
            w,
            h,
            |target| matches!(target,Target::MoreRow(i) if i == index),
        );
        assert!(
            app.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE
                },
                w,
                h
            )
            .is_none()
        );
        render(&app, w, h);
        let (column, row) = super::tests::target_position(
            &app,
            w,
            h,
            |target| matches!(target,Target::Key(event) if event.code == KeyCode::Enter),
        );
        let task = app
            .mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
                w,
                h,
            )
            .unwrap();
        f.finish(&mut app, task);
        key(&mut app, KeyCode::Esc);
        let task = app
            .key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE), w, h)
            .unwrap();
        f.finish(&mut app, task);
        let task = approve(&mut app);
        let task = f.finish(&mut app, task).unwrap();
        f.finish(&mut app, task);
        app.resize(w, h);
        render(&app, w, h);
        let (column, row) = super::tests::target_position(&app, w, h, |target| {
            matches!(target, Target::LocalDestination(2, 2))
        });
        let task = app
            .mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
                w,
                h,
            )
            .unwrap();
        f.finish(&mut app, task);
        assert!(matches!(app.shell.overlay, Some(Overlay::Ignored { .. })));
        let screen = render(&app, w, h);
        let footer = screen.lines().last().unwrap();
        assert!(
            footer.contains("Enter")
                && footer.contains("Unignore")
                && super::tests::has_action(&app, "Unignore"),
            "contextual footer at {w}"
        );
        assert!(!footer.contains("Actions") && !footer.contains("Menu"));
        assert!(app.preview.is_none() && app.confirmation.is_none());
        key(&mut app, KeyCode::Esc);
        let task = app
            .key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), w, h)
            .unwrap();
        f.finish(&mut app, task);
        let task = key(&mut app, KeyCode::Enter).unwrap();
        f.finish(&mut app, task);
        let task = approve(&mut app);
        let task = f.finish(&mut app, task).unwrap();
        f.finish(&mut app, task);
        assert_eq!(Rules::load(&f.host).unwrap().paths().count(), 0);
        assert_eq!(
            fs::read_to_string(f.host.home.join(".tool/sub/config")).unwrap(),
            "ordinary fixture"
        );
    }
}
#[test]
fn help_retains_real_preview_confirmation_and_shrink_still_clears_approval() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.finish(&mut app, task);
    let original = app.preview.as_ref().unwrap().text.clone();
    key(&mut app, KeyCode::F(1));
    assert!(app.help.is_some() && app.preview.is_some());
    for code in [
        KeyCode::Enter,
        KeyCode::Char('y'),
        KeyCode::Char('e'),
        KeyCode::Char('s'),
        KeyCode::Enter,
    ] {
        assert!(key(&mut app, code).is_none());
    }
    assert!(app.confirmation.is_none());
    assert!(!render(&app, 140, 40).contains(".tool"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.preview.as_ref().unwrap().text, original);
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::F(1));
    for code in [KeyCode::Char('s'), KeyCode::Enter] {
        assert!(key(&mut app, code).is_none());
    }
    assert!(app.preview.is_some() && app.confirmation.is_none());
    app.resize(47, 13);
    assert!(app.confirmation.is_none() && app.help.is_some() && app.preview.is_some());
    app.resize(80, 24);
    key(&mut app, KeyCode::Esc);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let task = approve(&mut app); // fresh input, real opaque core preview retained
    let refresh = f.finish(&mut app, task).unwrap();
    f.finish(&mut app, refresh);
    assert_eq!(Rules::load(&f.host).unwrap().paths().count(), 1);
}
#[test]
fn plain_section_navigation_drops_real_preview_ownership_without_any_write() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.finish(&mut app, task);
    assert!(app.preview.is_some());
    app.key(
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
        80,
        24,
    );
    assert!(app.preview.is_none() && app.confirmation.is_none());
    assert!(app.tab == super::app::Tab::Overview);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
}

#[test]
fn ignore_preview_cancel_dry_run_and_small_layout_cannot_write() {
    let f = Fixture::new();
    let mut app = f.app(true);
    let task = action(&mut app, "Ignore selected");
    f.finish(&mut app, task);
    assert!(key(&mut app, KeyCode::Char('y')).is_none());
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let mut app = f.app(false);
    let task = action(&mut app, "Ignore selected");
    f.finish(&mut app, task);
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            47,
            13
        )
        .is_none()
    );
    assert!(app.confirmation.is_none());
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.preview.is_some() && app.confirmation.is_none());
    assert!(!f.host.config_dir.exists());
    key(&mut app, KeyCode::Esc);
    assert!(!f.host.config_dir.exists() && !f.host.state_dir.exists());
    let output = actions::perform(Task::IgnoredPaths, &f.host, None, |_| Ok(())).unwrap();
    assert!(matches!(output, Output::IgnoredPaths(paths) if paths.is_empty()));
}
