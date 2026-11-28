use super::{
    actions::{Task, perform},
    app::{App, Tab},
    tests::{approve_ready, click_label, destination, render},
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fs;

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
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        if let Some(task) = self.app.key(key, self.width, self.height) {
            self.run(task);
        }
    }

    fn click(&mut self, label: &str) {
        if let Some(task) = click_label(&mut self.app, self.width, self.height, label) {
            self.run(task);
        }
    }

    fn screen(&mut self) -> String {
        render(&self.app, self.width, self.height)
    }

    fn review(&self) -> &str {
        &self.app.preview.as_ref().expect("expected review").text
    }

    fn approve(&mut self) {
        let task = approve_ready(&mut self.app, self.width, self.height).unwrap();
        self.run(task);
    }

    fn manifest(&self) -> String {
        fs::read_to_string(self.repo.join("dort.toml")).unwrap()
    }

    /// One link mapping whose destination is the managed symlink.
    fn link(&mut self, text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        fs::write(
            self.repo.join("dort.toml"),
            "schema = 1\n[[files]]\nid = 'settings'\nmode = 'link'\ntarget = '~/settings'\nsource = 'settings.sh'\n",
        )
        .unwrap();
        let source = self.repo.join("settings.sh");
        fs::write(&source, text).unwrap();
        let target = self.host.home.join("settings");
        std::os::unix::fs::symlink(&source, &target).unwrap();
        self.run(Task::Refresh);
        self.screen();
        (source, target)
    }
}

fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn managed_link_converts_on_key_and_keeps_exact_bytes_mode_manifest_and_link_target() {
    let mut w = Workshop::new(140, 30);
    let (source, target) = w.link("#!/bin/sh\necho hi\n");
    let screen = w.screen();
    assert!(screen.contains("Convert to generated file"), "{screen}");
    w.click("Convert to generated file");
    let screen = w.screen();
    assert!(screen.contains("Review conversion"), "{screen}");
    let review = w.review().to_owned();
    assert!(
        review.contains("Template file: \"templates/settings.jinja\""),
        "{review}"
    );
    assert!(review.contains("Retained link target"), "{review}");
    assert!(review.contains("SAME | #!/bin/sh\\n"), "{review}");
    assert!(review.contains("SAME | echo hi\\n"), "{review}");
    assert!(review.contains("Permissions: 0644"), "{review}");
    assert!(review.contains("Template permissions: 0644"), "{review}");
    assert!(review.contains("1. Create template file"), "{review}");
    assert!(review.contains("3. Change the mapping"), "{review}");
    assert!(screen.contains("Convert to generated file"), "{screen}");

    w.approve();
    let screen = w.screen();
    assert!(screen.contains("Converted"), "{screen}");
    assert!(screen.contains("templates/settings.jinja"), "{screen}");

    let template = w.repo.join("templates/settings.jinja");
    assert_eq!(fs::read(&template).unwrap(), b"#!/bin/sh\necho hi\n");
    assert_eq!(mode(&template), 0o644);
    assert_eq!(fs::read(&target).unwrap(), b"#!/bin/sh\necho hi\n");
    assert_eq!(mode(&target), 0o644);
    assert!(
        !fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // The retained link target keeps its bytes for the deployment record.
    assert_eq!(fs::read(&source).unwrap(), b"#!/bin/sh\necho hi\n");

    let manifest = dort_core::Manifest::parse(&w.manifest()).unwrap();
    assert_eq!(manifest.files.len(), 1);
    let file = &manifest.files[0];
    assert_eq!(file.mode, dort_core::model::FileMode::Template);
    assert_eq!(
        file.source.as_deref(),
        Some(std::path::Path::new("templates/settings.jinja"))
    );
    assert_eq!(file.permissions.as_deref(), Some("0644"));
    assert!(
        dort_core::generated::pending_conversion(&w.host)
            .unwrap()
            .is_none()
    );
}

#[test]
fn cancelled_conversion_writes_nothing() {
    let mut w = Workshop::new(80, 24);
    let (_source, target) = w.link("keep\n");
    let manifest = w.manifest();
    w.press(KeyCode::Char('c'));
    assert!(w.screen().contains("Review conversion"));
    w.press(KeyCode::Esc);
    let screen = w.screen();
    assert!(!screen.contains("Review conversion"), "{screen}");
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(fs::symlink_metadata(w.repo.join("templates")).is_err());
    assert_eq!(w.manifest(), manifest);
    assert!(
        dort_core::generated::pending_conversion(&w.host)
            .unwrap()
            .is_none()
    );
}

#[test]
fn refused_source_names_the_line_and_offers_no_review() {
    let mut w = Workshop::new(80, 24);
    w.link("keep\n{{ missing }}\n");
    w.press(KeyCode::Char('c'));
    let screen = w.screen();
    assert!(
        screen.contains("Line 2 of the source contains {{"),
        "{screen}"
    );
    assert!(!screen.contains("Review conversion"), "{screen}");
}

#[test]
fn pending_conversion_shows_in_recovery_and_finishing_it_converges() {
    use std::os::unix::fs::PermissionsExt;
    let mut w = Workshop::new(80, 24);
    let (_source, target) = w.link("keep\n");
    w.press(KeyCode::Char('c'));
    // A closed record folder fails the record step after the destination exchange.
    let record = record_folder(&w);
    fs::set_permissions(&record, fs::Permissions::from_mode(0o600)).unwrap();
    w.approve();
    fs::set_permissions(&record, fs::Permissions::from_mode(0o700)).unwrap();
    let screen = w.screen();
    assert!(screen.contains("Conversion needs recovery."), "{screen}");
    assert!(w.app.data.as_ref().unwrap().conversion_recovery.is_some());
    assert_eq!(fs::read(&target).unwrap(), b"keep\n");
    assert!(w.manifest().contains("mode = 'link'") || w.manifest().contains("mode = \"link\""));

    destination(&mut w.app, Tab::Recovery, w.width, w.height);
    let screen = w.screen();
    assert!(screen.contains("Conversion needs recovery."), "{screen}");
    assert!(screen.contains("Finish conversion"), "{screen}");
    if let Some(task) = click_label(&mut w.app, w.width, w.height, "Finish conversion") {
        w.run(task);
    } else {
        w.press(KeyCode::Char('c'));
    }
    let review = w.review().to_owned();
    assert!(review.contains("1. Template file created"), "{review}");
    assert!(!review.contains("2. Destination replaced"), "{review}");
    w.approve();
    let screen = w.screen();
    assert!(screen.contains("Converted"), "{screen}");
    assert_eq!(fs::read(&target).unwrap(), b"keep\n");
    assert!(
        dort_core::generated::pending_conversion(&w.host)
            .unwrap()
            .is_none()
    );
    let manifest = dort_core::Manifest::parse(&w.manifest()).unwrap();
    assert_eq!(manifest.files[0].mode, dort_core::model::FileMode::Template);
}

fn record_folder(w: &Workshop) -> std::path::PathBuf {
    let local = dort_core::local::LocalConfig::load(&w.host).unwrap();
    let id = local.repositories.keys().next().unwrap().clone();
    let folder = dort_core::local::repository_state_path(&w.host, &id)
        .unwrap()
        .join("generated");
    fs::create_dir_all(&folder).unwrap();
    folder
}

#[test]
fn narrow_layout_shows_the_control_and_hides_it_for_template_rows() {
    let mut w = Workshop::new(48, 14);
    w.link("keep\n");
    w.screen();
    w.press(KeyCode::F(2));
    let screen = w.screen();
    assert!(screen.contains("Convert to generated file"), "{screen}");
    w.press(KeyCode::Esc);
    w.press(KeyCode::Char('c'));
    assert!(w.screen().contains("Review conversion"));
    w.press(KeyCode::Esc);

    fs::write(
        w.repo.join("dort.toml"),
        "schema = 1\n[[files]]\nid = 'settings'\nmode = 'template'\ntarget = '~/settings'\nsource = 'templates/settings.jinja'\n",
    )
    .unwrap();
    fs::create_dir_all(w.repo.join("templates")).unwrap();
    fs::write(w.repo.join("templates/settings.jinja"), "keep\n").unwrap();
    w.run(Task::Refresh);
    w.screen();
    let screen = w.screen();
    assert!(!screen.contains("Convert to generated file"), "{screen}");
}
