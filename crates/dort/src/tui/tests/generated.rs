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
        let recipe = "theme={{ theme }}\n{{ padding }}{{ padding }}{{ padding }}{{ padding }}{{ padding }}last = \x1b[31mTAIL\n";
        fs::write(repo.join("editor.tmpl"), recipe).unwrap();
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
            recipe
        );
        assert_eq!(
            fs::read_to_string(repo.join("dort.toml")).unwrap(),
            manifest
        );
        assert_eq!(fs::read_dir(&home).unwrap().count(), 1);
    }
}
