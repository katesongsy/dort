use super::{click_label, focus_form, render};
use crate::tui::{
    actions::{self, Task},
    app::{App, Tab},
    configuration::{Kind, Request},
    shell::{Action, Form, Overlay},
};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    manifest_edit::{Change, FileChange},
    planner,
    platform::{Architecture, Platform},
    state::WriterLock,
    tools::declaration,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs, path::PathBuf};

const MANIFEST: &str = "schema=1
[profiles]
available=['inactive']
[[tools]]
id='node-stable'
binary='node'
[tools.install]
provider='brew'
package='node@20'
[[tools]]
id='node-lts'
binary='node'
profiles=['inactive']
[tools.install]
provider='brew'
package='node'
[[tools]]
id='formatter'
binary='formatter'
profiles=['inactive']
[tools.install]
provider='npm'
package='formatter'
runtime_tool='node-lts'
[[files]]
id='editor'
source='editor.conf'
target='~/.editor'
mode='link'

profiles=['base', 'inactive']
requires=['node-stable']
";
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join("editor.conf"), "fixture").unwrap();
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        RepositoryRegistry::register(&host, &WriterLock::acquire(&host).unwrap(), &repo).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
        }
    }
    fn app(&self, tab: Tab, manifest: &str) -> App {
        fs::write(self.repo.join("dort.toml"), manifest).unwrap();
        let mut app = App::new(false);
        app.tab = tab;
        app.data = Some(actions::load(&self.host, Some(&self.repo)).unwrap());
        app
    }
    fn unchanged(&self, app: &App, manifest: &str) {
        assert!(app.preview.is_none() && app.receipt.is_none());
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            manifest
        );
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn press(app: &mut App, code: KeyCode) {
    assert!(key(app, code).is_none());
}
fn click(app: &mut App, label: &str) {
    assert!(click_label(app, 80, 24, label).is_none());
}
fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}
fn row_selected(app: &App, id: &str) -> bool {
    let screen = render(app, 80, 24);
    let row = screen
        .lines()
        .find(|line| line.contains(id))
        .unwrap_or_else(|| panic!("No row for {id} in:\n{screen}"));
    row.contains("Selected") || row.contains("Current")
}
fn review_adoption(app: &mut App) -> Vec<String> {
    press(app, KeyCode::Esc);
    let Some(Task::Files(planner::Request::Adopt { mapping })) = key(app, KeyCode::Enter) else {
        panic!("Enter outside field focus should review adoption");
    };
    mapping.profiles
}

#[test]
fn adoption_picks_inactive_profiles_with_keys_and_mouse_and_blank_means_base() {
    let f = Fixture::new();
    let mut app = f.app(Tab::Files, MANIFEST);
    assert_eq!(app.data.as_ref().unwrap().profiles, ["base"]);
    app.shell.overlay = Some(Overlay::Form(Form::Adopt));
    focus_form(&mut app);
    type_text(&mut app, "~/.other");
    press(&mut app, KeyCode::Tab);
    type_text(&mut app, "other");
    press(&mut app, KeyCode::Tab);
    type_text(&mut app, "other.conf");
    press(&mut app, KeyCode::Tab);
    assert!(render(&app, 80, 24).contains("▾] Enter/Space"));
    type_text(&mut app, "invalid");
    press(&mut app, KeyCode::Enter);
    assert!(render(&app, 80, 24).contains("Blank uses base"));
    assert!(!row_selected(&app, "inactive"));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char(' '));
    assert!(row_selected(&app, "inactive"));
    click(&mut app, "base");
    assert!(row_selected(&app, "base"));
    press(&mut app, KeyCode::Esc);
    assert_eq!(review_adoption(&mut app), ["base"]);

    click(&mut app, "Profiles (blank uses base)");
    click(&mut app, "inactive");
    click(&mut app, "Apply to draft");
    assert_eq!(review_adoption(&mut app), ["inactive"]);

    click(&mut app, "Profiles (blank uses base)");
    assert!(row_selected(&app, "inactive"));
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Enter);
    assert_eq!(review_adoption(&mut app), ["base"]);
    f.unchanged(&app, MANIFEST);
}

fn file_review(task: Option<Task>) -> FileChange {
    let Some(Task::Configuration(request)) = task else {
        panic!("Expected a file mapping review task");
    };
    let Request::Manifest(Change::File(change)) = *request else {
        panic!("Expected a file mapping change");
    };
    *change
}

#[test]
fn add_and_edit_mapping_use_declared_tools_and_preserve_saved_references() {
    let f = Fixture::new();
    let mut add = f.app(Tab::Files, MANIFEST);
    assert!(add.activate(Action::Configure(Kind::File)).is_none());
    focus_form(&mut add);
    type_text(&mut add, "new-file");
    press(&mut add, KeyCode::Tab);
    type_text(&mut add, "new.conf");
    press(&mut add, KeyCode::Tab);
    type_text(&mut add, "~/.new");
    click(&mut add, "Profiles (blank uses base)");
    click(&mut add, "inactive");
    press(&mut add, KeyCode::Enter);
    click(&mut add, "Required tools");
    click(&mut add, "formatter");
    press(&mut add, KeyCode::Enter);
    assert!(render(&add, 80, 24).contains("F2 More"));
    press(&mut add, KeyCode::F(2));
    click(&mut add, "Review changes");
    let new = file_review(key(&mut add, KeyCode::Enter));
    assert!(new.before.is_none());
    assert_eq!(new.mapping.id, "new-file");
    assert_eq!(new.mapping.profiles, ["inactive"]);
    assert_eq!(new.mapping.requires, ["formatter"]);
    f.unchanged(&add, MANIFEST);

    let mut edit = f.app(Tab::Files, MANIFEST);
    assert!(edit.activate(Action::Configure(Kind::EditFile)).is_none());
    click(&mut edit, "Required tools");
    assert!(row_selected(&edit, "node-stable"));
    click(&mut edit, "node-stable");
    press(&mut edit, KeyCode::Esc);
    click(&mut edit, "Profiles (blank uses base)");
    click(&mut edit, "inactive");
    press(&mut edit, KeyCode::Enter);
    press(&mut edit, KeyCode::Esc);
    let changed = file_review(key(&mut edit, KeyCode::Enter));
    assert_eq!(changed.before.unwrap().profiles, ["base", "inactive"]);
    assert_eq!(changed.mapping.profiles, ["base"]);
    assert_eq!(changed.mapping.requires, ["node-stable"]);
    f.unchanged(&edit, MANIFEST);
}

#[test]
fn declaration_choices_omit_self_keep_inactive_runtime_and_do_not_prepare_on_apply() {
    let f = Fixture::new();
    let mut app = f.app(Tab::Tools, MANIFEST);
    let tool = app
        .data
        .as_ref()
        .unwrap()
        .tools
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .iter()
        .find(|tool| tool.id == "formatter")
        .unwrap()
        .clone();
    app.tools_view
        .change_declaration(&tool, declaration::Change::Edit);
    click(&mut app, "Profiles (blank = base)");
    assert!(row_selected(&app, "inactive"));
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::F(4));
    click(&mut app, "Required tools");
    assert!(!render(&app, 80, 24).contains("formatter"));
    click(&mut app, "node-lts");
    press(&mut app, KeyCode::Enter);
    click(&mut app, "npm runtime tool");
    assert!(row_selected(&app, "node-lts"));
    assert!(!render(&app, 80, 24).contains("formatter"));
    click(&mut app, "node-stable");
    press(&mut app, KeyCode::Esc);
    click(&mut app, "npm runtime tool");
    assert!(row_selected(&app, "node-lts"));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    f.unchanged(&app, MANIFEST);

    press(&mut app, KeyCode::Esc);
    let Some(Task::ToolDeclaration(request)) = key(&mut app, KeyCode::Enter) else {
        panic!("Enter outside field focus should review the declaration");
    };
    assert_eq!(request.change, Some(declaration::Change::Edit));
    assert_eq!(request.tool.profiles, ["base"]);
    assert_eq!(request.tool.requires, ["node-lts"]);
    assert_eq!(
        request.tool.install.runtime_tool.as_deref(),
        Some("node-stable")
    );
}

#[test]
fn missing_options_explain_why_and_keep_saved_ids_and_ctrl_shortcuts() {
    let f = Fixture::new();
    let mut broken = f.app(Tab::Files, "malformed [");
    broken.shell.overlay = Some(Overlay::Form(Form::Adopt));
    broken.shell.adoption.profiles = "old-profile".into();
    click(&mut broken, "Profiles (blank uses base)");
    let screen = render(&broken, 80, 24);
    assert!(screen.contains("Profiles unavailable"));
    assert!(screen.contains("old-profile"));
    press(&mut broken, KeyCode::Enter);
    assert_eq!(broken.shell.adoption.profiles, "old-profile");
    press(&mut broken, KeyCode::F(3));
    press(&mut broken, KeyCode::Char(' '));
    assert!(render(&broken, 80, 24).contains("Draft details"));
    press(&mut broken, KeyCode::Esc);
    assert!(
        broken
            .key(
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
                80,
                24
            )
            .is_none()
    );
    assert!(broken.shell.adoption.profiles.is_empty());

    let mut empty = f.app(Tab::Files, "schema=1\n");
    assert!(empty.activate(Action::Configure(Kind::File)).is_none());
    click(&mut empty, "Required tools");
    assert!(render(&empty, 80, 24).contains("No tools declared in this repository."));
    press(&mut empty, KeyCode::Char(' '));
    press(&mut empty, KeyCode::Enter);
    assert!(render(&empty, 80, 24).contains("Required tools"));
    f.unchanged(&empty, "schema=1\n");
}
