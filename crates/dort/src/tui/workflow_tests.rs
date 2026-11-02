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
    assert!(app.confirmation.is_some());
    let screen = super::tests::render(&app, 48, 14);
    assert_eq!(modal(&screen), expected.replace("\n>\n", "\n> yes\n"));
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
