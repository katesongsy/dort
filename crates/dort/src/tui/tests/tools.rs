//! Declaration-led Tools: stable navigation and process-free, necessary settings.
use super::{
    actions::{self, Task},
    app::{App, Tab},
    shell::Action,
    tests::render,
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
pub(super) fn run(f: &Fixture, app: &mut App, task: Task) {
    app.retry = task.retry_intent();
    app.start(matches!(
        task,
        Task::ToolsInstallMissing(_) | Task::Execute(_)
    ));
    let output = actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(()));
    if let Some(refresh) = app.complete(output.map_err(|e| actions::diagnostic(&e))) {
        run(f, app, refresh);
    }
}
pub(super) fn app(f: &Fixture, dry: bool) -> App {
    let mut app = App::new(dry);
    app.repository = Some(f.repo.clone());
    run(f, &mut app, Task::Refresh);
    app.activate(Action::Destination(Tab::Tools));
    app
}
pub(super) fn check(f: &Fixture, app: &mut App) {
    let Some(Task::ToolsCheckPrepare(mut input)) = key(app, KeyCode::Char('C')) else {
        panic!("Check action")
    };
    input.search_dirs = vec![f.bin.clone()];
    run(f, app, Task::ToolsCheckPrepare(input));
    assert!(app.tools_view.declared.check_preview.is_some());
    for _ in 0..3 {
        assert!(key(app, KeyCode::Enter).is_none());
    }
    let task = key(app, KeyCode::Char('y')).unwrap();
    assert!(matches!(task, Task::ToolsCheck(_)));
    run(f, app, task);
}
pub(super) fn settings(f: &Fixture, app: &mut App) {
    if !app.tools_view.declared_details {
        key(app, KeyCode::Enter);
    }
    let Some(Task::ToolsSettingsDetect { tool, mut inputs }) = key(app, KeyCode::Char('s')) else {
        panic!("Selected tool settings")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    run(f, app, Task::ToolsSettingsDetect { tool, inputs });
    assert!(app.tools_view.settings_open && app.tools_view.settings.request.is_some());
}
#[test]
fn list_enter_is_always_details_before_and_after_checks_and_installs() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    for stage in 0..3 {
        assert!(!app.tools_view.declared_details);
        assert!(key(&mut app, KeyCode::Enter).is_none());
        assert!(app.tools_view.declared_details);
        assert!(key(&mut app, KeyCode::Enter).is_none());
        key(&mut app, KeyCode::Esc);
        assert!(!app.tools_view.declared_details);
        if stage == 0 {
            check(&f, &mut app);
        }
        if stage == 1 {
            let task = key(&mut app, KeyCode::Char('i')).unwrap();
            run(&f, &mut app, task);
            assert!(app.tools_view.declared.install_preview.is_some());
            let task = key(&mut app, KeyCode::Char('y')).unwrap();
            run(&f, &mut app, task);
            key(&mut app, KeyCode::Enter);
        }
    }
    assert_eq!(f.calls().chars().filter(|c| *c == 'A').count(), 1);
}
#[test]
fn settings_detection_and_save_are_process_free_and_only_accepted_values_affect_checks() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    settings(&f, &mut app);
    assert_eq!(app.tools_view.settings.count(), 2);
    assert_eq!(
        app.tools_view.settings.fields[0],
        f.bin
            .join("brew")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert!(app.tools_view.inspection_inputs().overrides.is_empty());
    app.tools_view.settings.fields[0] = "relative-program".into();
    assert!(app.tools_view.inspection_inputs().overrides.is_empty());
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(())).is_err());
    assert!(f.calls().is_empty());
    app.tools_view.settings.fields[0] = f
        .bin
        .join("brew")
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run(&f, &mut app, task);
    assert!(!app.tools_view.settings_open && app.tools_view.declared_details);
    assert_eq!(app.tools_view.inspection_inputs().overrides.len(), 1);
    assert!(f.calls().is_empty());
    check(&f, &mut app);
    assert_eq!(f.calls(), "Q");
    settings(&f, &mut app);
    app.tools_view.settings.fields[0] = String::new();
    assert_eq!(
        app.tools_view.inspection_inputs().overrides[0].executable,
        f.bin.join("brew").canonicalize().unwrap()
    );
}
#[test]
fn settings_and_declaration_drafts_survive_help_but_navigation_drops_reviews() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    settings(&f, &mut app);
    key(&mut app, KeyCode::Tab);
    for c in "q?;oC3".chars() {
        assert!(key(&mut app, KeyCode::Char(c)).is_none());
    }
    let draft = app.tools_view.settings.fields.clone();
    key(&mut app, KeyCode::F(1)); // Help retains the draft.
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.tools_view.settings.fields, draft);
    assert!(!app.quit && app.tools_view.settings_open);
    key(&mut app, KeyCode::Esc); // Stop editing.
    assert!(app.tools_view.settings_open);
    key(&mut app, KeyCode::Esc); // Tool details.
    assert!(app.tools_view.declared_details && !app.tools_view.settings_open);
    settings(&f, &mut app);
    assert_eq!(app.tools_view.settings.fields, draft);
    assert!(f.calls().is_empty());
    app.activate(Action::Destination(Tab::Files));
    assert!(!app.tools_view.settings_open);
}
#[test]
fn accepted_settings_invalidate_old_results_and_late_detection_never_reopens_pages() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    check(&f, &mut app);
    settings(&f, &mut app);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run(&f, &mut app, task);
    assert!(app.tools_view.declared.checked.is_none());
    assert_eq!(f.calls(), "Q");
    let Some(Task::ToolsSettingsDetect { tool, mut inputs }) = key(&mut app, KeyCode::Char('s'))
    else {
        panic!("Settings")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    app.activate(Action::Destination(Tab::Git));
    run(&f, &mut app, Task::ToolsSettingsDetect { tool, inputs });
    assert!(app.tab == Tab::Git);
    assert!(!app.tools_view.settings_open && f.calls() == "Q");
}
#[test]
fn needed_settings_are_selected_tool_configuration_not_internal_binding_forms() {
    let f = Fixture::new();
    let path = f.repo.join("dort.toml");
    std::fs::write(&path, "schema=1\n[[tools]]\nid='runtime'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n[[tools]]\nid='cli'\nbinary='cli'\n[tools.install]\nprovider='npm'\npackage='cli'\nversion='1.2.3'\nruntime_tool='runtime'\n").unwrap();
    let mut app = app(&f, false);
    app.tools_view.declared_selected = app
        .data
        .as_ref()
        .unwrap()
        .tools
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .iter()
        .position(|t| t.id == "cli")
        .unwrap();
    settings(&f, &mut app);
    assert_eq!(app.tools_view.settings.count(), 4);
    let request = app.tools_view.settings.request.as_ref().unwrap();
    assert_eq!(request.runtime_tool.as_deref(), Some("runtime"));
    assert_eq!(request.manifest.as_ref(), Some(&path));
    let text = render(&app, 140, 40);
    for internal in [
        "Snapshot ID",
        "Runtime tool ID",
        "Manifest path",
        "Provider choice",
        "Advanced",
    ] {
        assert!(!text.contains(internal), "{text}");
    }
    assert!(text.contains("Node program") && text.contains("npm CLI") && text.contains("prefix"));
    assert!(f.calls().is_empty());
}
#[test]
fn tool_keys_have_no_snapshot_pages_numbered_tasks_or_f3_mode_switches() {
    let f = Fixture::new();
    let mut app = app(&f, true);
    for code in [
        KeyCode::Char('4'),
        KeyCode::Char('5'),
        KeyCode::F(3),
        KeyCode::F(4),
    ] {
        assert!(key(&mut app, code).is_none());
        assert!(app.tools_view.landing() && !app.tools_view.declared_details);
    }
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let text = render(&app, w, h);
        assert!(
            !text.contains("Snapshots") && !text.contains("Advanced") && !text.contains("3 Check")
        );
        assert!(text.lines().last().unwrap().contains("F1 Help"));
    }
    assert!(f.calls().is_empty());
}
