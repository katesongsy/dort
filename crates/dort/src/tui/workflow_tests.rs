use super::{
    actions::{self, Output, Task},
    app::App,
};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    planner::{Request, UnmanageMode},
    platform::{Architecture, Platform},
    state::WriterLock,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs, path::PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    root: PathBuf,
}
impl Fixture {
    fn new(register: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let home = base.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        let root = base.join("repository");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("source"), "ordinary settings\n").unwrap();
        fs::write(
            root.join("dort.toml"),
            "schema=1\n[[files]]\nid='editor'\nsource='source'\ntarget='~/.editor'\nmode='link'\n",
        )
        .unwrap();
        if register {
            let lock = WriterLock::acquire(&host).unwrap();
            RepositoryRegistry::register(&host, &lock, &root).unwrap();
        }
        Self {
            _temp: temp,
            host,
            root,
        }
    }
    fn prepare(&self, request: Request) -> Output {
        actions::perform(
            Task::Files(request),
            &self.host,
            Some(self.root.clone()),
            |_| Ok(()),
        )
        .unwrap()
    }
    fn execute(&self, task: Task) {
        actions::perform(task, &self.host, Some(self.root.clone()), |_| Ok(())).unwrap();
    }
}
// Sequential observations are not globally atomic: sibling test directories churn.
// Match the core approval digest's parent-only normalization, retaining identity/mode/owner.
fn assert_plan_parity(actual: &dort_core::planner::Plan, expected: &dort_core::planner::Plan) {
    fn normalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "parents" {
                        for parent in value.as_array_mut().unwrap() {
                            if let Some(fp) = parent
                                .get_mut("fingerprint")
                                .and_then(|v| v.as_object_mut())
                            {
                                for field in ["size", "modified", "changed", "links"] {
                                    fp.remove(field);
                                }
                            }
                        }
                    } else {
                        normalize(value);
                    }
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(normalize),
            _ => (),
        }
    }
    let mut actual = serde_json::to_value(actual).unwrap();
    let mut expected = serde_json::to_value(expected).unwrap();
    normalize(&mut actual);
    normalize(&mut expected);
    assert!(
        actual == expected,
        "CLI/shared and TUI plans differ after parent-only churn normalization"
    );
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn type_text(app: &mut App, value: &str) {
    for c in value.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}
fn approve(app: &mut App) -> Task {
    assert!(key(app, KeyCode::Enter).is_none());
    type_text(app, "yes");
    key(app, KeyCode::Enter).expect("explicit confirmation produces approved task")
}
#[test]
fn welcome_next_steps_are_visible_at_the_floor_without_creating_state() {
    let fixture = Fixture::new(false);
    let mut app = App::new(false);
    app.complete(Ok(Output::Data(
        actions::load(&fixture.host, None).unwrap(),
    )));
    let screen = super::tests::render(&app, 48, 14);
    assert!(screen.contains("Welcome | manage local files"));
    assert!(screen.contains("n: Create/attach | p: Choose repository"));
    assert!(screen.contains("f: Discover files | v: Check a manifest"));
    assert!(!fixture.host.state_dir.exists());
    assert!(!fixture.host.config_dir.exists());
}

#[test]
fn preview_scroll_is_bounded_by_escaped_metadata_and_resets_on_home() {
    let fixture = Fixture::new(false);
    let mut app = App::new(true);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    // Exercise only presentation offsets; retain the real opaque core preview.
    app.preview.as_mut().unwrap().text = format!("{}\n{}\nEND", "é".repeat(20), "row\n".repeat(30));
    for _ in 0..100 {
        app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), 48, 14);
        app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 48, 14);
    }
    let p = app.preview.as_ref().unwrap();
    assert_eq!(p.horizontal, 120 - 46); // Each é becomes six ASCII escape bytes.
    assert_eq!(p.scroll, 33 - 3);
    app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), 48, 14);
    assert_eq!(app.preview.as_ref().unwrap().horizontal, 120 - 46 - 8);
    app.resize(140, 60);
    let p = app.preview.as_ref().unwrap();
    assert_eq!((p.scroll, p.horizontal), (0, 0));
    app.resize(48, 14);
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), 48, 14);
    app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 48, 14);
    app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 48, 14);
    let p = app.preview.as_ref().unwrap();
    assert_eq!((p.scroll, p.horizontal), (0, 0));
    assert!(super::tests::render(&app, 48, 14).contains("\\u{e9}"));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.confirmation.is_none()); // Scroll/reset never changes dry-run approval.
    assert!(!fixture.host.state_dir.exists());
    assert!(!fixture.host.config_dir.exists());
}

#[test]
fn previews_cancellation_and_dry_run_never_create_state() {
    let fixture = Fixture::new(false);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "yes");
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(app.preview.is_none());
    assert!(!fixture.host.state_dir.exists());
    assert!(!fixture.host.config_dir.exists());
    assert!(!fixture.host.home.join(".editor").exists());
    let mut app = App::new(true);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    assert!(!fixture.host.state_dir.exists());
}
#[test]
fn approved_apply_remove_and_recovery_use_the_same_guarded_services() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    let task = approve(&mut app);
    fixture.execute(task);
    assert!(fixture.host.home.join(".editor").is_symlink());
    app.complete(Ok(fixture.prepare(Request::Unmanage {
        id: "editor".into(),
        mode: UnmanageMode::Remove,
    })));
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "yes");
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.as_ref().unwrap().removal_ack.is_none());
    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "editor");
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.confirmation.as_ref().unwrap().removal_ack.as_deref(),
        Some("editor")
    );
    type_text(&mut app, "yes");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    fixture.execute(task);
    assert!(!fixture.host.home.join(".editor").exists());
    assert_eq!(
        fs::read_to_string(fixture.root.join("source")).unwrap(),
        "ordinary settings\n"
    );
    let data = actions::load(&fixture.host, Some(&fixture.root)).unwrap();
    // Only the most recent transaction's original-manifest guards remain current.
    let row = data
        .history
        .iter()
        .find(|row| {
            dort_core::recovery::preview(&fixture.host, &row.repository_id, &row.transaction_id)
                .is_ok()
        })
        .unwrap();
    let preview = actions::perform(
        Task::Rollback(row.repository_id.clone(), row.transaction_id.clone()),
        &fixture.host,
        None,
        |_| Ok(()),
    )
    .unwrap();
    app.complete(Ok(preview));
    let task = approve(&mut app);
    fixture.execute(task);
    assert!(fixture.host.home.join(".editor").is_symlink());
}
#[test]
fn stale_approval_and_lock_contention_are_not_bypassed() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    let approved = approve(&mut app);
    fs::write(fixture.root.join("source"), "later edit\n").unwrap();
    assert!(actions::perform(approved, &fixture.host, None, |_| Ok(())).is_err());
    assert!(!fixture.host.home.join(".editor").exists());
    app.complete(Ok(fixture.prepare(Request::Apply)));
    let approved = approve(&mut app);
    let _lock = WriterLock::acquire(&fixture.host).unwrap();
    assert!(matches!(
        actions::perform(approved, &fixture.host, None, |_| Ok(())),
        Err(dort_core::Error::Locked)
    ));
}
#[test]
fn confirmation_snapshots_and_resize_guard() {
    let fixture = Fixture::new(false);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    key(&mut app, KeyCode::Enter);
    let screen = super::tests::render(&app, 48, 14);
    let modal = |screen: &str| {
        screen
            .lines()
            .skip(3)
            .take(8)
            .map(|line| {
                line.chars()
                    .skip(3)
                    .take(42)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let expected = "Approve reviewed effects\nType literal yes, then Enter.\nExecution independently revalidates all\nguards.\n\n>\n\nEsc: cancel without execution";
    assert_eq!(modal(&screen), expected);
    type_text(&mut app, "yes");
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 40, 10)
            .is_none()
    );
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Enter);
    let screen = super::tests::render(&app, 48, 14);
    assert_eq!(modal(&screen), expected);
    key(&mut app, KeyCode::Esc);
    assert!(app.confirmation.is_none());
    assert!(app.preview.is_some());
    assert!(!fixture.host.state_dir.exists());
}
#[test]
fn blocked_preview_cannot_confirm_and_errors_remain_inspectable() {
    let fixture = Fixture::new(false);
    fs::remove_file(fixture.root.join("source")).unwrap();
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    app.complete(Err("unsafe parent\nno mutation".into()));
    assert!(app.error.is_some());
    assert!(key(&mut app, KeyCode::Char('a')).is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.error.is_none());
}

#[test]
fn correct_apply_is_noop_and_keep_retains_source() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    fixture.execute(approve(&mut app));
    let count = dort_core::journal::diagnose(&fixture.host).unwrap().len();
    app.complete(Ok(fixture.prepare(Request::Apply)));
    fixture.execute(approve(&mut app));
    assert_eq!(
        dort_core::journal::diagnose(&fixture.host).unwrap().len(),
        count
    );
    app.complete(Ok(fixture.prepare(Request::Unmanage {
        id: "editor".into(),
        mode: UnmanageMode::Keep,
    })));
    fixture.execute(approve(&mut app));
    assert!(!fixture.host.home.join(".editor").is_symlink());
    assert_eq!(
        fs::read_to_string(fixture.host.home.join(".editor")).unwrap(),
        "ordinary settings\n"
    );
    assert!(fixture.root.join("source").is_file());
}

#[test]
fn bootstrap_abandon_is_explicit_and_available_without_a_selected_repository() {
    use dort_core::bootstrap;
    let fixture = Fixture::new(false);
    let root = fixture.root.parent().unwrap().join("new-repository");
    let git = PathBuf::from("/no-external-programs/git");
    let preview = bootstrap::preview(&fixture.host, &root, &git).unwrap();
    let lock = WriterLock::acquire(&fixture.host).unwrap();
    assert!(
        bootstrap::execute(
            &fixture.host,
            &lock,
            bootstrap::Approval::from_preview(preview).unwrap(),
            &git,
            |_| Err(dort_core::Error::Invalid("interrupt preparing".into()))
        )
        .is_err()
    );
    drop(lock);
    let data = actions::load(&fixture.host, None).unwrap();
    assert_eq!(data.history.len(), 1);
    let row = &data.history[0];
    assert!(row.blocks_writes());
    let mut app = App::new(false);
    app.complete(Ok(actions::perform(
        Task::Bootstrap(
            row.repository_id.clone(),
            row.transaction_id.clone(),
            bootstrap::RecoveryAction::Abandon,
        ),
        &fixture.host,
        None,
        |_| Ok(()),
    )
    .unwrap()));
    assert!(dort_core::journal::diagnose(&fixture.host).unwrap()[0].blocks_writes());
    fixture.execute(approve(&mut app));
    assert!(!dort_core::journal::diagnose(&fixture.host).unwrap()[0].blocks_writes());
    assert!(!root.exists());
}

fn fake_git(fixture: &Fixture) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = fixture.root.parent().unwrap().join("git");
    fs::write(&path, "#!/bin/sh\nfor arg do\ncase \"$arg\" in\ninit) /bin/mkdir .git || exit 90; printf 'ref: refs/heads/main\\n' > .git/HEAD; exit 0;;\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf '# branch.head main\\000'; exit 0;;\nesac\ndone\nexit 94\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}
fn onboarding_app(fixture: &Fixture, dry_run: bool) -> App {
    let mut app = App::new(dry_run);
    app.launch_directory = fixture.root.parent().unwrap().to_owned();
    app.git = Ok(fake_git(fixture));
    app.complete(Ok(Output::Data(
        actions::load(&fixture.host, None).unwrap(),
    )));
    app
}
fn run_task(app: &mut App, fixture: &Fixture, task: Task) -> Option<Task> {
    app.retry = task.retry_intent();
    app.start(matches!(task, Task::Execute(_)));
    let result = actions::perform_scoped(
        task,
        &fixture.host,
        app.repository.clone(),
        &app.requested_profiles,
        |_| Ok(()),
    )
    .map_err(|error| actions::diagnostic(&error));
    app.complete(result)
}

#[test]
fn create_retains_input_on_back_then_approves_and_switches_session() {
    use super::shell::{Form, Overlay};
    let fixture = Fixture::new(false);
    let mut app = onboarding_app(&fixture, false);
    key(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "new-q?repo");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let report = &app.preview.as_ref().unwrap().text;
    assert!(report.contains("Create local repository") && report.contains("new-q?repo"));
    assert!(!fixture.host.state_dir.exists());
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.shell.overlay, Some(Overlay::Form(Form::Init))));
    assert_eq!(app.shell.init_draft, "new-q?repo");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    assert!(app.result.as_ref().unwrap().contains("Transaction ID:"));
    assert!(app.shell.overlay.is_none());
    assert_eq!(
        app.repository.as_ref().unwrap(),
        &fixture.root.parent().unwrap().join("new-q?repo")
    );
    assert_eq!(app.origin, "approved create/attach");
    run_task(&mut app, &fixture, refresh);
    assert!(app.data.as_ref().unwrap().overview.contains("registered"));
    let root = app.repository.as_ref().unwrap();
    assert!(root.join(".git/HEAD").is_file());
    assert_eq!(
        fs::read_to_string(root.join("dort.toml")).unwrap(),
        "schema = 1\n"
    );
    assert_eq!(
        dort_core::local::LocalConfig::load(&fixture.host)
            .unwrap()
            .select_repository(None)
            .unwrap(),
        *root
    );
}

#[test]
fn attachment_uses_shared_bootstrap_and_session_picker_never_persists() {
    let fixture = Fixture::new(false);
    let mut app = onboarding_app(&fixture, false);
    let git = app.git.as_ref().unwrap();
    let shared =
        crate::services::BootstrapPreview::prepare(&fixture.host, &fixture.root, git).unwrap();
    let core = dort_core::bootstrap::preview(&fixture.host, &fixture.root, git).unwrap();
    assert_eq!(
        serde_json::to_value(&shared.report).unwrap(),
        serde_json::to_value(&core).unwrap()
    );
    assert!(!shared.report.create);
    let bytes = fs::read(fixture.root.join("dort.toml")).unwrap();
    key(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "repository");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(
        app.preview
            .as_ref()
            .unwrap()
            .text
            .contains("Attach local repository")
    );
    assert!(!fixture.host.state_dir.exists());
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert_eq!(fs::read(fixture.root.join("dort.toml")).unwrap(), bytes);
    let config_before = fs::read(fixture.host.local_config_path()).unwrap();
    let unregistered = fixture.root.parent().unwrap().join("other");
    fs::create_dir(&unregistered).unwrap();
    fs::write(unregistered.join("dort.toml"), "schema=1\n").unwrap();
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::Enter); // exact path form
    type_text(&mut app, "other");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.repository, Some(unregistered));
    assert!(app.data.as_ref().unwrap().overview.contains("unregistered"));
    assert_eq!(
        fs::read(fixture.host.local_config_path()).unwrap(),
        config_before
    );
    assert_eq!(app.data.as_ref().unwrap().history.len(), 1); // all repositories
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::Down); // registered active
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.repository.is_none());
    assert_eq!(
        app.data.as_ref().unwrap().repository,
        Some(fixture.root.clone())
    );
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down); // known root, raw identity rather than display parsing
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.repository, Some(fixture.root.clone()));
    assert_eq!(
        fs::read(fixture.host.local_config_path()).unwrap(),
        config_before
    );
}

#[test]
fn invalid_path_missing_git_and_read_errors_preserve_draft_and_old_scope() {
    let fixture = Fixture::new(true);
    let mut app = onboarding_app(&fixture, false);
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "absent/q?path");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.as_ref().unwrap().contains("Read failed:"));
    assert!(!app.error.as_ref().unwrap().contains("partial effects"));
    assert_eq!(
        app.data.as_ref().unwrap().repository,
        Some(fixture.root.clone())
    );
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.choose_draft, "absent/q?path");
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "new");
    app.git = Err("Git missing".into());
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.error.as_ref().unwrap().contains("Git missing"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.init_draft, "new");
}

#[test]
fn bootstrap_approval_staleness_and_lock_failure_are_visible_without_retry() {
    let fixture = Fixture::new(false);
    let mut app = onboarding_app(&fixture, false);
    key(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "new");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let task = approve(&mut app);
    let lock = WriterLock::acquire(&fixture.host).unwrap();
    assert!(run_task(&mut app, &fixture, task).is_none());
    assert!(app.error.as_ref().unwrap().contains("partial effects"));
    assert!(app.preview.is_none() && app.confirmation.is_none());
    assert!(app.repository.is_none());
    drop(lock);
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let task = approve(&mut app);
    fs::create_dir(fixture.root.parent().unwrap().join("new")).unwrap();
    assert!(run_task(&mut app, &fixture, task).is_none());
    assert!(app.error.is_some());
    assert!(
        !fixture
            .root
            .parent()
            .unwrap()
            .join("new/dort.toml")
            .exists()
    );
}

#[test]
fn dry_run_bootstrap_can_never_execute_even_after_mode_toggle() {
    let fixture = Fixture::new(false);
    let mut app = onboarding_app(&fixture, true);
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "toggle");
    key(&mut app, KeyCode::Enter);
    assert!(app.dry_run);
    key(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "new");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(!fixture.host.state_dir.exists() && !fixture.host.config_dir.exists());
    assert!(!fixture.root.parent().unwrap().join("new").exists());
}

#[test]
fn shrinking_clears_both_removal_ack_and_yes_until_fresh_confirmation() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.complete(Ok(fixture.prepare(Request::Apply)));
    fixture.execute(approve(&mut app));
    app.complete(Ok(fixture.prepare(Request::Unmanage {
        id: "editor".into(),
        mode: UnmanageMode::Remove,
    })));
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "editor");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "yes");
    app.resize(47, 14);
    assert!(app.confirmation.is_none());
    app.resize(80, 24);
    key(&mut app, KeyCode::Enter);
    let confirmation = app.confirmation.as_ref().unwrap();
    assert!(confirmation.removal_ack.is_none() && confirmation.input.is_empty());
    assert!(fixture.host.home.join(".editor").is_symlink());
}

#[test]
fn discovery_marks_ineligible_entries_and_enter_preserves_existing_draft() {
    let fixture = Fixture::new(false);
    let config = fixture.host.home.join(".config");
    fs::create_dir(&config).unwrap();
    let directory = config.join("nvim");
    fs::create_dir(&directory).unwrap();
    let file = directory.join("init.lua");
    fs::write(&file, "ordinary settings\n").unwrap();
    let linked = config.join("hardlink");
    fs::write(&linked, "ordinary settings\n").unwrap();
    fs::hard_link(&linked, config.join("hardlink-peer")).unwrap();
    let symlink = config.join("shortcut");
    std::os::unix::fs::symlink(&file, &symlink).unwrap();
    let mut app = onboarding_app(&fixture, false);
    app.shell.adoption.target = "retained target".into();
    app.shell.adoption.id = "retained-id".into();
    app.shell.adoption.source = "retained/source".into();
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    run_task(&mut app, &fixture, task);
    let index = app
        .discovery
        .as_ref()
        .unwrap()
        .report
        .candidates
        .iter()
        .position(|c| c.entry.path == directory)
        .unwrap();
    app.candidate_selected = index;
    for (width, height) in [(48, 14), (80, 24), (140, 40)] {
        let screen = super::tests::render(&app, width, height);
        assert!(screen.contains("> [not eligible]"));
    }
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.shell.overlay.is_none());
    assert!(app.status.contains("Directories cannot be adopted"));
    assert_eq!(app.shell.adoption.target, "retained target");
    assert_eq!(app.shell.adoption.id, "retained-id");
    assert_eq!(app.shell.adoption.source, "retained/source");
    for path in [&linked, &symlink] {
        app.candidate_selected = app
            .discovery
            .as_ref()
            .unwrap()
            .report
            .candidates
            .iter()
            .position(|c| c.entry.path == *path)
            .unwrap();
        assert!(super::tests::render(&app, 80, 24).contains("> [not eligible]"));
        assert!(key(&mut app, KeyCode::Enter).is_none());
        assert!(app.shell.overlay.is_none());
        assert_eq!(app.shell.adoption.target, "retained target");
    }
    app.candidate_selected = index;
    key(&mut app, KeyCode::Char('i'));
    assert!(super::tests::render(&app, 140, 40).contains("Not eligible for adoption"));
    key(&mut app, KeyCode::Esc);
    app.candidate_selected = app
        .discovery
        .as_ref()
        .unwrap()
        .report
        .candidates
        .iter()
        .position(|c| c.entry.path == file)
        .unwrap();
    assert!(super::tests::render(&app, 80, 24).contains("> [eligible hint]"));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.shell.adoption.target, file.to_str().unwrap());
    assert!(matches!(
        app.shell.overlay,
        Some(super::shell::Overlay::Form(super::shell::Form::Adopt))
    ));
    assert!(!fixture.host.state_dir.exists());
    assert!(!fixture.host.config_dir.join("dort").exists());
    assert!(directory.is_dir() && file.is_file());
}

#[test]
fn discovery_is_independent_metadata_only_and_keeps_exact_candidate_identity() {
    let fixture = Fixture::new(false);
    fs::write(fixture.host.home.join(".q?config"), "private-body-marker\n").unwrap();
    let mut app = onboarding_app(&fixture, false);
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    run_task(&mut app, &fixture, task);
    let discovery = app.discovery.as_ref().unwrap();
    assert!(discovery.repository.is_none());
    let core = dort_core::discovery::discover(&fixture.host, None).unwrap();
    assert_eq!(
        serde_json::to_value(&discovery.report).unwrap(),
        serde_json::to_value(core).unwrap()
    );
    let screen = super::tests::render(&app, 240, 50);
    assert!(
        screen.contains(".q?config") && screen.contains("Metadata eligibility"),
        "{screen}"
    );
    assert!(!screen.contains("private-body-marker"));
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.shell.adoption.target,
        fixture.host.home.join(".q?config").to_str().unwrap()
    );
    assert_eq!(app.shell.adoption.id, "q-config");
    assert_eq!(app.shell.adoption.source, "files/q-config");
    key(&mut app, KeyCode::Esc);
    fs::write(fixture.host.home.join(".aaa"), "ordinary\n").unwrap();
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.candidate_selected, 1);
    fs::remove_file(fixture.host.home.join(".q?config")).unwrap();
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.status.contains("disappeared"));
    assert!(!fixture.host.config_dir.exists() && !fixture.host.state_dir.exists());
}

#[test]
fn discovery_protects_selected_root_and_rejects_invalid_scope_without_fallback() {
    let fixture = Fixture::new(false);
    let root = fixture.host.home.join(".config/repository");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("ordinary"), "ordinary settings\n").unwrap();
    let local = dort_core::local::LocalConfig::load(&fixture.host).unwrap();
    let unscoped = crate::services::discover(&fixture.host, &local, None).unwrap();
    assert!(
        unscoped
            .report
            .candidates
            .iter()
            .any(|c| c.entry.path.starts_with(&root))
    );
    let scoped = crate::services::discover(&fixture.host, &local, Some(&root)).unwrap();
    assert!(
        scoped
            .report
            .candidates
            .iter()
            .all(|c| !c.entry.path.starts_with(&root))
    );
    assert!(crate::services::discover(&fixture.host, &local, Some(&root.join("absent"))).is_err());
    assert!(
        crate::services::discover(&fixture.host, &local, Some(&root.join("ordinary"))).is_err()
    );
    let fixture = Fixture::new(true);
    fs::remove_dir_all(&fixture.root).unwrap();
    let local = dort_core::local::LocalConfig::load(&fixture.host).unwrap();
    assert!(crate::services::discover(&fixture.host, &local, None).is_err());
}

fn adoption_fields(app: &mut App, target: &str, id: &str, source: &str) {
    key(app, KeyCode::Char('o'));
    type_text(app, target);
    key(app, KeyCode::Enter);
    type_text(app, id);
    key(app, KeyCode::Enter);
    type_text(app, source);
    key(app, KeyCode::Enter); // focus Review; never executes from ordinary field
    assert!(app.preview.is_none());
}

#[test]
fn adoption_retains_all_input_through_missing_repository_picker_and_attachment() {
    use super::shell::{Form, Overlay};
    let fixture = Fixture::new(false);
    fs::write(fixture.host.home.join(".q?config"), "ordinary settings\n").unwrap();
    let mut app = onboarding_app(&fixture, false);
    adoption_fields(&mut app, "~/.q?config", "config", "files/config");
    assert!(!app.quit);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.is_some());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.adoption.target, "~/.q?config");
    key(&mut app, KeyCode::Tab); // Choose repository
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter); // exact path
    type_text(&mut app, "repository");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Form(Form::Adopt))
    ));
    assert_eq!(app.shell.adoption.source, "files/config");
    assert!(!fixture.host.config_dir.exists() && !fixture.host.state_dir.exists());
    key(&mut app, KeyCode::Tab); // Create/attach, from Choose button
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "repository");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Form(Form::Adopt))
    ));
    assert_eq!(app.shell.adoption.id, "config");
    app.shell.adoption.focused = 3;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    let shared = crate::services::adopt_request(
        std::path::Path::new("~/.q?config"),
        "config",
        std::path::Path::new("files/config"),
    );
    let expected = dort_core::planner::plan(
        dort_core::planner::observe(&fixture.host, &fixture.root, &[], shared).unwrap(),
    );
    let super::actions::Pending::Files(plan) = &app.preview.as_ref().unwrap().pending else {
        panic!("file plan")
    };
    assert_plan_parity(plan, &expected);
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert!(fixture.host.home.join(".q?config").is_symlink());
    assert_eq!(
        fs::read_to_string(fixture.root.join("files/config")).unwrap(),
        "ordinary settings\n"
    );
    assert!(app.result.as_ref().unwrap().contains("Transaction ID:"));
}

#[test]
fn adoption_blockers_dry_run_and_back_never_write_or_lose_drafts() {
    let fixture = Fixture::new(false);
    fs::write(fixture.host.home.join(".ordinary"), "ordinary settings\n").unwrap();
    let mut app = onboarding_app(&fixture, true);
    app.repository = Some(fixture.root.clone());
    adoption_fields(&mut app, "~/.ordinary", "ordinary", "source"); // existing source
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.is_some() || app.preview.as_ref().is_some_and(|p| p.pending.blocked()));
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    app.shell.adoption.source = "files/ordinary".into();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(!app.preview.as_ref().unwrap().pending.blocked());
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('o'));
    assert_eq!(app.shell.adoption.target, "~/.ordinary");
    assert!(!fixture.host.config_dir.exists() && !fixture.host.state_dir.exists());
    assert!(!fixture.root.join("files").exists());
    assert!(!fixture.host.home.join(".ordinary").is_symlink());
    // Ordinary policy refusals come from the same core, without any content display.
    for (target, id, source) in [
        ("~/.ssh/key", "key", "files/key"),
        ("~/.ordinary", "bad/id", "files/ordinary"),
        ("~/.ordinary", "ordinary", "../escape"),
    ] {
        let request = crate::services::adopt_request(
            std::path::Path::new(target),
            id,
            std::path::Path::new(source),
        );
        let outcome = actions::perform(
            Task::Files(request),
            &fixture.host,
            Some(fixture.root.clone()),
            |_| Ok(()),
        );
        assert!(match outcome {
            Err(_) => true,
            Ok(Output::Preview(p)) => p.blocked(),
            _ => false,
        });
    }
}

#[test]
fn standalone_check_is_independent_and_preserves_path_profile_drafts_on_errors() {
    use super::{
        app::Tab,
        shell::{Form, Overlay},
    };
    let fixture = Fixture::new(false);
    let manifest = fixture.root.parent().unwrap().join("standalone-q?.toml");
    fs::write(&manifest, "schema=1\n[profiles]\navailable=['extra']\n").unwrap();
    let mut app = onboarding_app(&fixture, false);
    app.repository = Some(fixture.root.join("absent")); // independent even of invalid file scope
    key(&mut app, KeyCode::Char('v'));
    type_text(&mut app, "standalone-q?.toml");
    key(&mut app, KeyCode::Enter); // path -> explicit Check button
    assert!(app.check.is_none() && !app.quit);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.tab == Tab::Checks);
    let (report, standalone) = app.check.as_ref().unwrap();
    assert!(*standalone && report.active_profiles.contains("base"));
    let local = dort_core::local::LocalConfig::load(&fixture.host).unwrap();
    let expected =
        crate::services::check_manifest(&fixture.host, &local, manifest.clone(), &[]).unwrap();
    assert_eq!(
        serde_json::to_value(&**report).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert!(
        !serde_json::to_string(&**report)
            .unwrap()
            .contains("choices")
    ); // CLI JSON unchanged
    let screen = super::tests::render(&app, 160, 40);
    assert!(
        screen.contains("standalone manifest (independent")
            && screen.contains("Tool dependency order")
    );
    key(&mut app, KeyCode::Char('v'));
    key(&mut app, KeyCode::Tab); // Check -> Profiles
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(matches!(app.shell.overlay, Some(Overlay::Profiles(_))));
    key(&mut app, KeyCode::Down); // extra
    key(&mut app, KeyCode::Enter); // toggle
    key(&mut app, KeyCode::Down); // Apply
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.shell.check_profiles, ["extra"]);
    assert!(app.requested_profiles.is_empty());
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Form(Form::Check))
    ));
    assert!(
        app.check
            .as_ref()
            .unwrap()
            .0
            .active_profiles
            .contains("extra")
    );
    fs::write(&manifest, "malformed").unwrap();
    app.shell.manifest_focused = 1;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.is_some());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.manifest_draft, "standalone-q?.toml");
    assert_eq!(app.shell.check_profiles, ["extra"]);
    assert!(!fixture.host.config_dir.exists() && !fixture.host.state_dir.exists());
}

#[test]
fn temporary_profiles_are_mandatory_union_and_drive_full_inventory_and_plans() {
    use super::shell::Overlay;
    let fixture = Fixture::new(true);
    fs::write(fixture.root.join("dort.toml"), "schema=1\n[profiles]\navailable=['defaults','platform','local','extra']\ndefault=['defaults']\n[profiles.platform]\nubuntu=['platform']\n[[files]]\nid='extra'\nsource='source'\ntarget='~/.extra'\nmode='link'\nprofiles=['extra']\n").unwrap();
    let config_path = fixture.host.local_config_path();
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("active_profiles = []", "active_profiles = ['local']");
    fs::write(&config_path, &config).unwrap();
    let mut app = onboarding_app(&fixture, false);
    assert!(app.data.as_ref().unwrap().files.is_empty());
    let task = key(&mut app, KeyCode::Char('t')).unwrap();
    run_task(&mut app, &fixture, task);
    let Overlay::Profiles(picker) = app.shell.overlay.as_ref().unwrap() else {
        panic!("profiles")
    };
    assert_eq!(
        picker
            .report
            .choices
            .mandatory
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["base", "defaults", "local", "platform"]
    );
    assert!(super::tests::render(&app, 48, 14).contains("> [x] base (mandatory)"));
    key(&mut app, KeyCode::Enter); // base cannot be removed
    let Overlay::Profiles(picker) = app.shell.overlay.as_ref().unwrap() else {
        panic!("profiles")
    };
    assert!(picker.requested.is_empty());
    key(&mut app, KeyCode::Down); // defaults
    key(&mut app, KeyCode::Down); // extra
    key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    } // local/platform/Apply
    assert!(super::tests::render(&app, 48, 14).contains("> Apply additions"));
    let task = key(&mut app, KeyCode::Enter).unwrap();
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert_eq!(app.requested_profiles, ["extra"]);
    assert_eq!(app.data.as_ref().unwrap().files[0].0, "extra");
    let task = key(&mut app, KeyCode::Char('a')).unwrap();
    run_task(&mut app, &fixture, task);
    let super::actions::Pending::Files(plan) = &app.preview.as_ref().unwrap().pending else {
        panic!("files")
    };
    let local = dort_core::local::LocalConfig::load(&fixture.host).unwrap();
    let expected = dort_core::planner::plan(
        crate::services::observe(
            &fixture.host,
            &local,
            None,
            &["extra".into()],
            Request::Apply,
        )
        .unwrap(),
    );
    assert_plan_parity(plan, &expected);
    assert!(plan.inputs.active_profiles.contains("extra"));
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Char('c')).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(
        app.check
            .as_ref()
            .unwrap()
            .0
            .active_profiles
            .contains("extra")
    );
    // Returning to even the same registered root invalidates optional scope additions.
    key(&mut app, KeyCode::Char('p'));
    key(&mut app, KeyCode::Down);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.requested_profiles.is_empty() && app.check.is_none());
    assert_eq!(fs::read_to_string(&config_path).unwrap(), config);
    assert!(
        crate::services::check_manifest(
            &fixture.host,
            &local,
            fixture.root.join("dort.toml"),
            &["unknown".into()]
        )
        .is_err()
    );
    assert!(!fixture.host.home.join(".extra").exists());
}

fn choose_action(app: &mut App, query: &str) {
    key(app, KeyCode::Char(' '));
    key(app, KeyCode::Char('/'));
    type_text(app, query);
    key(app, KeyCode::Enter);
}

#[test]
fn file_search_is_text_only_and_never_narrows_approved_apply() {
    let fixture = Fixture::new(true);
    fs::write(fixture.root.join("second"), "other ordinary settings\n").unwrap();
    let mut manifest = fs::read_to_string(fixture.root.join("dort.toml")).unwrap();
    manifest.push_str("[[files]]\nid='second'\nsource='second'\ntarget='~/.second'\nmode='link'\n");
    fs::write(fixture.root.join("dort.toml"), manifest).unwrap();
    let mut app = onboarding_app(&fixture, false);
    key(&mut app, KeyCode::Char('2'));
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "q?/");
    assert!(!app.quit && app.visible_file_rows().is_empty());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.inventory.managed.query, "q?/");
    assert!(!app.inventory.managed.editing);
    let task = key(&mut app, KeyCode::Char('a')).unwrap();
    run_task(&mut app, &fixture, task);
    let preview = app.preview.as_ref().unwrap();
    assert!(preview.text.contains("display filters never narrow"));
    let super::actions::Pending::Files(plan) = &preview.pending else {
        panic!("plan")
    };
    assert_eq!(plan.files.len(), 2);
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert!(
        fixture.host.home.join(".editor").is_symlink()
            && fixture.host.home.join(".second").is_symlink()
    );
    assert_eq!(app.data.as_ref().unwrap().history.len(), 1);
    assert!(app.result.as_ref().unwrap().contains("Transaction ID:"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Delete);
    assert_eq!(app.visible_file_rows(), [0, 1]);
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "second");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.selected, 1);
    assert_eq!(app.visible_file_rows(), [1]);
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.selected, 1);
    assert_eq!(app.inventory.managed.query, "second");
    key(&mut app, KeyCode::Enter); // compact details
    let screen = super::tests::render(&app, 80, 30);
    assert!(screen.contains("Source:") && screen.contains("Symlink"));
    let task = key(&mut app, KeyCode::Char('a')).unwrap();
    run_task(&mut app, &fixture, task);
    let task = approve(&mut app);
    let refresh = run_task(&mut app, &fixture, task).unwrap();
    run_task(&mut app, &fixture, refresh);
    assert!(
        app.result
            .as_ref()
            .unwrap()
            .contains("no transaction created")
    );
    assert_eq!(app.data.as_ref().unwrap().history.len(), 1);
}

#[test]
fn exact_id_forms_can_keep_or_delete_a_mapping_outside_the_display_filter() {
    use super::shell::{Form, Overlay};
    for remove in [false, true] {
        let fixture = Fixture::new(true);
        let mut app = App::new(false);
        app.complete(Ok(fixture.prepare(Request::Apply)));
        fixture.execute(approve(&mut app));
        let mut app = onboarding_app(&fixture, false);
        key(&mut app, KeyCode::Char('2'));
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "not-editor");
        key(&mut app, KeyCode::Esc);
        assert!(key(&mut app, KeyCode::Char('k')).is_none()); // no hidden selected operation
        assert!(app.preview.is_none());
        choose_action(
            &mut app,
            if remove {
                "delete managed link: enter"
            } else {
                "keep local file: enter"
            },
        );
        assert!(matches!(
            app.shell.overlay,
            Some(Overlay::Form(Form::KeepId | Form::RemoveId))
        ));
        type_text(&mut app, "editor");
        key(&mut app, KeyCode::Enter);
        let task = key(&mut app, KeyCode::Enter).unwrap();
        run_task(&mut app, &fixture, task);
        key(&mut app, KeyCode::Enter);
        if remove {
            type_text(&mut app, "yes");
            assert!(key(&mut app, KeyCode::Enter).is_none());
            assert!(app.confirmation.as_ref().unwrap().removal_ack.is_none());
            for _ in 0..3 {
                key(&mut app, KeyCode::Backspace);
            }
            type_text(&mut app, "editor");
            key(&mut app, KeyCode::Enter);
        }
        type_text(&mut app, "yes");
        let task = key(&mut app, KeyCode::Enter).unwrap();
        let refresh = run_task(&mut app, &fixture, task).unwrap();
        run_task(&mut app, &fixture, refresh);
        assert_eq!(fixture.host.home.join(".editor").exists(), !remove);
        assert!(!fixture.host.home.join(".editor").is_symlink());
        assert_eq!(
            fs::read_to_string(fixture.root.join("source")).unwrap(),
            "ordinary settings\n"
        );
        assert!(app.data.as_ref().unwrap().files.is_empty());
        assert!(app.result.as_ref().unwrap().contains("Transaction ID:"));
    }
}

#[test]
fn candidate_search_retains_raw_selection_and_does_not_change_manual_drafts() {
    let fixture = Fixture::new(false);
    fs::write(fixture.host.home.join(".aaa"), "ordinary\n").unwrap();
    fs::write(fixture.host.home.join(".q?config"), "ordinary\n").unwrap();
    let mut app = onboarding_app(&fixture, false);
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    run_task(&mut app, &fixture, task);
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "q?");
    key(&mut app, KeyCode::Esc);
    let selected = app.candidate_selected;
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    run_task(&mut app, &fixture, task);
    assert_eq!(app.candidate_selected, selected);
    assert_eq!(app.visible_file_rows(), [selected]);
    key(&mut app, KeyCode::Enter);
    assert!(app.shell.adoption.target.ends_with(".q?config"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Delete);
    assert_eq!(app.visible_file_rows().len(), 2);
    key(&mut app, KeyCode::Char('o'));
    assert!(app.shell.adoption.target.ends_with(".q?config"));
    assert!(!fixture.host.state_dir.exists() && !fixture.host.config_dir.exists());
}

#[test]
fn failed_profile_union_leaves_prior_scope_and_invalid_inventory_is_not_empty_success() {
    let fixture = Fixture::new(false);
    fs::write(
        fixture.root.join("dort.toml"),
        "schema=1\n[profiles]\navailable=['extra']\n",
    )
    .unwrap();
    let mut app = onboarding_app(&fixture, false);
    app.repository = Some(fixture.root.clone());
    let task = key(&mut app, KeyCode::Char('t')).unwrap();
    run_task(&mut app, &fixture, task);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter); // extra
    // Revalidate on Apply: a later manifest edit makes the requested name unknown.
    fs::write(fixture.root.join("dort.toml"), "schema=1\n").unwrap();
    key(&mut app, KeyCode::Down);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.is_some() && app.requested_profiles.is_empty());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc); // picker -> destination
    fs::write(fixture.root.join("dort.toml"), "bad manifest").unwrap();
    let data = actions::load(&fixture.host, Some(&fixture.root)).unwrap();
    assert!(data.files_error.is_some() && data.history_error.is_none());
    app.complete(Ok(Output::Data(data)));
    key(&mut app, KeyCode::Char('2'));
    assert!(super::tests::render(&app, 80, 24).contains("Unavailable:"));
    assert!(!fixture.host.state_dir.exists() && !fixture.host.config_dir.exists());
}

#[test]
fn structural_check_reports_dependencies_and_capability_limits_without_observing_sources() {
    let fixture = Fixture::new(false);
    fs::write(
        fixture.root.join("dort.toml"),
        include_str!("../../../../examples/dort.toml"),
    )
    .unwrap();
    let output = actions::perform(
        Task::Check {
            manifest: Some(fixture.root.join("dort.toml")),
            requested: vec!["dev".into()],
        },
        &fixture.host,
        Some(fixture.root.join("invalid")),
        |_| Ok(()),
    )
    .unwrap();
    let Output::Checked { report, standalone } = &output else {
        panic!("structural report")
    };
    assert!(*standalone && report.selected_tools == 3 && report.selected_files == 3);
    assert_eq!(report.tool_dependency_order, ["git", "node", "pi"]);
    assert!(
        report
            .unsupported_capabilities
            .iter()
            .any(|s| s.contains("Template"))
    );
    assert!(
        report
            .unsupported_capabilities
            .iter()
            .any(|s| s.contains("Secret"))
    );
    assert!(!report.filesystem_and_tools_inspected);
    let mut app = App::new(false);
    app.complete(Ok(output));
    let screen = super::tests::render(&app, 160, 40);
    assert!(screen.contains("live files and installed tools were not inspected"));
    assert!(screen.contains("tool inventory and installation"));
    assert!(!fixture.root.join("templates").exists() && !fixture.root.join("secrets").exists());
    assert!(!fixture.host.config_dir.exists() && !fixture.host.state_dir.exists());
}

#[test]
fn malformed_manifest_errors_never_render_source_lines() {
    let fixture = Fixture::new(false);
    fs::write(
        fixture.root.join("dort.toml"),
        "schema = 'private-body-marker'\n",
    )
    .unwrap();
    let data = actions::load(&fixture.host, Some(&fixture.root)).unwrap();
    assert!(data.overview.contains("Invalid TOML"));
    assert!(!data.overview.contains("private-body-marker"));
    assert!(
        !data
            .files_error
            .as_ref()
            .unwrap()
            .contains("private-body-marker")
    );
    let error = dort_core::Manifest::parse("schema = 'private-body-marker'")
        .err()
        .unwrap();
    assert!(
        !actions::exit_error(error)
            .to_string()
            .contains("private-body-marker")
    );
    let mut app = onboarding_app(&fixture, false);
    let task = Task::Check {
        manifest: Some(fixture.root.join("dort.toml")),
        requested: vec![],
    };
    run_task(&mut app, &fixture, task);
    let screen = super::tests::render(&app, 120, 40);
    assert!(screen.contains("Invalid TOML") && !screen.contains("private-body-marker"));
    assert!(!fixture.host.state_dir.exists() && !fixture.host.config_dir.exists());
}

#[test]
fn malformed_configuration_does_not_hide_independent_history() {
    let fixture = Fixture::new(true);
    fs::write(fixture.host.local_config_path(), "not a local config").unwrap();
    let data = actions::load(&fixture.host, None).unwrap();
    assert!(data.overview.contains("Configuration unavailable:"));
    assert!(data.history_error.is_none());
    assert!(data.known_roots.is_empty());
}

#[test]
fn diagnostics_share_doctor_metadata_and_keep_configuration_failure_content_free() {
    let fixture = Fixture::new(false);
    let mut app = onboarding_app(&fixture, false);
    app.repository = Some(fixture.root.clone());
    key(&mut app, KeyCode::Char('4'));
    let task = key(&mut app, KeyCode::Char('j')).unwrap();
    run_task(&mut app, &fixture, task);
    let report = app.diagnostics.as_ref().unwrap();
    let local = dort_core::local::LocalConfig::load(&fixture.host).unwrap();
    assert_eq!(
        serde_json::to_value(report.registry.as_ref().unwrap()).unwrap(),
        serde_json::to_value(crate::services::doctor(&fixture.host, &local)).unwrap()
    );
    assert_eq!(*report.history.as_ref().unwrap(), (0, 0));
    assert!(!fixture.host.state_dir.exists() && !fixture.host.config_dir.exists());
    let text = super::tests::render(&app, 140, 45);
    assert!(text.contains("Diagnostics | Dort") && text.contains("Global write blockers: 0"));
    assert!(text.contains("Session file root:") && text.contains("Live files/tools not inspected"));
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    assert!(matches!(task, Task::Diagnostics));
    run_task(&mut app, &fixture, task);
    fs::create_dir_all(&fixture.host.config_dir).unwrap();
    fs::write(
        fixture.host.local_config_path(),
        "secret='private-config-body-marker' invalid",
    )
    .unwrap();
    run_task(&mut app, &fixture, Task::Diagnostics);
    let text = super::tests::render(&app, 140, 45);
    assert!(text.contains("Configuration unavailable:"));
    assert!(text.contains("unknown (not zero)"));
    assert!(!text.contains("private-config-body-marker"));
    assert_eq!(
        *app.diagnostics.as_ref().unwrap().history.as_ref().unwrap(),
        (0, 0)
    );
    assert!(!fixture.host.state_dir.exists());
    key(&mut app, KeyCode::Char('m'));
    assert!(!app.showing_diagnostics);
}

#[test]
fn all_repository_recovery_survives_bad_configuration_and_selected_root() {
    let fixture = Fixture::new(true);
    let other = fixture.root.parent().unwrap().join("other-repository");
    fs::create_dir_all(other.join(".git")).unwrap();
    fs::write(other.join("source"), "ordinary other settings\n").unwrap();
    fs::write(
        other.join("dort.toml"),
        "schema=1\n[[files]]\nid='other'\nsource='source'\ntarget='~/.other'\nmode='link'\n",
    )
    .unwrap();
    let lock = WriterLock::acquire(&fixture.host).unwrap();
    RepositoryRegistry::register(&fixture.host, &lock, &other).unwrap();
    drop(lock);
    for root in [&fixture.root, &other] {
        let mut app = App::new(false);
        app.repository = Some(root.clone());
        run_task(&mut app, &fixture, Task::Files(Request::Apply));
        let task = approve(&mut app);
        run_task(&mut app, &fixture, task);
    }
    fs::write(fixture.host.local_config_path(), "invalid config").unwrap();
    let mut app = App::new(false);
    app.repository = Some(fixture.root.join("missing"));
    app.tab = super::app::Tab::Recovery;
    run_task(&mut app, &fixture, Task::History);
    let rows = dort_core::journal::diagnose(&fixture.host).unwrap();
    assert_eq!(app.data.as_ref().unwrap().history.len(), 2);
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].repository_id, rows[1].repository_id);
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, &rows[1].transaction_id);
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.history_selection().unwrap().transaction_id,
        rows[1].transaction_id
    );
    assert!(super::tests::render(&app, 140, 40).contains(&rows[1].repository_id));
    assert!(
        matches!(key(&mut app, KeyCode::Char('b')), Some(Task::Rollback(r,t)) if r == rows[1].repository_id && t == rows[1].transaction_id)
    );
}

#[test]
fn recovery_lock_failure_needs_new_preview_and_confirmation_not_replayed_approval() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.repository = Some(fixture.root.clone());
    run_task(&mut app, &fixture, Task::Files(Request::Apply));
    let task = approve(&mut app);
    run_task(&mut app, &fixture, task);
    app.tab = super::app::Tab::Recovery;
    run_task(&mut app, &fixture, Task::History);
    key(&mut app, KeyCode::Enter);
    let task = key(&mut app, KeyCode::Char('b')).unwrap();
    run_task(&mut app, &fixture, task);
    let text = &app.preview.as_ref().unwrap().text;
    assert!(text.contains("Journal head:") && text.contains("Inverse paths:"));
    let task = approve(&mut app);
    let lock = WriterLock::acquire(&fixture.host).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.error.as_ref().unwrap().contains("partial effects"));
    assert!(app.preview.is_none() && app.confirmation.is_none() && app.retry.is_none());
    assert!(fixture.host.home.join(".editor").is_symlink());
    drop(lock);
    let task = key(&mut app, KeyCode::Char('h')).unwrap();
    run_task(&mut app, &fixture, task);
    let task = key(&mut app, KeyCode::Char('b')).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(app.preview.is_some() && app.confirmation.is_none());
    let task = approve(&mut app);
    run_task(&mut app, &fixture, task);
    assert!(!fixture.host.home.join(".editor").exists());
    assert!(fixture.root.join("source").exists());
    assert!(!dort_core::journal::diagnose(&fixture.host).unwrap()[0].blocks_writes());
}

#[test]
fn preview_read_retry_keeps_intent_but_reads_new_state_before_any_approval() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.repository = Some(fixture.root.clone());
    let old = fs::read(fixture.root.join("dort.toml")).unwrap();
    fs::write(
        fixture.root.join("dort.toml"),
        "private-value-marker invalid",
    )
    .unwrap();
    run_task(&mut app, &fixture, Task::Files(Request::Apply));
    assert!(app.error.is_some());
    assert!(!super::tests::render(&app, 120, 35).contains("private-value-marker"));
    fs::write(fixture.root.join("dort.toml"), old).unwrap();
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    assert!(matches!(task, Task::Files(Request::Apply)));
    run_task(&mut app, &fixture, task);
    assert!(app.preview.is_some() && app.confirmation.is_none());
    assert!(!fixture.host.home.join(".editor").exists());
    assert!(
        dort_core::journal::diagnose(&fixture.host)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn bootstrap_running_recovery_uses_contextual_preview_and_retains_published_artifacts() {
    use dort_core::{adoption::Boundary, bootstrap};
    for abandon in [false, true] {
        let fixture = Fixture::new(false);
        let root = fixture.root.parent().unwrap().join("interrupted-bootstrap");
        let git = fake_git(&fixture);
        let preview = bootstrap::preview(&fixture.host, &root, &git).unwrap();
        let lock = WriterLock::acquire(&fixture.host).unwrap();
        let result = bootstrap::execute(
            &fixture.host,
            &lock,
            bootstrap::Approval::from_preview(preview).unwrap(),
            &git,
            |event| {
                if matches!(event.boundary, Boundary::Mutation) {
                    Err(dort_core::Error::Invalid(
                        "stop after repository publication".into(),
                    ))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err() && root.exists());
        drop(lock);
        let mut app = App::new(false);
        app.tab = super::app::Tab::Recovery;
        run_task(&mut app, &fixture, Task::History);
        let task = key(&mut app, KeyCode::Char(if abandon { 'x' } else { 's' })).unwrap();
        run_task(&mut app, &fixture, task);
        let report = &app.preview.as_ref().unwrap().text;
        assert!(report.contains("Repository published: true") && report.contains("Journal head:"));
        assert!(report.contains("retains published repositories"));
        key(&mut app, KeyCode::Esc);
        assert!(root.exists());
        assert!(dort_core::journal::diagnose(&fixture.host).unwrap()[0].blocks_writes());
        let task = key(&mut app, KeyCode::Char(if abandon { 'x' } else { 's' })).unwrap();
        run_task(&mut app, &fixture, task);
        let task = approve(&mut app);
        run_task(&mut app, &fixture, task);
        assert!(root.join("dort.toml").exists());
        let history = dort_core::journal::diagnose(&fixture.host).unwrap();
        assert!(!history[0].blocks_writes());
        assert!(
            dort_core::local::LocalConfig::load(&fixture.host)
                .unwrap()
                .repositories
                .is_empty()
                == abandon
        );
        let transaction = fixture
            .host
            .state_dir
            .join("repositories")
            .join(&history[0].repository_id)
            .join("transactions")
            .join(&history[0].transaction_id);
        assert!(transaction.exists());
        run_task(&mut app, &fixture, Task::History);
        assert!(key(&mut app, KeyCode::Char('s')).is_none());
        assert!(key(&mut app, KeyCode::Char('x')).is_none());
    }
}

#[test]
fn selected_schema_one_rollback_withdraws_source_into_private_storage() {
    let fixture = Fixture::new(true);
    let target = fixture.host.home.join(".new-settings");
    fs::write(&target, "ordinary original settings\n").unwrap();
    let mut app = App::new(false);
    app.repository = Some(fixture.root.clone());
    run_task(
        &mut app,
        &fixture,
        Task::Files(crate::services::adopt_request(
            &target,
            "new-settings",
            std::path::Path::new("files/new-settings"),
        )),
    );
    let task = approve(&mut app);
    run_task(&mut app, &fixture, task);
    assert!(target.is_symlink());
    app.tab = super::app::Tab::Recovery;
    run_task(&mut app, &fixture, Task::History);
    assert_eq!(app.history_selection().unwrap().journal_schema, Some(1));
    let task = key(&mut app, KeyCode::Char('b')).unwrap();
    run_task(&mut app, &fixture, task);
    assert!(
        app.preview
            .as_ref()
            .unwrap()
            .text
            .contains("Rollback schema 1")
    );
    let task = approve(&mut app);
    run_task(&mut app, &fixture, task);
    assert!(!target.is_symlink());
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "ordinary original settings\n"
    );
    assert!(!fixture.root.join("files/new-settings").exists());
    let rows = dort_core::journal::diagnose(&fixture.host).unwrap();
    let transaction = fixture
        .host
        .state_dir
        .join("repositories")
        .join(&rows[0].repository_id)
        .join("transactions")
        .join(&rows[0].transaction_id);
    // The created source parent is op-0000; the source is withdrawn to op-0001.
    assert_eq!(
        fs::read_to_string(transaction.join("op-0001")).unwrap(),
        "ordinary original settings\n"
    );
}

#[test]
fn malformed_journal_tail_is_a_global_blocker_not_a_recovery_permission() {
    let fixture = Fixture::new(true);
    let mut app = App::new(false);
    app.repository = Some(fixture.root.clone());
    run_task(&mut app, &fixture, Task::Files(Request::Apply));
    let task = approve(&mut app);
    run_task(&mut app, &fixture, task);
    let rows = dort_core::journal::diagnose(&fixture.host).unwrap();
    let transaction = fixture
        .host
        .state_dir
        .join("repositories")
        .join(&rows[0].repository_id)
        .join("transactions")
        .join(&rows[0].transaction_id);
    fs::write(
        transaction.join("999999.json"),
        "deliberately malformed fixture tail",
    )
    .unwrap();
    app.tab = super::app::Tab::Recovery;
    run_task(&mut app, &fixture, Task::History);
    assert!(app.history_selection().unwrap().problem.is_some());
    assert!(app.history_selection().unwrap().blocks_writes());
    for c in ['b', 's', 'x'] {
        assert!(key(&mut app, KeyCode::Char(c)).is_none());
    }
    assert!(app.preview.is_none() && app.confirmation.is_none());
    assert!(fixture.host.home.join(".editor").is_symlink());
    assert_eq!(
        fs::read_to_string(transaction.join("999999.json")).unwrap(),
        "deliberately malformed fixture tail"
    );
}
