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
    app.open_menu(2, 100);
    key(&mut app, KeyCode::End);
    let task = key(&mut app, KeyCode::Enter).unwrap();
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
fn visible_ignore_controls_mouse_and_keyboard_work_at_all_supported_sizes() {
    use super::{input::Target, shell::Action};
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let f = Fixture::new();
        let mut app = f.app(false);
        app.resize(w, h);
        let screen = render(&app, w, h);
        assert!(screen.contains("Ignore selected") && screen.contains("F10"));
        assert!(screen.contains("Actions") && screen.contains("F1") && screen.contains("Help"));
        let mut found = Vec::new();
        for action in [Action::IgnoreSelected] {
            let mut point = None;
            for y in 0..h {
                for x in 0..w {
                    if matches!(app.hits.target(x,y), Some(Target::Action(a)) if a == action) {
                        point = Some((x, y));
                    }
                }
            }
            found.push(point.expect("visible typed control"));
        }
        let (column, row) = found[0];
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
            .key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE), w, h)
            .unwrap();
        f.finish(&mut app, task);
        let task = approve(&mut app);
        let task = f.finish(&mut app, task).unwrap();
        f.finish(&mut app, task);
        app.resize(w, h);
        app.open_menu(2, w);
        app.navigation.menu.as_mut().unwrap().expanded = true;
        assert!(render(&app, w, h).contains("Ignored paths"));
        let (column, row) = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .find(|&(x, y)| matches!(app.hits.target(x, y), Some(Target::MenuItem(2))))
            .expect("visible Ignored paths menu item");
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
            footer.contains("Enter") && footer.contains("Unignore"),
            "contextual footer at {w}"
        );
        assert!(!footer.contains("Actions") && !footer.contains("Menu"));
        assert!(app.preview.is_none() && app.confirmation.is_none());
        key(&mut app, KeyCode::Esc);
        let task = app
            .key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::NONE), w, h)
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
    let task = key(&mut app, KeyCode::Char('I')).unwrap();
    f.finish(&mut app, task);
    let original = app.preview.as_ref().unwrap().text.clone();
    key(&mut app, KeyCode::F(10));
    assert!(app.navigation.menu.is_none() && app.preview.is_some());
    key(&mut app, KeyCode::F(1));
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
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('y'));
    key(&mut app, KeyCode::Char('e'));
    key(&mut app, KeyCode::F(10));
    assert!(app.navigation.menu.is_none());
    assert_eq!(app.confirmation.as_ref().unwrap().input, "ye");
    key(&mut app, KeyCode::F(1));
    for code in [KeyCode::Char('s'), KeyCode::Enter] {
        assert!(key(&mut app, code).is_none());
    }
    assert_eq!(app.confirmation.as_ref().unwrap().input, "ye");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.confirmation.as_ref().unwrap().input, "ye");
    key(&mut app, KeyCode::F(1));
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
