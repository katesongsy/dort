use super::input::Target;
use super::{
    actions::{Task, perform},
    app::{App, Tab},
    tests::{click_label, destination, key, render, target_position},
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};
use std::fs;

#[test]
fn managed_template_preview_opens_by_key_and_click_and_scrolls_without_approval() {
    for (width, height) in [(48, 14), (80, 24), (120, 32)] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        let home = root.join("home");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&home).unwrap();
        let host = HostContext::new(
            home.clone(),
            None,
            None,
            Platform::Linux,
            Architecture::X86_64,
        )
        .unwrap();
        let manifest = format!(
            r#"schema = 1
[[files]]
id = "editor-settings"
mode = "template"
source = "editor.tmpl"
target = "~/settings"
inputs = {{ theme = "dark", padding = "{}" }}
"#,
            "\\n".repeat(14_000)
        );
        fs::write(repo.join("dort.toml"), &manifest).unwrap();
        let template = "theme={{ theme }}\n{{ padding }}{{ padding }}{{ padding }}{{ padding }}{{ padding }}last = \x1b[31mTAIL\n";
        fs::write(repo.join("editor.tmpl"), template).unwrap();
        fs::write(home.join("settings"), "destination-bytes-must-stay-private").unwrap();
        let label = if width < 80 {
            "Preview generated"
        } else {
            "Preview generated file"
        };
        let mut app = App::new(false);
        app.repository = Some(repo.clone());
        let run = |app: &mut App, task| {
            app.start(false);
            app.complete_task(perform(task, &host, Some(repo.clone()), |_| Ok(())));
        };
        run(&mut app, Task::Refresh);
        destination(&mut app, Tab::Files, width, height);
        let screen = render(&app, width, height);
        assert!(screen.contains(label), "{screen}");
        assert!(
            !screen.contains("Edit file mapping") && !screen.contains("Edit mapping"),
            "{screen}"
        );
        assert!(
            super::discover_content::next(&mut app).is_none(),
            "template destination must not be read"
        );
        let open = if width < 80 {
            KeyCode::Enter
        } else {
            KeyCode::Char('v')
        };
        let task = app.key(key(open), width, height).unwrap();
        assert!(matches!(task, Task::PreviewGenerated(_)));
        run(&mut app, task);
        let screen = render(&app, width, height);
        assert!(screen.contains("Generated file preview"), "{screen}");
        assert!(
            screen.contains("Inputs from dort.toml: padding, theme"),
            "{screen}"
        );
        assert!(screen.contains("No files written."), "{screen}");
        assert!(
            !screen.contains("destination-bytes") && !screen.contains("Confirm"),
            "{screen}"
        );
        assert!(app.key(key(KeyCode::Char('y')), width, height).is_none());
        assert!(app.key(key(KeyCode::End), width, height).is_none());
        let screen = render(&app, width, height);
        assert!(
            screen.contains("last = \\u{1b}[31mTAIL") && screen.contains("End of rendered output"),
            "{screen}"
        );
        assert!(!screen.contains('\x1b'), "{screen}");
        assert!(app.key(key(KeyCode::Home), width, height).is_none());
        render(&app, width, height);
        assert!(
            app.mouse(
                MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: width / 2,
                    row: height / 2,
                    modifiers: KeyModifiers::NONE,
                },
                width,
                height
            )
            .is_none()
        );
        let screen = render(&app, width, height);
        assert!(
            !screen.contains("No files written."),
            "wheel should scroll:\n{screen}"
        );
        assert!(app.key(key(KeyCode::Esc), width, height).is_none());
        let task = click_label(&mut app, width, height, label).unwrap();
        run(&mut app, task);
        render(&app, width, height);
        target_position(&app, width, height, |target| matches!(target, Target::Back));
        assert!(click_label(&mut app, width, height, "Esc Back").is_none());
        assert!(render(&app, width, height).contains(label));
        assert_eq!(
            fs::read_to_string(home.join("settings")).unwrap(),
            "destination-bytes-must-stay-private"
        );
        assert_eq!(
            fs::read_to_string(repo.join("editor.tmpl")).unwrap(),
            template
        );
        assert_eq!(
            fs::read_to_string(repo.join("dort.toml")).unwrap(),
            manifest
        );
        assert_eq!(fs::read_dir(&home).unwrap().count(), 1);
    }
}

struct Workshop {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: std::path::PathBuf,
    app: App,
    width: u16,
    height: u16,
}

impl Workshop {
    fn new(width: u16, height: u16) -> Self {
        use dort_core::{local::RepositoryRegistry, state::WriterLock};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        let home = root.join("home");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir(&home).unwrap();
        let host = HostContext::new(
            home,
            Some(root.join("config")),
            Some(root.join("state")),
            Platform::Linux,
            Architecture::X86_64,
        )
        .unwrap();
        fs::write(repo.join("dort.toml"), "schema = 1\n").unwrap();
        RepositoryRegistry::register(&host, &WriterLock::acquire(&host).unwrap(), &repo).unwrap();
        let mut w = Self {
            _temp: temp,
            host,
            repo,
            app: App::new(false),
            width,
            height,
        };
        w.run(Task::Refresh);
        destination(&mut w.app, Tab::Files, width, height);
        w
    }

    fn run(&mut self, mut task: Task) {
        loop {
            self.app.start(matches!(task, Task::Execute(_)));
            let output = perform(task, &self.host, None, |_| Ok(()));
            if let Some(next) = self.app.complete_task(output) {
                task = next;
            } else {
                break;
            }
        }
    }

    fn press(&mut self, code: KeyCode) {
        if let Some(task) = self.app.key(key(code), self.width, self.height) {
            self.run(task);
        }
    }

    fn paste(&mut self, text: &str) {
        self.app.paste_generated(text, self.width, self.height);
    }

    fn screen(&mut self) -> String {
        render(&self.app, self.width, self.height)
    }

    fn review(&self) -> &str {
        &self.app.preview.as_ref().expect("expected review").text
    }

    fn approve(&mut self) {
        let task = super::tests::approve_ready(&mut self.app, self.width, self.height).unwrap();
        self.run(task);
    }

    fn manifest(&self) -> String {
        fs::read_to_string(self.repo.join("dort.toml")).unwrap()
    }
}

#[test]
fn blank_repository_can_create_cancel_save_preview_and_edit_generated_template() {
    for (width, height) in [(48, 14), (120, 32)] {
        let mut w = Workshop::new(width, height);
        let blank = w.manifest();
        assert!(w.screen().contains("s New generated"));
        if width > 80 {
            assert!(click_label(&mut w.app, width, height, "New generated file").is_none());
        } else {
            w.press(KeyCode::Char('s'));
        }
        let screen = w.screen();
        assert!(
            screen.contains("File ID")
                && screen.contains("F4 Continue")
                && screen.contains("Template"),
            "{screen}"
        );
        assert!(
            screen.contains(if width < 80 {
                "F5 Editor"
            } else {
                "F5 Open in editor"
            }),
            "{screen}"
        );
        for c in "prefs".chars() {
            w.press(KeyCode::Char(c));
        }
        w.press(KeyCode::Tab);
        w.paste("  ~/settings ");
        w.press(KeyCode::Tab);
        w.press(KeyCode::BackTab);
        w.paste(" ");
        w.press(KeyCode::Tab);
        w.paste("{broken={{ env.HOME }}");
        w.press(KeyCode::F(4));
        assert!(w.app.preview.is_none());
        assert!(w.screen().contains("Needs attention"));
        w.app.key(
            ratatui::crossterm::event::KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            width,
            height,
        );
        w.paste("theme=[{{ theme }}]");
        w.press(KeyCode::Enter);
        w.paste("accent=[{{ accent }}]\r\nrepeat={{ theme }}\n");
        if width > 80 {
            assert!(click_label(&mut w.app, width, height, "F4 Continue").is_none());
        } else {
            w.press(KeyCode::F(4));
        }
        let screen = w.screen();
        assert!(
            screen.contains("Input: accent") && screen.contains("Input: theme"),
            "{screen}"
        );
        assert_eq!(screen.matches("Input: theme").count(), 1, "{screen}");
        w.press(KeyCode::Tab);
        w.paste(" dark ");
        w.press(KeyCode::Esc);
        w.press(KeyCode::F(4));
        assert!(w.screen().contains(" dark"));
        w.press(KeyCode::Enter);
        assert!(
            w.review()
                .contains("theme=[ dark ]\naccent=[]\nrepeat= dark \n"),
            "{:?}",
            w.review()
        );
        assert!(w.review().contains("Update dort.toml only"));
        w.paste("y\n");
        w.press(KeyCode::Enter);
        assert_eq!(w.manifest(), blank);
        w.press(KeyCode::Esc);
        assert!(w.app.preview.is_none());
        assert!(w.screen().contains(" dark"));
        w.press(KeyCode::Enter);
        w.approve();
        assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 0);
        let saved = dort_core::Manifest::parse(&w.manifest()).unwrap();
        assert_eq!(
            saved.files[0]
                .inputs
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["accent", "theme"]
        );
        assert_eq!(saved.files[0].source, None);
        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('v'));
        assert!(w.screen().contains("Inline template in dort.toml"));
        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('e'));
        assert!(w.screen().contains("Edit generated file: prefs"));
        w.press(KeyCode::End);
        w.paste("enabled=true\n");
        w.press(KeyCode::F(4));
        w.paste("blue");
        w.press(KeyCode::Enter);
        assert!(w.review().contains("accent=[blue]"));
        assert!(w.review().contains("enabled=true\n"));
        w.approve();
        let edited = dort_core::Manifest::parse(&w.manifest()).unwrap();
        assert_eq!(edited.files[0].id, "prefs");
        assert!(
            edited.files[0]
                .template
                .as_deref()
                .unwrap()
                .ends_with("enabled=true\n")
        );
        assert_eq!(edited.files[0].inputs["accent"], "blue");
        assert_eq!(edited.files[0].inputs["theme"], " dark ");
        assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 0);
    }
}

#[test]
fn stale_generated_review_cannot_replace_a_new_manifest() {
    let mut w = Workshop::new(100, 26);
    w.press(KeyCode::Char('s'));
    w.paste("prefs");
    w.press(KeyCode::Tab);
    w.paste("~/settings");
    w.press(KeyCode::Tab);
    w.paste("theme=dark");
    w.press(KeyCode::F(4));
    assert!(w.review().contains("theme=dark"));
    let external = "schema = 1\n# External edit\n";
    fs::write(w.repo.join("dort.toml"), external).unwrap();
    w.approve();
    assert!(w.app.error.as_ref().unwrap().contains("Manifest changed"));
    assert_eq!(w.manifest(), external);
    assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 0);
}

#[test]
fn compact_managed_files_keep_new_generated_and_link_actions_reachable() {
    let mut w = Workshop::new(80, 24);
    fs::write(w.repo.join("shell"), "existing source").unwrap();
    let original = r#"schema = 1
[[files]]
id = "shell"
mode = "link"
source = "shell"
target = "~/shell"
"#;
    fs::write(w.repo.join("dort.toml"), original).unwrap();
    w.run(Task::Refresh);
    let screen = w.screen();
    assert!(screen.contains("s New generated file"), "{screen}");
    w.press(KeyCode::F(2));
    let items = &w.app.more.as_ref().unwrap().items;
    for label in ["Add file mapping", "Edit file mapping"] {
        assert!(
            screen.contains(label)
                || items
                    .iter()
                    .any(|item| item.action.label == label && item.reason.is_none()),
            "missing {label}: {screen}"
        );
    }
    w.press(KeyCode::Esc);
    click_label(&mut w.app, w.width, w.height, "New generated file");
    assert!(w.screen().contains("File ID"));
    assert_eq!(w.manifest(), original);
}

#[test]
fn editor_launches_by_key_and_mouse_and_only_updates_the_draft() {
    use super::template_editor::Edit;
    let mut w = Workshop::new(80, 24);
    let original = w.manifest();
    w.press(KeyCode::Char('s'));
    w.paste("prefs");
    w.press(KeyCode::Tab);
    w.paste("~/settings");
    w.press(KeyCode::Tab);
    w.paste("before\n");
    w.press(KeyCode::F(5));
    assert!(std::mem::take(&mut w.app.shell.open_template_editor));
    w.app
        .shell
        .finish_template_edit(Edit::with_config(None, None, "before\n").and_then(Edit::run));
    assert!(w.screen().contains("Set VISUAL or EDITOR"));
    assert!(w.screen().contains("before"));
    assert!(w.app.preview.is_none());
    assert_eq!(w.manifest(), original);

    assert!(click_label(&mut w.app, w.width, w.height, "F5 Open in editor").is_none());
    assert!(std::mem::take(&mut w.app.shell.open_template_editor));
    let result = Edit::with_config(
        Some("/bin/sh -c 'printf \"after\\n{{ theme }}\\n\" > \"$1\"' editor".into()),
        None,
        "before\n",
    )
    .and_then(Edit::run);
    w.app.shell.finish_template_edit(result);
    assert!(w.screen().contains("after"));
    assert!(w.app.preview.is_none());
    assert_eq!(w.manifest(), original);
    assert!(!w.host.home.join("settings").exists());

    w.press(KeyCode::F(4));
    assert!(w.screen().contains("Input: theme"));
    w.press(KeyCode::F(5));
    assert!(!w.app.shell.open_template_editor);
    w.paste("dark");
    w.press(KeyCode::Enter);
    assert!(w.review().contains("after\ndark\n"));
    assert_eq!(w.manifest(), original);
}

#[test]
fn first_deployment_requires_a_ready_review_and_a_separate_approval_at_narrow_widths() {
    for (width, height) in [(48, 14), (80, 24)] {
        let mut w = Workshop::new(width, height);
        let declaration = r#"schema = 1
[[files]]
id = "settings"
mode = "template"
source = "settings.tmpl"
target = "~/settings"
"#;
        let mut expected = "  text\r\n".repeat(40);
        expected.push_str(&format!("{}END\t\\\x1b  ", "x".repeat(67)));
        fs::write(w.repo.join("dort.toml"), declaration).unwrap();
        fs::write(w.repo.join("settings.tmpl"), &expected).unwrap();
        w.run(Task::Refresh);
        let target = w.host.home.join("settings");
        w.screen();
        w.press(KeyCode::Enter);
        let screen = w.screen();
        assert!(screen.contains("Review deployment"), "{screen}");
        if width == 80 {
            let task = click_label(&mut w.app, width, height, "Review deployment").unwrap();
            w.run(task);
        } else {
            w.press(KeyCode::Char('R'));
        }
        let screen = w.screen();
        assert!(
            screen.contains("Review first deployment: settings"),
            "{screen}"
        );
        assert!(screen.contains("Permissions: 0600"), "{screen}");
        assert!(!screen.contains("y Deploy generated file"), "{screen}");
        w.press(KeyCode::Char('y'));
        w.press(KeyCode::Enter);
        assert!(!target.exists());
        w.press(KeyCode::End);
        w.press(KeyCode::Char('y'));
        assert!(!target.exists(), "approval before the tail is drawn");
        let screen = w.screen();
        let joined = screen.lines().map(str::trim_end).collect::<String>();
        assert!(joined.contains("END\\t\\\\\\x1b\\x20\\x20"), "{screen}");
        assert!(screen.contains("End of rendered output"), "{screen}");
        assert!(screen.contains("Deploy generated file"), "{screen}");
        w.press(KeyCode::Esc);
        assert!(!target.exists());
        assert!(w.screen().contains("Generated file preview"));
        w.press(KeyCode::Char('R'));
        w.screen();
        w.press(KeyCode::End);
        w.screen();
        w.press(KeyCode::Char('y'));
        let screen = w.screen();
        assert!(screen.contains("Deployed generated file"), "{screen}");
        assert_eq!(fs::read(&target).unwrap(), expected.as_bytes());
        w.press(KeyCode::Char('y'));
        assert_eq!(w.manifest(), declaration);
        assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 1);
    }
}

#[test]
fn dry_run_and_blocked_deployment_never_offer_approval() {
    let mut w = Workshop::new(80, 24);
    fs::write(
        w.repo.join("dort.toml"),
        "schema=1\n[[files]]\nid='settings'\nmode='template'\ntarget='~/settings'\ntemplate='value'\n",
    )
    .unwrap();
    w.run(Task::Refresh);
    let target = w.host.home.join("settings");
    w.screen();
    w.press(KeyCode::Enter);
    let screen = w.screen();
    assert!(screen.contains("Generated file preview"), "{screen}");
    fs::write(&target, b"existing").unwrap();
    w.press(KeyCode::Char('R'));
    let screen = w.screen();
    assert!(screen.contains("already exists"), "{screen}");
    assert!(!screen.contains("Deploy generated file"), "{screen}");
    assert_eq!(fs::read(&target).unwrap(), b"existing");
    w.press(KeyCode::Esc);
    fs::remove_file(&target).unwrap();
    w.app.dry_run = true;
    w.screen();
    w.press(KeyCode::Char('R'));
    w.screen();
    w.press(KeyCode::End);
    let screen = w.screen();
    assert!(!screen.contains("y Deploy generated file"), "{screen}");
    w.press(KeyCode::Char('y'));
    assert!(!target.exists());
}
