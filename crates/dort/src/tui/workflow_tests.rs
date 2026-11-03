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
    app.start(matches!(task, Task::Execute(_)));
    let result = actions::perform(task, &fixture.host, app.repository.clone(), |_| Ok(()))
        .map_err(|error| error.to_string());
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
fn malformed_configuration_does_not_hide_independent_history() {
    let fixture = Fixture::new(true);
    fs::write(fixture.host.local_config_path(), "not a local config").unwrap();
    let data = actions::load(&fixture.host, None).unwrap();
    assert!(data.overview.contains("Configuration unavailable:"));
    assert!(data.history_error.is_none());
    assert!(data.known_roots.is_empty());
}
