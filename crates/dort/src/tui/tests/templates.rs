use super::{click_label, key, render};
use crate::tui::{
    actions::{Task, perform},
    app::App,
};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs, path::PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
    app: App,
}

impl Fixture {
    fn new(manifest: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        let home = root.join("home");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir(&home).unwrap();
        fs::write(repo.join("dort.toml"), manifest).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        RepositoryRegistry::register(&host, &WriterLock::acquire(&host).unwrap(), &repo).unwrap();
        let mut fixture = Self {
            _temp: temp,
            host,
            repo,
            app: App::new(false),
        };
        fixture.run(Task::Refresh);
        fixture.press(KeyCode::Char('f'));
        fixture
    }

    fn run(&mut self, task: Task) {
        self.app.start(matches!(&task, Task::Execute(_)));
        let follow =
            self.app
                .complete_task(perform(task, &self.host, Some(self.repo.clone()), |_| {
                    Ok(())
                }));
        assert!(follow.is_none());
    }

    fn key(&mut self, code: KeyCode) -> Option<Task> {
        self.app.key(key(code), 80, 24)
    }

    fn press(&mut self, code: KeyCode) {
        assert!(self.key(code).is_none());
    }

    fn run_key(&mut self, code: KeyCode) {
        let task = self.key(code).expect("key should start a task");
        self.run(task);
    }

    fn screen(&self) -> String {
        render(&self.app, 80, 24)
    }
}

#[test]
fn template_files_inventory_opens_by_click_and_previews_only_the_selected_source() {
    let manifest = "schema=1\n[[files]]\nid='legacy'\nmode='template'\nsource='legacy.txt'\ntarget='~/.legacy'\n\n[[files]]\nid='editor'\nmode='template'\nsource='templates/editor.jinja'\ntarget='~/.editor'\n\n[[files]]\nid='inline'\nmode='template'\ntemplate='inline sample'\ntarget='~/.inline'\n";
    let mut f = Fixture::new(manifest);
    fs::create_dir(f.repo.join("templates")).unwrap();
    fs::write(f.repo.join("templates/editor.jinja"), "editable-marker").unwrap();
    fs::write(f.repo.join("legacy.txt"), "legacy-marker").unwrap();
    f.run(Task::Refresh);
    f.press(KeyCode::Char('4'));
    let screen = f.screen();
    assert!(
        screen.contains("Template file") && screen.contains("In dort.toml"),
        "{screen}"
    );
    let task = click_label(&mut f.app, 80, 24, "5 Templates").unwrap();
    f.run(task);
    let screen = f.screen();
    assert!(
        screen.lines().any(|row| row.contains("legacy.txt")
            && row.contains(" 1 ")
            && row.contains("Read only")),
        "{screen}"
    );
    assert!(
        screen
            .lines()
            .any(|row| row.contains("templates/editor.jinja")
                && row.contains(" 1 ")
                && row.contains("Editable")),
        "{screen}"
    );
    assert!(!screen.contains("-marker"));
    f.run_key(KeyCode::Enter);
    let screen = f.screen();
    assert!(screen.contains("|legacy-marker|"));
    assert!(!screen.contains("editable-marker") && !screen.contains("Edit template file"));
    f.press(KeyCode::Enter);
    assert!(f.screen().contains("|legacy-marker|"));
    f.press(KeyCode::Esc);
    f.press(KeyCode::Down);
    f.run_key(KeyCode::Enter);
    assert!(f.screen().contains("|editable-marker|"));
}

#[test]
fn template_files_create_checks_folder_and_guards_dirty_draft_and_exact_review() {
    let mut f = Fixture::new("schema=1\n");
    f.run_key(KeyCode::Char('5'));
    f.press(KeyCode::Char('s'));
    f.app.paste_generated("new.jinja", 80, 24);
    f.press(KeyCode::Tab);
    let text = "setting=value\n".repeat(60);
    f.app.paste_generated(&text, 80, 24);
    f.press(KeyCode::Enter);
    assert!(!f.repo.join("templates").exists());
    f.press(KeyCode::Esc);
    f.press(KeyCode::Char('g'));
    assert!(f.screen().contains("Discard your changes?"));
    assert!(f.screen().contains("Git"));
    f.press(KeyCode::Esc);
    f.press(KeyCode::Char('q'));
    assert!(f.screen().contains("Discard your changes and quit Dort?"));
    f.press(KeyCode::Enter);
    f.run_key(KeyCode::Enter);
    assert!(f.screen().contains("Template folder is missing"));
    assert!(!f.repo.join("templates").exists());
    fs::create_dir(f.repo.join("templates")).unwrap();
    let path = f.repo.join("templates/new.jinja");
    f.run_key(KeyCode::Enter);
    assert!(f.screen().contains("Read remaining effects"));
    f.press(KeyCode::Char('y'));
    f.press(KeyCode::Enter);
    assert!(!path.exists());
    f.press(KeyCode::End);
    assert!(f.screen().contains("y Create template file"));
    f.run_key(KeyCode::Char('y'));
    assert_eq!(fs::read_to_string(&path).unwrap(), format!("{text}\n"));
    assert!(f.screen().contains("Created template file."));
    f.press(KeyCode::Esc);
    assert!(f.screen().contains("templates/new.jinja"));

    f.run_key(KeyCode::Enter);
    f.press(KeyCode::Enter);
    f.app.paste_generated("last", 80, 24);
    f.press(KeyCode::Esc);
    f.run_key(KeyCode::Enter);
    f.press(KeyCode::End);
    assert!(f.screen().contains("y Save template file"));
    f.run_key(KeyCode::Char('y'));
    assert_eq!(fs::read_to_string(&path).unwrap(), format!("{text}\nlast"));
    let screen = f.screen();
    assert!(
        screen.contains("Reviewed copy:") && screen.contains("Replaced entry:"),
        "{screen}"
    );
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "schema=1\n"
    );
}

#[test]
fn template_files_quit_from_a_focused_field_keeps_or_discards_the_draft() {
    let mut f = Fixture::new("schema=1\n");
    f.run_key(KeyCode::Char('5'));
    f.press(KeyCode::Char('s'));
    f.app.paste_generated("new.jinja", 80, 24);
    let quit = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(f.app.key(quit, 80, 24).is_none());
    assert!(f.screen().contains("Discard your changes and quit Dort?"));
    f.press(KeyCode::Esc);
    assert!(f.screen().contains("templates/new.jinja"));
    assert!(f.app.key(quit, 80, 24).is_none());
    f.press(KeyCode::Char('d'));
    assert!(f.app.quit);
    assert!(!f.repo.join("templates").exists());
}
