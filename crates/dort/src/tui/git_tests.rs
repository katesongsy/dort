use super::{
    actions::{self, Task},
    app::{App, Tab},
    tests::render,
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
    git: PathBuf,
    calls: PathBuf,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let home = base.join("home");
        let root = base.join("git-only");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&root).unwrap();
        let git = base.join("git");
        let calls = base.join("calls");
        fs::write(&git, format!(r#"#!/bin/sh
[ "$GIT_OPTIONAL_LOCKS" = 0 ] || exit 81
[ "$GIT_CONFIG_GLOBAL" = /dev/null ] || exit 82
printf '%s\n' "$*" >> '{}'
for arg do case "$arg" in
config) if [ -e refuse ]; then printf 'filter.bad.clean\n'; exit 0; fi; exit 1;;
rev-parse) if [ -e bad-root ]; then printf '/not-this-root\n'; else pwd -P; fi; exit 0;;
status) printf '# branch.oid (initial)\000# branch.head main\000? strange\nname\000'; exit 0;;
diff) if [ -e fail ]; then printf 'PRIVATE-PATCH-FAIL' >&2; exit 1; fi
case " $* " in
*" --patch "*) printf 'PRIVATE-PATCH\033[31m\n';;
*) if [ ! -e empty ]; then printf ':100644 100755 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 M\000strange\nname\000'; fi;;
esac; exit 0;;
esac; done
exit 90
"#, calls.display())).unwrap();
        fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
        let host = HostContext::new(
            home.clone(),
            None,
            None,
            Platform::Ubuntu,
            Architecture::X86_64,
        )
        .unwrap();
        Self {
            _temp: temp,
            home,
            root,
            git,
            calls,
            host,
        }
    }
    fn app(&self) -> App {
        let mut app = App::new(false);
        app.git = Ok(self.git.clone());
        app.tab = Tab::Git;
        app.launch_directory = self.root.parent().unwrap().to_owned();
        app
    }
    fn run(&self, app: &mut App, task: Task) {
        app.start(false);
        let result =
            actions::perform_scoped(task, &self.host, app.repository.clone(), &[], |_| Ok(()))
                .map_err(|e| actions::diagnostic(&e));
        assert!(app.complete(result).is_none());
    }
    fn choose(&self, app: &mut App) {
        key(app, KeyCode::Char('g'));
        text(app, "git-only");
        let task = key(app, KeyCode::Enter).unwrap();
        self.run(app, task);
    }
    fn patch_calls(&self) -> usize {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains(" --patch "))
            .count()
    }
    fn readonly(&self) {
        assert_eq!(fs::read_dir(&self.home).unwrap().count(), 0);
        assert!(!self.root.join("dort.toml").exists());
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn text(app: &mut App, value: &str) {
    for c in value.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}
fn patch_task(app: &mut App) -> Task {
    key(app, KeyCode::Char('w'));
    text(app, "view patch");
    key(app, KeyCode::Enter).unwrap()
}
#[test]
fn independent_git_summaries_match_backend_and_never_prefetch_content() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    assert!(app.repository.is_none() && !app.git_view.follows_files);
    let screen = render(&app, 100, 30);
    assert!(
        screen.contains("initial") && screen.contains("main") && screen.contains("strange\\nname")
    );
    let backend = dort_core::git::Git::new(f.git.clone()).unwrap();
    if let Some(super::git::Metadata::Status(actual)) = &app.git_view.metadata {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(backend.status(&f.root).unwrap()).unwrap()
        );
    } else {
        panic!("expected typed status");
    }
    for comparison in ['u', 's'] {
        let task = key(&mut app, KeyCode::Char(comparison)).unwrap();
        f.run(&mut app, task);
        if let Some(super::git::Metadata::Diff(actual)) = &app.git_view.metadata {
            assert!(actual.patch.is_none());
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(backend.diff(&f.root, comparison == 's', false).unwrap())
                    .unwrap()
            );
        } else {
            panic!("expected typed diff");
        }
        key(&mut app, KeyCode::Char('/'));
        text(&mut app, "q?/");
        key(&mut app, KeyCode::Esc);
        assert!(!app.quit && render(&app, 100, 30).contains("0 displayed"));
        key(&mut app, KeyCode::Delete);
        key(&mut app, KeyCode::Char('i'));
        assert!(render(&app, 100, 30).contains("100644 -> 100755"));
        let task = key(&mut app, KeyCode::Char('r')).unwrap();
        f.run(&mut app, task);
    }
    assert_eq!(f.patch_calls(), 0);
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    f.readonly();
}
#[test]
fn patch_disclosure_is_once_isolated_and_dropped_on_close_scope_resize_or_exit() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    let task = key(&mut app, KeyCode::Char('u')).unwrap();
    f.run(&mut app, task);
    key(&mut app, KeyCode::Tab); // Actions focus must return to Git content after activation.
    key(&mut app, KeyCode::Char(' ')); // Actions opens with search focused.
    text(&mut app, "View patch content");
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.tab == Tab::Git);
    assert!(render(&app, 48, 14).contains("May expose secrets"));
    text(&mut app, "yes");
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(f.patch_calls(), 0);
    key(&mut app, KeyCode::Esc);
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    let screen = render(&app, 100, 30);
    assert!(screen.contains("PRIVATE-PATCH\\x1b[31m"));
    assert!(!app.status.contains("PRIVATE-PATCH"));
    key(&mut app, KeyCode::Esc);
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    f.run(&mut app, task);
    assert_eq!(f.patch_calls(), 1);
    key(&mut app, KeyCode::Char('w'));
    text(&mut app, "view patch");
    app.resize(47, 14);
    app.resize(80, 24);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(f.patch_calls(), 1);
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    app.resize(80, 13);
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    let task = key(&mut app, KeyCode::Char('s')).unwrap();
    f.run(&mut app, task);
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    key(&mut app, KeyCode::Char('g'));
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    key(&mut app, KeyCode::Esc);
    let task = patch_task(&mut app);
    app.start(false);
    key(&mut app, KeyCode::Right); // safe navigation while worker is active discards pending disclosure
    let output = actions::perform(task, &f.host, None, |_| Ok(())).unwrap();
    app.complete(Ok(output));
    app.tab = Tab::Git;
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    key(&mut app, KeyCode::Char('q'));
    assert!(app.quit && !app.git_view.sensitive());
    f.readonly();
}
#[test]
fn refusals_empty_missing_git_and_patch_errors_keep_content_out_of_diagnostics() {
    let f = Fixture::new();
    let mut app = f.app();
    app.git = Err("Git unavailable".into());
    key(&mut app, KeyCode::Char('g'));
    text(&mut app, "git-only");
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.error.as_ref().unwrap().contains("restart"));
    assert_eq!(app.git_view.draft, "git-only");
    app.git = Ok(f.git.clone());
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    for refusal in ["refuse", "bad-root"] {
        fs::write(f.root.join(refusal), "").unwrap();
        let task = key(&mut app, KeyCode::Char('u')).unwrap();
        f.run(&mut app, task);
        assert!(app.error.is_some());
        key(&mut app, KeyCode::Esc);
        fs::remove_file(f.root.join(refusal)).unwrap();
    }
    fs::write(f.root.join("empty"), "").unwrap();
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    f.run(&mut app, task);
    assert!(render(&app, 100, 30).contains("No changes"));
    fs::write(f.root.join("fail"), "").unwrap();
    let task = patch_task(&mut app);
    f.run(&mut app, task);
    assert!(app.error.is_some());
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    assert!(!app.git_view.sensitive());
    f.readonly();
}
#[test]
fn git_root_default_and_independent_selection_do_not_require_healthy_file_observation() {
    let f = Fixture::new();
    let mut app = f.app();
    // Select an ordinary directory with no manifest: file observation fails, Git remains usable.
    let result = actions::perform(
        Task::Select(Some(f.root.clone())),
        &f.host,
        None,
        |_| Ok(()),
    )
    .unwrap();
    app.complete(Ok(result));
    assert!(app.data.as_ref().unwrap().files_error.is_some());
    let task = key(&mut app, KeyCode::Char('z')).unwrap();
    f.run(&mut app, task);
    assert_eq!(app.git_view.root.as_ref(), Some(&f.root));
    assert!(app.git_view.follows_files && app.git_view.metadata.is_some());
    // Pin the Git-only root, then change file repository independently.
    f.choose(&mut app);
    let other = f.root.parent().unwrap().join("other");
    fs::create_dir(&other).unwrap();
    let result =
        actions::perform(Task::Select(Some(other.clone())), &f.host, None, |_| Ok(())).unwrap();
    app.complete(Ok(result));
    assert_eq!(app.repository.as_ref(), Some(&other));
    assert_eq!(app.git_view.root.as_ref(), Some(&f.root));
    key(&mut app, KeyCode::Char(' '));
    text(&mut app, "Git: use current file repository root");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(app.git_view.root.as_ref(), Some(&other));
    assert!(app.git_view.follows_files);
    f.readonly();
}
#[test]
fn git_selection_uses_raw_identity_across_refresh_and_compact_search_is_visible() {
    fn summary(names: &[&str]) -> super::git::Report {
        super::git::Report::Summary(super::git::Metadata::Status(dort_core::git::Status {
            schema: 1,
            read_only: true,
            branch: vec!["branch.head detached".into()],
            entries: names
                .iter()
                .map(|name| dort_core::git::StatusEntry {
                    kind: "renamed".into(),
                    xy: Some("R.".into()),
                    path: (*name).into(),
                    original_path: Some("old\nname".into()),
                })
                .collect(),
        }))
    }
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view
        .accept(summary(&["first", "second\u{202e}"]), true);
    key(&mut app, KeyCode::Down);
    app.git_view
        .accept(summary(&["second\u{202e}", "first"]), true);
    key(&mut app, KeyCode::Char('i'));
    let screen = render(&app, 100, 30);
    assert!(
        screen.contains("second\\u{202e}")
            && screen.contains("Original path: Some(\"old\\nname\")")
    );
    key(&mut app, KeyCode::Esc);
    assert!(render(&app, 48, 14).contains("> second\\u{202e}"));
    key(&mut app, KeyCode::Char('/'));
    text(&mut app, "q?/");
    assert!(render(&app, 48, 14).contains("Filter > q?/"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Delete);
    app.git_view.accept(summary(&[]), true);
    assert!(render(&app, 100, 30).contains("0 entries"));
}
#[test]
fn compact_git_drafts_and_confirmation_remain_visible_and_small_cannot_disclose() {
    let mut app = App::new(true);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = "q?".repeat(2000);
    assert!(render(&app, 48, 14).contains("Path > ..."));
    key(&mut app, KeyCode::Esc);
    app.git_view.choose(super::git::Comparison::Unstaged);
    app.git_view.metadata = Some(super::git::Metadata::Diff(dort_core::git::Diff {
        schema: 1,
        read_only: true,
        staged: false,
        entries: vec![],
        patch: None,
    }));
    key(&mut app, KeyCode::Char('w'));
    assert!(render(&app, 48, 14).contains("Confirm >"));
    text(&mut app, "view patch");
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(!app.git_view.sensitive());
}
