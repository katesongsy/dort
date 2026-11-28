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
fn generated_template_preview_opens_by_key_and_click_and_scrolls_without_approval() {
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
            "Enter Preview"
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
        assert!(render(&app, width, height).contains("4 Generated"));
        assert!(click_label(&mut app, width, height, "4 Generated").is_none());
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
        assert!(screen.contains("Syntax: Literal"), "{screen}");
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
        w.press(KeyCode::Char('4'));
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
        assert!(w.screen().contains("s New"));
        if width > 80 {
            assert!(click_label(&mut w.app, width, height, "s New generated file").is_none());
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
        assert!(!w.review().contains("Status:"));
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
        assert_eq!(saved.files[0].syntax, None);
        assert_eq!(saved.files[0].host_inputs, None);
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
fn jinja_draft_opts_in_and_keeps_explicit_values_and_host_choices_until_review() {
    for (width, height) in [(48, 14), (112, 28)] {
        let mut w = Workshop::new(width, height);
        let original = w.manifest();
        w.press(KeyCode::Char('s'));
        w.paste("prefs");
        w.press(KeyCode::Tab);
        w.paste("~/settings");
        w.press(KeyCode::Tab);
        let template = concat!(
            "theme=[{{ inputs.theme }}]\n",
            "{% if inputs.empty is defined %}empty=[{{ inputs.empty }}]\n{% endif %}",
            "{% if host.os == 'linux' and host.user is not defined %}",
            "{% for item in [host.os, host.arch, 'fixed'] %}{{ loop.index }}={{ item }};{% endfor %}\n",
            "{% endif %}",
            "{% if false %}{{ inputs.optional }}{{ host.hostname }}{% endif %}",
        );
        w.paste(template);
        w.press(KeyCode::F(4));
        let screen = w.screen();
        assert!(
            screen.contains("Literal") && screen.contains("Use {{ name }}"),
            "{screen}"
        );
        assert!(w.app.preview.is_none());
        if width < 80 {
            w.press(KeyCode::Tab);
            w.press(KeyCode::Enter);
            assert!(w.screen().contains("Choose template syntax"));
            w.press(KeyCode::Down);
        } else {
            assert!(click_label(&mut w.app, width, height, "Syntax").is_none());
            assert!(w.screen().contains("Choose template syntax"));
            assert!(click_label(&mut w.app, width, height, "Jinja").is_none());
        }
        w.press(KeyCode::Enter);
        assert!(w.screen().contains("Jinja"));
        w.press(KeyCode::F(4));
        assert!(w.screen().contains("Input: inputs.empty"));
        w.press(KeyCode::Enter);
        assert!(w.app.preview.is_none());
        assert_eq!(w.manifest(), original);
        assert!(w.screen().contains("Needs attention"));
        w.press(KeyCode::F(6));
        w.press(KeyCode::Tab);
        assert!(w.screen().contains("Input: inputs.optional"));
        w.press(KeyCode::Backspace);
        w.press(KeyCode::Tab);
        let color = " <&>\t";
        w.paste(color);
        w.press(KeyCode::Tab);
        assert!(w.screen().contains("host.os (referenced)"));
        if width < 80 {
            w.press(KeyCode::Char(' '));
        } else {
            assert!(click_label(&mut w.app, width, height, "host.os (referenced)").is_none());
        }
        w.press(KeyCode::Tab);
        w.press(KeyCode::Char(' '));
        w.press(KeyCode::Esc);
        w.press(KeyCode::Enter);
        w.press(KeyCode::Up);
        w.press(KeyCode::Esc);
        assert!(w.screen().contains("Jinja"));
        w.press(KeyCode::Enter);
        w.press(KeyCode::Up);
        w.press(KeyCode::Enter);
        assert!(w.screen().contains("Literal"));
        w.press(KeyCode::Enter);
        w.press(KeyCode::Down);
        w.press(KeyCode::Enter);
        assert!(w.screen().contains("Jinja"));
        w.press(KeyCode::F(4));
        w.press(KeyCode::Enter);
        let expected = format!("theme=[{color}]\nempty=[]\n1=linux;2=x86_64;3=fixed;\n");
        for text in [
            "Syntax: Jinja",
            "host.os = \"linux\" (this host)",
            "host.arch = \"x86_64\" (this host)",
            "inputs.empty = \"\" (dort.toml)",
            "inputs.theme = \" <&>\\t\" (dort.toml)",
        ] {
            assert!(w.review().contains(text), "{:?}", w.review());
        }
        assert!(!w.review().contains("inputs.optional"), "{:?}", w.review());
        assert_eq!(w.manifest(), original);
        let target = w.host.home.join("settings");
        assert!(!target.exists());
        w.approve();
        assert!(!target.exists());
        let saved = dort_core::Manifest::parse(&w.manifest()).unwrap();
        assert_eq!(
            saved.files[0].syntax,
            Some(dort_core::template::Dialect::Jinja)
        );
        assert_eq!(saved.files[0].template.as_deref(), Some(template));
        let keys: Vec<_> = saved.files[0]
            .host_inputs
            .unwrap()
            .iter()
            .map(|key| key.key())
            .collect();
        assert_eq!(keys, ["os", "arch"]);
        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('v'));
        assert!(w.screen().contains("Syntax: Jinja"));
        w.press(KeyCode::Char('R'));
        assert!(w.review().contains("host.os = \"linux\" (this host)"));
        assert!(
            w.review()
                .contains("inputs.theme = \" <&>\\t\" (dort.toml)")
        );
        w.approve();
        assert_eq!(fs::read(&target).unwrap(), expected.as_bytes());

        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('e'));
        w.press(KeyCode::F(4));
        w.press(KeyCode::Tab);
        w.press(KeyCode::Tab);
        w.app.key(
            ratatui::crossterm::event::KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            width,
            height,
        );
        w.paste("light");
        w.press(KeyCode::Enter);
        w.approve();
        assert_eq!(fs::read(&target).unwrap(), expected.as_bytes());
        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('v'));
        assert!(w.screen().contains("Update available"));
        w.press(KeyCode::Char('R'));
        assert!(w.review().contains("host.os = \"linux\" (this host)"));
        assert!(w.review().contains("inputs.theme = \"light\" (dort.toml)"));
        assert!(w.review().contains("NEW | theme=[light]\\n"));
        assert_eq!(fs::read(&target).unwrap(), expected.as_bytes());
        w.approve();
        assert_eq!(
            fs::read(&target).unwrap(),
            expected.replace(color, "light").as_bytes()
        );
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
fn generated_save_selects_the_saved_template_even_when_a_filter_hides_it() {
    let mut w = Workshop::new(100, 26);
    fs::write(
        w.repo.join("dort.toml"),
        "schema = 1\n[[files]]\nid = 'alpha'\nmode = 'template'\ntemplate = 'first output'\ntarget = '~/.alpha'\n",
    )
    .unwrap();
    w.run(Task::Refresh);
    w.press(KeyCode::Char('/'));
    for c in "alpha".chars() {
        w.press(KeyCode::Char(c));
    }
    w.press(KeyCode::Enter);
    w.press(KeyCode::Char('s'));
    w.paste("zeta");
    w.press(KeyCode::Tab);
    w.paste("~/.zeta");
    w.press(KeyCode::Tab);
    w.paste("second output");
    w.press(KeyCode::F(4));
    w.approve();
    w.press(KeyCode::Esc);
    w.press(KeyCode::Enter);
    assert!(w.screen().contains("second output"));
    w.press(KeyCode::Esc);
    w.press(KeyCode::Char('e'));
    assert!(w.screen().contains("Edit generated file: zeta"));
    w.press(KeyCode::End);
    w.paste(" edited");
    w.press(KeyCode::F(4));
    w.press(KeyCode::Esc);
    assert!(w.screen().contains("Edit generated file: zeta"));
    w.press(KeyCode::F(4));
    w.approve();
    w.press(KeyCode::Esc);
    w.press(KeyCode::Char('v'));
    assert!(w.screen().contains("second output edited"));
    assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 0);
}

#[test]
fn managed_and_generated_have_separate_rows_actions_and_filters() {
    for (width, height) in [(48, 14), (80, 24), (120, 32)] {
        let mut w = Workshop::new(width, height);
        fs::write(w.repo.join("shell"), "existing source").unwrap();
        let original = r#"schema = 1
[[files]]
id = "shell"
mode = "link"
source = "shell"
target = "~/shell"
[[files]]
id = "settings4"
mode = "template"
template = "editor=vim"
target = "~/config/settings"
"#;
        fs::write(w.repo.join("dort.toml"), original).unwrap();
        w.run(Task::Refresh);
        let generated = w.screen();
        assert!(
            generated.contains("settings4") && generated.contains("s New"),
            "{generated}"
        );
        assert!(
            !generated.contains("shell") && !generated.contains("marked"),
            "{generated}"
        );
        w.press(KeyCode::Char(' '));
        w.press(KeyCode::Char('a'));
        assert!(w.app.preview.is_none());
        w.press(KeyCode::Char('/'));
        for c in "settings4".chars() {
            w.press(KeyCode::Char(c));
        }
        w.press(KeyCode::Enter);
        w.press(KeyCode::Char('f'));
        let managed = w.screen();
        assert!(
            managed.contains("shell")
                && !managed.contains("settings4")
                && !managed.contains("s New"),
            "{managed}"
        );
        w.press(KeyCode::Home);
        w.press(KeyCode::Char(' '));
        w.press(KeyCode::Char('A'));
        assert!(w.review().contains("shell") && !w.review().contains("settings4"));
        w.press(KeyCode::Esc);
        w.press(KeyCode::End);
        w.press(KeyCode::Char('e'));
        assert!(w.screen().contains("Edit file mapping"));
        w.press(KeyCode::Esc);
        w.press(KeyCode::Esc);
        assert!(w.screen().contains("4 Generated"));
        click_label(&mut w.app, width, height, "4 Generated");
        assert!(w.screen().contains("settings4"));
        w.press(KeyCode::F(3));
        let details = w.screen();
        assert!(
            details.contains("Selected item")
                && details.contains("settings4")
                && !details.contains("editor=vim"),
            "{details}"
        );
        w.press(KeyCode::Esc);
        w.press(KeyCode::Char('v'));
        let preview = w.screen();
        assert!(preview.contains("Generated file preview"), "{preview}");
        w.press(KeyCode::End);
        assert!(w.screen().contains("editor=vim"));
        w.press(KeyCode::Esc);
        assert!(w.screen().contains("settings4"));
        click_label(
            &mut w.app,
            width,
            height,
            if width < 80 {
                "s New"
            } else {
                "s New generated file"
            },
        );
        assert!(w.screen().contains("File ID"));
        w.press(KeyCode::Esc);
        assert!(w.screen().contains("settings4"));
        assert_eq!(w.manifest(), original);
    }
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
        w.press(KeyCode::Esc);
        w.run(Task::PreviewGenerated("settings".into()));
        let screen = w.screen();
        assert!(screen.contains("Up to date"), "{screen}");
        assert!(!screen.contains("Review deployment"), "{screen}");
        fs::remove_file(&target).unwrap();
        w.run(Task::PreviewGenerated("settings".into()));
        let screen = w.screen();
        assert!(screen.contains("Local file deleted"), "{screen}");
        assert!(!screen.contains("Review deployment"), "{screen}");
        assert!(screen.contains("Review recreation"), "{screen}");
        w.press(KeyCode::Char('R'));
        w.press(KeyCode::Char('y'));
        assert!(!target.exists());
        assert!(w.review().contains("Detected: Local file deleted"));
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

fn tracked(w: &mut Workshop) -> (std::path::PathBuf, std::path::PathBuf) {
    fs::write(
        w.repo.join("dort.toml"),
        "schema=1\n[[files]]\nid='settings'\nmode='template'\ntarget='~/settings'\nsource='file.tmpl'\npermissions='0600'\n",
    )
    .unwrap();
    let source = w.repo.join("file.tmpl");
    fs::write(&source, b"keep\nold value\n").unwrap();
    dort_core::generated::Deployment::prepare(&w.host, None, "settings", &[])
        .unwrap()
        .approve()
        .execute()
        .unwrap();
    w.run(Task::Refresh);
    (source, w.host.home.join("settings"))
}

#[test]
fn tracked_update_displays_exact_escaped_diff_and_requires_a_fresh_approval() {
    use std::os::unix::fs::PermissionsExt;
    let mut w = Workshop::new(48, 14);
    let (source, target) = tracked(&mut w);
    let proposed = format!("keep\n{}END\t\\\x1b  ", "x".repeat(82));
    fs::write(&source, &proposed).unwrap();
    let manifest = w.manifest().replace("0600", "0644");
    fs::write(w.repo.join("dort.toml"), &manifest).unwrap();

    w.screen();
    w.press(KeyCode::Enter);
    let screen = w.screen();
    assert!(screen.contains("Review update"), "{screen}");
    assert!(!screen.contains("Review deployment"));
    let task = click_label(&mut w.app, w.width, w.height, "Review update").unwrap();
    w.run(task);
    let screen = w.screen();
    assert!(screen.contains("Permissions: 0600 -> 0644"), "{screen}");
    assert!(w.review().contains("Final newline: Yes -> No"));
    assert!(w.review().contains("OLD | old\\x20value\\n"));
    assert!(!w.review().contains("OLD | keep"));
    assert!(!w.review().contains('\x1b'));
    w.press(KeyCode::Enter);
    w.press(KeyCode::Char('y'));
    w.press(KeyCode::End);
    w.press(KeyCode::Char('y'));
    assert_eq!(fs::read(&target).unwrap(), b"keep\nold value\n");
    let screen = w.screen();
    assert!(screen.contains("End of update review"), "{screen}");
    let joined = screen.lines().map(str::trim_end).collect::<String>();
    assert!(joined.contains("END\\t\\\\\\x1b\\x20\\x20"), "{screen}");
    assert!(screen.contains("Update generated file"));
    w.press(KeyCode::Esc);
    let screen = w.screen();
    assert!(!screen.contains("Generated file preview"), "{screen}");
    assert!(screen.contains("settings"), "{screen}");
    assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 1);

    w.app.dry_run = true;
    w.press(KeyCode::Enter);
    w.press(KeyCode::Char('R'));
    w.screen();
    w.press(KeyCode::End);
    let screen = w.screen();
    assert!(screen.contains("End of update review"), "{screen}");
    assert!(!screen.contains("Update generated file"), "{screen}");
    w.press(KeyCode::Char('y'));
    assert_eq!(fs::read(&target).unwrap(), b"keep\nold value\n");
    w.press(KeyCode::Esc);
    w.app.dry_run = false;

    w.press(KeyCode::Enter);
    w.press(KeyCode::Char('R'));
    w.approve();
    let screen = w.screen();
    assert!(screen.contains("Generated update recorded"), "{screen}");
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
    use std::os::unix::fs::MetadataExt;
    w.press(KeyCode::Char('y'));
    let private = fs::read_dir(&w.host.home)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir())
        .unwrap();
    let saved = fs::read_dir(private)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(fs::read(saved).unwrap(), b"keep\nold value\n");
    assert_eq!(fs::read(&target).unwrap(), proposed.as_bytes());
    assert_eq!(fs::metadata(&target).unwrap().ino(), metadata.ino());
    w.press(KeyCode::Esc);
    w.press(KeyCode::Enter);
    let screen = w.screen();
    assert!(screen.contains("Up to date"), "{screen}");
    assert!(!screen.contains("Review update"));
}

#[test]
fn tracked_update_rejects_stale_input_and_live_edits() {
    for change_live in [false, true] {
        let mut w = Workshop::new(80, 24);
        let (source, target) = tracked(&mut w);
        fs::write(&source, "proposed").unwrap();
        w.screen();
        w.press(KeyCode::Enter);
        w.press(KeyCode::Char('R'));
        assert!(w.review().contains("NEW | proposed"));
        if change_live {
            fs::write(&target, "local edit").unwrap();
        } else {
            fs::write(&source, "later proposal").unwrap();
        }
        w.approve();
        let screen = w.screen();
        assert!(screen.contains("Open a new review"), "{screen}");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            if change_live {
                "local edit"
            } else {
                "keep\nold value\n"
            }
        );
        assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 1);
        if change_live {
            w.press(KeyCode::Esc);
            w.run(Task::PreviewGenerated("settings".into()));
            let screen = w.screen();
            assert!(screen.contains("Local edits"), "{screen}");
            assert!(!screen.contains("Review update"));
            assert!(screen.contains("Review replacement"), "{screen}");
            w.press(KeyCode::Char('R'));
            assert!(w.review().contains("OLD | local\\x20edit"));
        }
    }
}

#[test]
fn local_generated_reviews_keep_by_default_and_apply_only_the_named_action_at_the_end() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for (width, height, recreate) in [(48, 14, false), (80, 24, true)] {
        let mut w = Workshop::new(width, height);
        let (source, dest) = tracked(&mut w);
        let proposed = "keep\n".to_owned() + &"another line\n".repeat(30) + "proposed\tend";
        fs::write(&source, &proposed).unwrap();
        let local = b"keep\nlocal edit\r\n";
        let (label, confirm, end, status, inode) = if recreate {
            fs::remove_file(&dest).unwrap();
            (
                "Review recreation",
                "Recreate generated file",
                "End of recreation review",
                "Generated recreation recorded",
                None,
            )
        } else {
            fs::write(&dest, local).unwrap();
            fs::set_permissions(&dest, fs::Permissions::from_mode(0o640)).unwrap();
            (
                "Review replacement",
                "Replace generated file",
                "End of replacement review",
                "Generated replacement recorded",
                Some(fs::metadata(&dest).unwrap().ino()),
            )
        };
        let old = inode.map(|_| local.to_vec());
        for cancel in [true, false] {
            w.screen();
            w.press(KeyCode::Enter);
            let screen = w.screen();
            assert!(screen.contains(label), "{screen}");
            assert!(!screen.contains("Review update") && !screen.contains("Review deployment"));
            let task = click_label(&mut w.app, width, height, label).unwrap();
            w.run(task);
            let review = w.review();
            assert!(review.contains("NEW | proposed\\tend"));
            if recreate {
                assert!(review.contains("Detected: Local file deleted"));
                assert!(review.contains("Permissions: Missing -> 0600"));
                assert!(!review.contains("OLD |"));
            } else {
                assert!(review.contains("Detected: Local edits and permissions changed"));
                assert!(review.contains("Permissions: 0640 -> 0600"));
                assert!(review.contains("SAME | keep\\n"));
                assert!(review.contains("OLD | local\\x20edit\\r\\n"));
            }
            assert!(!w.screen().contains(confirm));
            w.press(KeyCode::Enter);
            w.press(KeyCode::Char('y'));
            assert_eq!(fs::read(&dest).ok(), old);
            w.press(KeyCode::End);
            w.press(KeyCode::Char('y'));
            assert_eq!(fs::read(&dest).ok(), old);
            let screen = w.screen();
            assert!(screen.contains(end) && screen.contains(confirm), "{screen}");
            if cancel {
                w.press(KeyCode::Esc);
                assert_eq!(fs::read(&dest).ok(), old);
                assert_eq!(
                    fs::read_dir(&w.host.home).unwrap().count(),
                    usize::from(inode.is_some())
                );
            } else {
                w.press(KeyCode::Char('y'));
            }
        }
        let screen = w.screen();
        assert!(screen.contains(status), "{screen}");
        assert_eq!(fs::read(&dest).unwrap(), proposed.as_bytes());
        let published = fs::metadata(&dest).unwrap();
        assert_eq!(published.mode() & 0o7777, 0o600);
        let artifact = fs::read_dir(&w.host.home)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.is_dir())
            .unwrap()
            .join("file");
        if let Some(inode) = inode {
            assert_eq!(fs::read(&artifact).unwrap(), local);
            let retained = fs::metadata(&artifact).unwrap();
            assert_eq!(retained.ino(), inode);
            assert_eq!(retained.mode() & 0o7777, 0o640);
        } else {
            assert!(!artifact.exists());
            assert!(!screen.contains("retained"));
        }
        w.press(KeyCode::Char('y'));
        assert_eq!(fs::metadata(&dest).unwrap().ino(), published.ino());
    }
}

#[test]
fn update_refuses_when_the_full_review_exceeds_the_display_limit() {
    let mut w = Workshop::new(80, 24);
    let (source, target) = tracked(&mut w);
    fs::write(source, " ".repeat(20_000)).unwrap();
    w.screen();
    w.press(KeyCode::Enter);
    w.press(KeyCode::Char('R'));
    let screen = w.screen();
    assert!(screen.contains("too large to display"), "{screen}");
    w.press(KeyCode::End);
    w.press(KeyCode::Char('y'));
    assert_eq!(fs::read(&target).unwrap(), b"keep\nold value\n");
    assert_eq!(fs::read_dir(&w.host.home).unwrap().count(), 1);
}

#[test]
fn interrupted_generated_actions_have_explicit_recovery_and_truthful_effects() {
    use dort_core::generated::UpdateAction;
    for (action, notice, effect) in [
        (
            UpdateAction::Update,
            "Generated update needs recovery",
            "It does not replace either file.",
        ),
        (
            UpdateAction::Replace,
            "Generated replacement needs recovery",
            "It does not replace either file.",
        ),
        (
            UpdateAction::Recreate,
            "Generated recreation needs recovery",
            "Recovery never creates the file",
        ),
    ] {
        check_generated_recovery(action, notice, effect);
    }
}

fn check_generated_recovery(
    action: dort_core::generated::UpdateAction,
    notice: &str,
    effect: &str,
) {
    use crate::tui::actions::{Output, Pending};
    use dort_core::generated::{ExchangeStatus, UpdateLocation, UpdateOutcome};
    let mut w = Workshop::new(80, 24);
    let location = || UpdateLocation {
        destination: "/home/settings".into(),
        artifact: "/home/.saved/file".into(),
        stage_device: 12,
        stage_inode: 34,
        action,
    };
    w.app.complete(Ok(Output::GeneratedUpdated(Some(
        UpdateOutcome::NeedsRecovery {
            location: location(),
            exchange: ExchangeStatus::Unknown,
            reason: "The files changed.".into(),
        },
    ))));
    let screen = w.screen();
    assert!(
        screen.contains("The new file may already be visible"),
        "{screen}"
    );
    assert!(screen.contains("/home/.saved/file"), "{screen}");
    assert!(screen.contains("h Recovery"), "{screen}");
    assert!(!screen.contains("No changes were made"));

    let task = w.app.key(key(KeyCode::Char('h')), w.width, w.height);
    assert!(matches!(task, Some(Task::History)));
    w.app.complete(Ok(Output::History {
        rows: vec![],
        error: None,
        generated_recovery: Some(Ok(location())),
    }));
    let screen = w.screen();
    assert!(screen.contains(notice), "{screen}");
    assert!(screen.contains("Blocked writes: 1"), "{screen}");
    assert!(matches!(
        click_label(&mut w.app, w.width, w.height, "Review generated recovery"),
        Some(Task::ReviewGeneratedRecovery),
    ));
    w.app
        .complete(Ok(Output::Preview(Pending::GeneratedRecovery(location()))));
    assert!(
        w.app
            .key(key(KeyCode::Char('y')), w.width, w.height)
            .is_none()
    );
    let screen = w.screen();
    assert!(screen.contains(effect), "{screen}");
    assert!(matches!(
        super::tests::approve_ready(&mut w.app, w.width, w.height),
        Some(Task::Execute(_)),
    ));
    assert!(
        w.app
            .key(key(KeyCode::Char('y')), w.width, w.height)
            .is_none()
    );
}
