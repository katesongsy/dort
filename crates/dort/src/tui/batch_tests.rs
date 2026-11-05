use super::{
    actions::{self, Output, Pending, Task},
    app::{App, Tab},
    tests::render,
};
use dort_core::{
    context::HostContext,
    journal,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::{fs, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
}
impl Fixture {
    fn new(managed: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let home = base.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let mut manifest = "schema=1\n".to_owned();
        fs::create_dir_all(host.home.join(".config/editor")).unwrap();
        for name in ["only-a", "only-b", "only-c"] {
            let path = host.home.join(".config/editor").join(name);
            fs::write(path, "ordinary harmless settings\n").unwrap();
            if managed {
                fs::write(repo.join(name), "ordinary source settings\n").unwrap();
                manifest += &format!(
                    "[[files]]\nid='{name}'\nsource='{name}'\ntarget='~/.config/editor/{name}'\nmode='link'\n"
                );
            }
        }
        fs::write(repo.join("dort.toml"), manifest).unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
        }
    }
    fn app(&self, dry: bool) -> App {
        let mut app = App::new(dry);
        app.repository = Some(self.repo.clone());
        app.complete(Ok(Output::Data(
            actions::load(&self.host, Some(&self.repo)).unwrap(),
        )));
        app.tab = Tab::Files;
        app
    }
    fn run(&self, app: &mut App, task: Task) -> Option<Task> {
        app.start(matches!(task, Task::Execute(_)));
        let out = actions::perform(task, &self.host, Some(self.repo.clone()), |_| Ok(()))
            .map_err(|e| actions::diagnostic(&e));
        app.complete(out)
    }
}
const SIZE: (u16, u16) = (48, 14);
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), SIZE.0, SIZE.1)
}
fn text(app: &mut App, s: &str) {
    for c in s.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}
fn clear(app: &mut App) {
    if let Some(c) = &app.confirmation {
        for _ in 0..c.input.chars().count() {
            assert!(key(app, KeyCode::Backspace).is_none());
        }
    } else {
        assert!(
            app.key(
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                SIZE.0,
                SIZE.1
            )
            .is_none()
        );
    }
}
fn select(app: &mut App, name: &str) {
    for _ in 0..20 {
        key(app, KeyCode::Up);
    }
    for _ in 0..30 {
        if app.inventory.group.is_none() {
            let matches = if app.discovering {
                app.discovery
                    .as_ref()
                    .and_then(|d| d.report.candidates.get(app.candidate_selected))
                    .is_some_and(|c| c.entry.path.file_name().is_some_and(|p| p == name))
            } else {
                app.data
                    .as_ref()
                    .and_then(|d| d.files.get(app.selected))
                    .is_some_and(|r| r.0 == name)
            };
            if matches {
                return;
            }
        }
        key(app, KeyCode::Down);
    }
    panic!("file not keyboard reachable: {name}");
}
fn click(app: &mut App, label: &str, offset: u16) -> Option<Task> {
    let screen = render(app, SIZE.0, SIZE.1);
    let (x, y) = screen
        .lines()
        .enumerate()
        .find_map(|(y, row)| {
            row.find(label)
                .map(|x| (row[..x].chars().count() as u16 + offset, y as u16))
        })
        .expect("visible mouse target");
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        SIZE.0,
        SIZE.1,
    )
}
fn approve(app: &mut App) -> Task {
    assert!(key(app, KeyCode::Enter).is_none());
    text(app, "yes");
    key(app, KeyCode::Enter).expect("literal approval")
}
#[test]
fn tree_folds_marks_hidden_items_and_batch_editor_retains_real_fields_at_floor() {
    let f = Fixture::new(false);
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, task);
    select(&mut app, "only-a");
    assert!(key(&mut app, KeyCode::Char('x')).is_none());
    select(&mut app, "only-b");
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(app.inventory.candidate_marks.len(), 2);
    key(&mut app, KeyCode::Char('h'));
    assert!(app.inventory.group.is_some());
    assert!(render(&app, 48, 14).contains("2 marked (2 hidden)"));
    key(&mut app, KeyCode::Char('l'));
    select(&mut app, "only-a");
    key(&mut app, KeyCode::Char('/'));
    text(&mut app, "only-a");
    key(&mut app, KeyCode::Enter);
    assert!(render(&app, 48, 14).contains("2 marked (1 hidden)"));
    assert!(key(&mut app, KeyCode::Char('E')).is_none());
    let screen = render(&app, 48, 14);
    assert!(screen.contains("File 1/2") && screen.contains("TARGET"));
    key(&mut app, KeyCode::Tab);
    clear(&mut app);
    text(&mut app, "custom-a");
    key(&mut app, KeyCode::Tab);
    clear(&mut app);
    text(&mut app, "files/custom-a");
    // Tab traverses only local controls. Next is an explicit button.
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert!(render(&app, 48, 14).contains("File 2/2"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('E'));
    assert_eq!(app.batch_editor.as_ref().unwrap().drafts[0].id, "custom-a");
    for _ in 0..5 {
        key(&mut app, KeyCode::Tab);
    }
    let task = click(&mut app, "Review batch", 0).expect("explicit real review button");
    f.run(&mut app, task);
    let Pending::Batch(p) = &app.preview.as_ref().unwrap().pending else {
        panic!("combined plan expected")
    };
    assert_eq!(p.items.len(), 2);
    assert!(!p.has_blockers());
    assert!(p.items.iter().any(|p| p.files[0].id == "custom-a"));
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
    key(&mut app, KeyCode::Down);
    assert_eq!(app.preview.as_ref().unwrap().pending.review_len(), Some(2));
    let task = approve(&mut app);
    let refresh = f.run(&mut app, task).unwrap();
    f.run(&mut app, refresh);
    assert!(!app.batch_editor.as_ref().unwrap().open);
    assert!(f.host.home.join(".config/editor/only-a").is_symlink());
    assert!(f.host.home.join(".config/editor/only-b").is_symlink());
    assert!(!f.host.home.join(".config/editor/only-c").is_symlink());
    assert_eq!(journal::diagnose(&f.host).unwrap().len(), 1);
    assert!(f.repo.join("files/custom-a").is_file());
}
#[test]
fn selected_apply_keep_delete_are_combined_and_every_delete_id_then_yes_is_required() {
    for operation in ['K', 'D'] {
        let f = Fixture::new(true);
        let mut app = f.app(false);
        select(&mut app, "only-a");
        key(&mut app, KeyCode::Char('x'));
        select(&mut app, "only-b");
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Char('/'));
        text(&mut app, "only-a");
        key(&mut app, KeyCode::Enter);
        let task = key(&mut app, KeyCode::Char('A')).unwrap();
        f.run(&mut app, task);
        assert_eq!(app.preview.as_ref().unwrap().pending.review_len(), Some(2));
        let task = approve(&mut app);
        let refresh = f.run(&mut app, task).unwrap();
        f.run(&mut app, refresh);
        assert!(!f.host.home.join(".config/editor/only-c").is_symlink());
        key(&mut app, KeyCode::Esc);
        let task = key(&mut app, KeyCode::Char(operation)).unwrap();
        f.run(&mut app, task);
        assert!(key(&mut app, KeyCode::Enter).is_none());
        if operation == 'D' {
            assert!(render(&app, 48, 14).contains("1/2"));
            text(&mut app, "yes");
            assert!(key(&mut app, KeyCode::Enter).is_none());
            assert!(app.confirmation.as_ref().unwrap().batch_acks.is_empty());
            clear(&mut app);
            text(&mut app, "only-a");
            assert!(key(&mut app, KeyCode::Enter).is_none());
            assert_eq!(
                app.confirmation.as_ref().unwrap().batch_acks,
                vec!["only-a"]
            );
            text(&mut app, "only-b");
            app.resize(47, 14);
            assert!(app.confirmation.is_none());
            app.resize(48, 14);
            key(&mut app, KeyCode::Enter);
            assert!(app.confirmation.as_ref().unwrap().batch_acks.is_empty());
            text(&mut app, "only-a");
            render(&app, 48, 14);
            assert!(
                app.mouse(
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: 10,
                        row: 9,
                        modifiers: KeyModifiers::NONE
                    },
                    48,
                    14
                )
                .is_none()
            );
            assert!(app.confirmation.as_ref().unwrap().batch_acks.is_empty());
            key(&mut app, KeyCode::Enter);
            text(&mut app, "only-b");
            key(&mut app, KeyCode::Enter);
            assert!(render(&app, 48, 14).contains("literal yes"));
        }
        text(&mut app, "yes");
        let task = key(&mut app, KeyCode::Enter).unwrap();
        let refresh = f.run(&mut app, task).unwrap();
        f.run(&mut app, refresh);
        for id in ["only-a", "only-b"] {
            let path = f.host.home.join(".config/editor").join(id);
            if operation == 'K' {
                assert!(!path.is_symlink());
                assert!(path.is_file());
            } else {
                assert!(!path.exists());
            }
            assert!(f.repo.join(id).is_file());
        }
        let history = journal::diagnose(&f.host).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|d| d.journal_schema == Some(4)));
        let manifest =
            dort_core::Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap())
                .unwrap();
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].id, "only-c");
        // Schema-4 rollback is offered by the real Recovery action guard.
        key(&mut app, KeyCode::Char('3'));
        let index = app
            .data
            .as_ref()
            .unwrap()
            .history
            .iter()
            .position(|d| d.transaction_id == history.last().unwrap().transaction_id)
            .unwrap();
        for _ in 0..index {
            key(&mut app, KeyCode::Down);
        }
        assert!(app.page_actions().contains(&super::shell::Action::Rollback));
    }
}
#[test]
fn mouse_tree_controls_use_rendered_geometry_and_never_open_previews_or_approve() {
    let f = Fixture::new(false);
    let mut app = f.app(false);
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, task);
    select(&mut app, "only-a");
    assert!(click(&mut app, "[ ]", 1).is_none());
    assert_eq!(app.inventory.candidate_marks.len(), 1);
    key(&mut app, KeyCode::Char('h'));
    assert!(render(&app, 48, 14).contains("[>]"));
    assert!(click(&mut app, "[>]", 1).is_none());
    select(&mut app, "only-b");
    assert!(click(&mut app, "[ ] only-b", 1).is_none());
    assert_eq!(app.inventory.candidate_marks.len(), 2);
    assert!(app.preview.is_none() && app.confirmation.is_none());
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
    // Group toggling explicitly marks matching regular descendants, including folded rows.
    key(&mut app, KeyCode::Char('h'));
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(app.inventory.candidate_marks.len(), 3);
    key(&mut app, KeyCode::Char('x'));
    assert!(app.inventory.candidate_marks.is_empty());
}
#[test]
fn blocked_item_never_silently_skips_and_dry_run_cannot_approve_batch() {
    let f = Fixture::new(true);
    let mut app = f.app(false);
    select(&mut app, "only-a");
    key(&mut app, KeyCode::Char('x'));
    select(&mut app, "only-b");
    key(&mut app, KeyCode::Char('x'));
    fs::remove_file(f.host.home.join(".config/editor/only-b")).unwrap();
    std::os::unix::fs::symlink("/foreign", f.host.home.join(".config/editor/only-b")).unwrap();
    let task = key(&mut app, KeyCode::Char('A')).unwrap();
    f.run(&mut app, task);
    assert!(app.preview.as_ref().unwrap().pending.blocked());
    assert_eq!(app.preview.as_ref().unwrap().pending.review_len(), Some(2));
    key(&mut app, KeyCode::Enter);
    assert!(app.confirmation.is_none());
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
    let mut dry = f.app(true);
    select(&mut dry, "only-a");
    key(&mut dry, KeyCode::Char('x'));
    let task = key(&mut dry, KeyCode::Char('A')).unwrap();
    f.run(&mut dry, task);
    key(&mut dry, KeyCode::Enter);
    text(&mut dry, "yes");
    assert!(key(&mut dry, KeyCode::Enter).is_none());
    assert!(dry.confirmation.is_none());
    assert!(journal::diagnose(&f.host).unwrap().is_empty());
}
