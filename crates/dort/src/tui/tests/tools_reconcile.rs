use super::{
    actions::{self, Task},
    app::{App, Tab},
    tests::render,
    tools_reconcile::Mode,
    views,
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
    tools::planning::PackageAction,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    manifest: PathBuf,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        let manifest = root.join("dort.toml");
        fs::write(&manifest, "schema = 1\n[[tools]]\nid = 'node'\nbinary = 'node'\n[tools.install]\nubuntu = { provider = 'mise', package = 'node', version = '22' }\n[[tools]]\nid = 'rg'\nbinary = 'rg'\n[tools.install]\nubuntu = { provider = 'mise', package = 'rg' }\n").unwrap();
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
            root,
            home,
            bin,
            manifest,
            host,
        }
    }
    fn app(&self) -> App {
        let mut app = App::new(false);
        app.tab = Tab::Tools;
        app.tools_view.query = 2;
        app.tools_view.fields[1] = self.root.join("provider-search").to_str().unwrap().into();
        app
    }
    fn provider(&self, id: &str, response: &str) -> PathBuf {
        let path = self.root.join(id);
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf '{id}\\n' >> \"$HOME/calls\"\n{response}\n"),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn run(&self, app: &mut App, task: Task) {
        app.start(false);
        let result = actions::perform_scoped(task, &self.host, None, &[], |_| Ok(()))
            .map_err(|e| actions::diagnostic(&e));
        assert!(app.complete(result).is_none());
    }
    fn collect(&self, app: &mut App, executable: &std::path::Path) {
        app.tools_view.fields[0] = executable.to_str().unwrap().into();
        let task = key(app, KeyCode::Char('R')).unwrap();
        self.run(app, task);
        for c in "yes".chars() {
            assert!(key(app, KeyCode::Char(c)).is_none());
        }
        let task = key(app, KeyCode::Enter).unwrap();
        self.run(app, task);
    }
    fn load(&self, app: &mut App) {
        key(app, KeyCode::Char('p'));
        assert!(app.tools_view.recon_open);
        app.tools_view.reconciliation.fields[0] = self.manifest.to_str().unwrap().into();
        app.tools_view.reconciliation.fields[1] = self.bin.to_str().unwrap().into();
        app.tools_view.reconciliation.focus = 0;
        let task = key(app, KeyCode::Enter).unwrap();
        assert!(matches!(task, Task::ToolsReconcilePrepare(_)));
        self.run(app, task);
    }
    fn calculate(&self, app: &mut App) {
        app.tools_view.reconciliation.focus = 0;
        let task = key(app, KeyCode::Enter).unwrap();
        assert!(matches!(task, Task::ToolsReconcile { .. }));
        self.run(app, task);
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.join("calls")).unwrap_or_default()
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn bind(app: &mut App, index: usize) {
    let before = app.tools_view.reconciliation.bindings.clone();
    key(app, KeyCode::Char(' '));
    assert!(app.choice.is_some());
    assert_eq!(app.tools_view.reconciliation.bindings, before);
    key(app, KeyCode::Home);
    for _ in 0..index {
        key(app, KeyCode::Down);
    }
    assert!(key(app, KeyCode::Enter).is_none());
    assert!(app.choice.is_none());
}
const NODE: &str =
    r#"printf '{"node":[{"version":"22.1.0","installed":true,"source":{"path":"PRIVATE"}}]}'"#;
const RG: &str = r#"printf '{"rg":[{"version":"14.0.0","installed":true}]}'"#;
#[test]
fn tools_menu_destinations_switch_subviews_without_querying_or_losing_drafts() {
    let f = Fixture::new();
    let mut app = f.app();
    assert!(app.activate(super::shell::Action::ToolsReconcile).is_none());
    assert!(app.tools_view.recon_open);
    app.tools_view.reconciliation.fields[0] = "retained".into();
    // Results are read-only, so navigation remains available after leaving the form.
    app.tools_view.reconciliation.mode = Mode::Results;
    assert!(app.activate(super::shell::Action::ToolsSnapshots).is_none());
    assert!(app.tools_view.catalog_browser && !app.tools_view.recon_open);
    assert_eq!(app.tools_view.reconciliation.fields[0], "retained");
    assert!(f.calls().is_empty());
}

#[test]
fn snapshots_are_distinct_immutable_and_selection_or_forgetting_never_queries() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("first", NODE));
    f.collect(&mut app, &f.provider("second", RG));
    assert_eq!(
        app.tools_view
            .snapshots
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["s1", "s2"]
    );
    assert_eq!(f.calls(), "first\nsecond\n");
    key(&mut app, KeyCode::Char('s'));
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Enter);
    assert!(render(&app, 80, 24).contains("Snapshot s1"));
    assert!(
        render(&app, 80, 24)
            .lines()
            .any(|line| line.contains("node") && line.contains("22.1.0"))
    );
    key(&mut app, KeyCode::Char('s'));
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(app.tools_view.snapshots.len(), 1);
    assert_eq!(app.tools_view.snapshots[0].id, "s2");
    key(&mut app, KeyCode::Esc);
    assert_eq!(f.calls(), "first\nsecond\n");
    assert!(!f.home.join(".config").exists());
}
#[test]
fn explicit_bindings_reconcile_multiple_scopes_without_merging_or_executing_candidates() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("first", NODE));
    f.collect(&mut app, &f.provider("second", RG));
    fs::write(
        f.bin.join("node"),
        format!(
            "#!/bin/sh\nprintf ran > '{}'\nPRIVATE",
            f.home.join("binary-ran").display()
        ),
    )
    .unwrap();
    fs::set_permissions(f.bin.join("node"), fs::Permissions::from_mode(0o700)).unwrap();
    f.load(&mut app);
    assert!(app.tools_view.reconciliation.bindings.is_empty());
    assert!(app.tools_view.reconciliation.mode == Mode::Bindings);
    assert!(super::hints::keys(&app).contains(&("Enter", "Compare metadata")));
    bind(&mut app, 1);
    assert_eq!(app.tools_view.reconciliation.bindings["node"], "s1");
    key(&mut app, KeyCode::Char('d'));
    let first = render(&app, 48, 14);
    assert!(first.contains("Snapshot") && first.contains("s1"));
    key(&mut app, KeyCode::PageDown);
    assert_ne!(first, render(&app, 48, 14));
    let hints = super::hints::keys(&app);
    assert!(
        !hints
            .iter()
            .any(|(k, _)| ["Enter", "Space", "Tab"].contains(k))
    );
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.tools_view.reconciliation.binding_metadata());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Tab);
    bind(&mut app, 2);
    assert_eq!(app.tools_view.reconciliation.bindings["rg"], "s2");
    f.calculate(&mut app);
    let result = app.tools_view.reconciliation.result.as_ref().unwrap();
    assert!(
        result
            .plan
            .rows
            .iter()
            .all(|r| r.action == PackageAction::NoChange)
    );
    assert_eq!(f.calls(), "first\nsecond\n");
    assert!(!f.home.join("binary-ran").exists());
    assert!(app.receipt.is_none() && app.preview.is_none());
    assert!(!render(&app, 80, 24).contains("PRIVATE"));
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.reconciliation.details);
    assert!(render(&app, 80, 24).contains("analysis_scope"));
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.tools_view.reconciliation.scroll, 0);
}
#[test]
fn unbound_is_not_observed_and_reconciliation_retry_does_not_inspect_again() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("first", NODE));
    f.load(&mut app);
    f.calculate(&mut app);
    assert!(
        app.tools_view
            .reconciliation
            .result
            .as_ref()
            .unwrap()
            .plan
            .rows
            .iter()
            .all(|r| matches!(
                r.package,
                dort_core::tools::planning::PackageFinding::NotObserved
            ))
    );
    assert!(render(&app, 80, 24).contains("Not observed"));
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    f.calculate(&mut app);
    assert_eq!(f.calls(), "first\n");
}
#[test]
fn changed_declarations_fail_closed_retain_snapshots_and_withhold_source() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("first", NODE));
    f.load(&mut app);
    fs::write(&f.manifest, "PRIVATE invalid manifest").unwrap();
    f.calculate(&mut app);
    assert!(app.tools_view.reconciliation.result.is_none());
    assert!(app.tools_view.reconciliation.failure.is_some());
    assert_eq!(app.tools_view.snapshots.len(), 1);
    assert!(!render(&app, 80, 24).contains("PRIVATE"));
    assert_eq!(f.calls(), "first\n");
}
#[test]
fn small_dry_run_and_late_load_preserve_read_only_scope_and_never_execute() {
    let f = Fixture::new();
    let mut app = f.app();
    app.tools_view.recon_open = true;
    app.tools_view.reconciliation.fields[0] = f.manifest.to_str().unwrap().into();
    app.tools_view.reconciliation.fields[1] = f.bin.to_str().unwrap().into();
    app.tools_view.reconciliation.focus = 3;
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    let task = key(&mut app, KeyCode::Enter).unwrap();
    app.start(false);
    key(&mut app, KeyCode::Left);
    assert!(app.tab == Tab::Tools && app.interaction_notice.is_some());
    app.go_tab(Tab::Git); // Exercise late data after an actual scope change.
    assert!(app.tab == Tab::Git);
    let out = actions::perform_scoped(task, &f.host, None, &[], |_| Ok(()))
        .map_err(|e| actions::diagnostic(&e));
    app.complete(out);
    assert!(!app.tools_view.recon_open);
    app.tab = Tab::Tools;
    app.tools_view.recon_open = true;
    app.dry_run = true;
    f.calculate(&mut app);
    assert!(app.tools_view.reconciliation.result.is_some());
    assert!(f.calls().is_empty());
}
#[test]
fn npm_snapshot_from_a_different_manifest_is_not_silently_rebound() {
    let f = Fixture::new();
    let mut app = f.app();
    let original = f.root.join("original.toml");
    let text = "schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nubuntu={provider='mise',package='node'}\n[[tools]]\nid='cli'\nbinary='cli'\n[tools.install]\nubuntu={provider='npm',package='@scope/cli',runtime_tool='node'}\n";
    fs::write(&original, text).unwrap();
    fs::write(&f.manifest, text).unwrap();
    app.tools_view.query = 3;
    app.tools_view.fields[2] = original.to_str().unwrap().into();
    app.tools_view.fields[3] = "node".into();
    app.tools_view.fields[4] = f.root.join("npm-cli.js").to_str().unwrap().into();
    app.tools_view.fields[5] = f.root.join("prefix").to_str().unwrap().into();
    let executable = f.provider(
        "npm",
        r#"printf '{"dependencies":{"@scope/cli":{"version":"1.0.0"}}}'"#,
    );
    f.collect(&mut app, &executable);
    f.load(&mut app);
    app.tools_view
        .reconciliation
        .bindings
        .insert("cli".into(), "s1".into());
    f.calculate(&mut app);
    assert!(app.tools_view.reconciliation.result.is_none());
    assert!(
        app.tools_view
            .reconciliation
            .failure
            .as_ref()
            .unwrap()
            .contains("manifest differs or changed")
    );
    assert_eq!(app.tools_view.snapshots.len(), 1);
    assert_eq!(f.calls(), "npm\n");
}

#[test]
fn reconciliation_text_shortcuts_and_failed_load_retry_remain_provider_free() {
    let f = Fixture::new();
    let mut app = f.app();
    key(&mut app, KeyCode::Char('p'));
    for ch in "q;?f".chars() {
        key(&mut app, KeyCode::Char(ch));
    }
    assert!(!app.quit && app.help.is_none() && app.shell.overlay.is_none());
    assert_eq!(app.tools_view.reconciliation.fields[0], "q;?f");
    app.tools_view.reconciliation.fields[0] = f.root.join("absent.toml").to_str().unwrap().into();
    app.tools_view.reconciliation.fields[1] = f.bin.to_str().unwrap().into();
    app.tools_view.reconciliation.focus = 3;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.error.is_some());
    let retry = key(&mut app, KeyCode::Char('R')).unwrap();
    assert!(matches!(retry, Task::ToolsReconcilePrepare(_)));
    assert!(f.calls().is_empty());
}

#[test]
fn full_session_refuses_a_new_query_before_approval_instead_of_overwriting_snapshots() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("one", NODE));
    let sample = app.tools_view.snapshots[0].clone();
    for i in 2..=crate::services::reconciliation::MAX_SNAPSHOTS {
        let mut copy = sample.clone();
        copy.id = format!("s{i}");
        app.tools_view.snapshots.push(copy);
    }
    let task = key(&mut app, KeyCode::Char('R')).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.preview.is_none());
    assert!(app.tools_view.notice.is_some());
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(f.calls(), "one\n");
}

#[test]
fn missing_package_details_show_non_executable_commands_without_an_install_action() {
    let f = Fixture::new();
    let mut app = f.app();
    let executable = f.provider(
        "preview",
        "if [ \"$1\" = install ]; then printf ran > \"$HOME/INSTALL-RAN\"; fi\nprintf '{}' ",
    );
    f.collect(&mut app, &executable);
    f.load(&mut app);
    bind(&mut app, 1);
    f.calculate(&mut app);
    let row = &app
        .tools_view
        .reconciliation
        .result
        .as_ref()
        .unwrap()
        .plan
        .rows[0];
    assert_eq!(row.action, PackageAction::ReviewMissingPackage);
    assert_eq!(
        serde_json::to_value(&row.install_preview).unwrap()["state"],
        "preview_only"
    );
    key(&mut app, KeyCode::Enter);
    let mut seen = String::new();
    for _ in 0..100 {
        seen.push_str(&render(&app, 48, 14));
        key(&mut app, KeyCode::PageDown);
    }
    assert!(seen.contains("install_preview") && seen.contains("PREVIEW ONLY"));
    assert!(seen.contains("execution_authorized"));
    assert_eq!(f.calls(), "preview\n");
    assert!(!f.home.join("INSTALL-RAN").exists());
    assert!(app.preview.is_none() && app.confirmation.is_none() && app.receipt.is_none());
}

#[test]
fn compact_forms_bindings_and_results_keep_focus_caret_and_semantic_selection() {
    let f = Fixture::new();
    let mut app = f.app();
    f.collect(&mut app, &f.provider("first", NODE));
    key(&mut app, KeyCode::Char('p'));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for focus in 0..3 {
            app.tools_view.reconciliation.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|fr| views::draw(fr, &app)).unwrap();
            let cursor = terminal.get_cursor_position().unwrap();
            super::tests::assert_text_focus(&terminal, (w, h), cursor);
            super::tests::assert_primary_action(&terminal);
        }
        app.tools_view.reconciliation.focus = 0;
        for _ in 0..3 {
            key(&mut app, KeyCode::Tab);
        }
        assert_eq!(app.tools_view.reconciliation.focus, 0);
    }
    f.load(&mut app);
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        for focus in 0..2 {
            app.tools_view.reconciliation.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|fr| views::draw(fr, &app)).unwrap();
            super::tests::assert_primary_action(&terminal);
            let before = app.tools_view.reconciliation.bindings.clone();
            key(&mut app, KeyCode::Char(' '));
            assert!(app.choice.is_some());
            assert_eq!(app.tools_view.reconciliation.bindings, before);
            key(&mut app, KeyCode::Esc);
            assert_eq!(app.tools_view.reconciliation.focus, focus);
        }
        app.tools_view.reconciliation.focus = 0;
        for _ in 0..2 {
            key(&mut app, KeyCode::Tab);
        }
        assert_eq!(app.tools_view.reconciliation.focus, 0);
    }
    f.calculate(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(48, 14)).unwrap();
    terminal.draw(|fr| views::draw(fr, &app)).unwrap();
    super::tests::assert_primary_action(&terminal);
}
