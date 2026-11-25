//! Declaration-led Tools: stable navigation and process-free, necessary settings.
use super::{
    actions::{self, Task},
    app::{App, Tab},
    shell::Action,
    tests::render,
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs, path::Path};

pub(super) fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
pub(super) fn run(f: &Fixture, app: &mut App, task: Task) {
    app.retry = task.retry_intent();
    app.start(matches!(
        task,
        Task::ToolsInstallMissing(_) | Task::Execute(_)
    ));
    let output = actions::perform_scoped(task, &f.host, app.repository.clone(), &[], |_| Ok(()));
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
    let task = super::tests::approve_ready(app, 80, 24).unwrap();
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
    assert!(app.tools_view.settings_open && app.tools_view.settings.editor.is_some());
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
            let task = super::tests::approve_ready(&mut app, 80, 24).unwrap();
            run(&f, &mut app, task);
            key(&mut app, KeyCode::Enter);
        }
    }
    assert_eq!(f.calls().chars().filter(|c| *c == 'A').count(), 1);
}
#[test]
fn saved_paths_survive_restart_and_are_scoped_to_one_manifest_and_tool() {
    let f = Fixture::new();
    f.program(
        "custom-brew",
        r#"printf Z >> "$HOME/calls"; printf '{"formulae":[]}'"#,
    );
    let custom = f.bin.join("custom-brew");
    let config = f.host.local_config_path();
    let before = fs::read(&config).unwrap();
    let manifest = fs::read(f.repo.join("dort.toml")).unwrap();
    let mut first = app(&f, false);
    settings(&f, &mut first);
    assert!(render(&first, 100, 30).contains("Save on this host"));
    first.tools_view.settings.fields[0] = "relative-program".into();
    let task = key(&mut first, KeyCode::Enter).unwrap();
    assert!(actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(())).is_err());
    assert_eq!(fs::read(&config).unwrap(), before);
    first.tools_view.settings.fields[0] = custom.display().to_string();
    let task = key(&mut first, KeyCode::Enter).unwrap();
    run(&f, &mut first, task);
    assert!(!first.tools_view.settings_open && first.tools_view.declared_details);
    assert_ne!(fs::read(&config).unwrap(), before);
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), manifest);
    assert!(f.calls().is_empty());

    let mut reopened = app(&f, false);
    settings(&f, &mut reopened);
    assert_eq!(
        reopened.tools_view.settings.fields[0],
        custom.display().to_string()
    );
    reopened.tools_view.back();
    check(&f, &mut reopened);
    assert_eq!(
        f.calls(),
        "QZ",
        "only the configured tool uses the custom manager"
    );

    let other = f.repo.parent().unwrap().join("other");
    fs::create_dir(&other).unwrap();
    fs::create_dir(other.join(".git")).unwrap();
    fs::write(other.join("dort.toml"), manifest).unwrap();
    reopened.repository = Some(other);
    run(&f, &mut reopened, Task::Refresh);
    check(&f, &mut reopened);
    assert_eq!(
        f.calls(),
        "QZQ",
        "the other repository uses only the default manager"
    );
}

#[test]
fn dry_run_validates_settings_without_writing_configuration_or_running_a_manager() {
    let f = Fixture::new();
    let config = f.host.local_config_path();
    let before = fs::read(&config).unwrap();
    let mut dry = app(&f, true);
    settings(&f, &mut dry);
    assert!(render(&dry, 100, 30).contains("Validate paths"));
    let task = key(&mut dry, KeyCode::Enter).unwrap();
    run(&f, &mut dry, task);
    assert!(dry.tools_view.settings_open);
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(f.calls().is_empty());
}

fn standalone(f: &Fixture, app: &mut App, path: &Path) -> Task {
    key(app, KeyCode::Char('m'));
    assert!(render(app, 100, 30).contains("Absolute manifest path"));
    key(app, KeyCode::Tab);
    for c in path.to_str().unwrap().chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
    let Some(Task::ToolsStandaloneCheckPrepare(mut input)) = key(app, KeyCode::Enter) else {
        panic!("Expected standalone check preparation");
    };
    input.detection.search_dirs = vec![f.bin.clone()];
    Task::ToolsStandaloneCheckPrepare(input)
}

#[test]
fn standalone_check_reviews_an_exact_file_and_cannot_feed_installs() {
    let f = Fixture::new();
    let path = f.repo.parent().unwrap().join("alternate.toml");
    fs::write(&path, "schema=1\n[[tools]]\nid='standalone-tool'\nbinary='a'\n[tools.install]\nprovider='brew'\npackage='a'\n").unwrap();
    let config = fs::read(f.host.local_config_path()).unwrap();
    let mut ui = app(&f, false);
    let task = standalone(&f, &mut ui, &path);
    run(&f, &mut ui, task);
    let preview = render(&ui, 120, 32);
    assert!(preview.contains("alternate.toml") && preview.contains("standalone-tool"));
    assert!(f.calls().is_empty());
    assert!(key(&mut ui, KeyCode::Enter).is_none());
    let task = super::tests::approve_ready(&mut ui, 80, 24).unwrap();
    run(&f, &mut ui, task);
    assert_eq!(f.calls(), "Q");
    assert_eq!(ui.repository.as_ref(), Some(&f.repo));
    assert_eq!(fs::read(f.host.local_config_path()).unwrap(), config);
    let result = render(&ui, 120, 32);
    assert!(result.contains("standalone-tool") && result.contains("Missing"));
    assert!(ui.tools_view.declared.checked.is_none());
    let checked = ui.tools_view.declared.standalone_result.as_ref().unwrap();
    assert!(checked.prepare_missing(&f.host).is_err());
    key(&mut ui, KeyCode::Enter);
    assert!(key(&mut ui, KeyCode::Char('i')).is_none());
    assert_eq!(f.calls(), "Q");
}

#[test]
fn standalone_check_rejects_relative_and_symlink_paths_without_running_a_manager() {
    let f = Fixture::new();
    let path = f.repo.parent().unwrap().join("alias.toml");
    std::os::unix::fs::symlink(f.repo.join("dort.toml"), &path).unwrap();
    for path in [Path::new("dort.toml"), &path] {
        let mut ui = app(&f, false);
        let task = standalone(&f, &mut ui, path);
        run(&f, &mut ui, task);
        assert!(ui.error.is_some());
        assert!(ui.tools_view.declared.check_preview.is_none());
    }
    assert!(f.calls().is_empty());
}

#[test]
fn saved_paths_are_rebound_before_execution_and_never_fall_back_when_invalid() {
    let f = Fixture::new();
    f.program(
        "custom-brew",
        r#"printf Z >> "$HOME/calls"; printf '{"formulae":[]}'"#,
    );
    let mut ui = app(&f, false);
    settings(&f, &mut ui);
    let custom = f.bin.join("custom-brew");
    ui.tools_view.settings.fields[0] = custom.display().to_string();
    let task = key(&mut ui, KeyCode::Enter).unwrap();
    run(&f, &mut ui, task);
    let mut input = f.input();
    let preview = input.prepare_for_tools(&f.host, false).unwrap();
    fs::remove_file(custom).unwrap();
    assert!(preview.approve().check(&f.host).is_err());
    assert!(f.calls().is_empty());
    input.detection.search_dirs = vec![f.bin.clone()];
    let preview = input.prepare_for_tools(&f.host, false).unwrap();
    assert_eq!(preview.review().needs_setup[0].tool, "a");
    assert_eq!(preview.review().inspections[0].tools, ["b"]);
    assert!(f.calls().is_empty());
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
    let request = &app.tools_view.settings.editor.as_ref().unwrap().request;
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
