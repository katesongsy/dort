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
status) printf '# branch.oid (initial)\000# branch.head main\000? strange\nname\000'
if [ -e tracked ]; then
printf '1 MM N... 100644 100644 100644 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 alpha*\000'; fi; exit 0;;
diff) if [ -e fail ]; then printf 'PRIVATE-PATCH-FAIL' >&2; exit 1; fi
case " $* " in
*" --patch "*)
if [ -e no-hunks ]; then exit 0; fi
case " $* " in
*" :(top,literal)alpha* "*) name=ALPHA;;
*" :(top,literal)beta "*) name=BETA;;
*) exit 92;;
esac
case " $* " in *" --cached "*) side=STAGED;; *) side=UNSTAGED;; esac
printf '%s-%s\033[31m\n' "$name" "$side"
i=1; while [ "$i" -lt 25 ]; do printf 'context %s\n' "$i"; i=$((i + 1)); done
printf '%s-%s-END\n' "$name" "$side";;
*) if [ ! -e empty ]; then
printf ':100644 100755 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 M\000alpha*\000:100644 100755 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 M\000beta\000'; fi;;
esac; exit 0;;
esac; done
exit 90
"#, calls.display())).unwrap();
        fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
        let host = HostContext::new(
            home.clone(),
            None,
            None,
            Platform::Linux,
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
        if !matches!(task, Task::GitPreview(_)) {
            app.start(false);
        }
        let result =
            actions::perform_scoped(task, &self.host, app.repository.clone(), &[], |_| Ok(()))
                .map_err(|e| actions::diagnostic(&e));
        assert!(app.complete(result).is_none());
    }
    fn choose(&self, app: &mut App) {
        key(app, KeyCode::Char('P'));
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
    super::tests::focus_form(app);
    for c in value.chars() {
        assert!(key(app, KeyCode::Char(c)).is_none());
    }
}
#[test]
fn git_context_actions_inspect_and_clear_only_the_git_display_filter() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    app.inventory.managed.query = "retained-file-filter".into();
    key(&mut app, KeyCode::Char('/'));
    text(&mut app, "no-match");
    key(&mut app, KeyCode::Esc);
    assert!(app.activate(super::shell::Action::ClearFilter).is_none());
    assert!(app.git_view.search.query.is_empty());
    assert_eq!(app.inventory.managed.query, "retained-file-filter");
    assert!(app.activate(super::shell::Action::Inspect).is_none());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(render(&app, 80, 24).contains("Git details"));
    assert_eq!(f.patch_calls(), 0);
    f.readonly();
}
#[test]
fn independent_git_summaries_match_backend_without_patch_content() {
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
        key(&mut app, KeyCode::F(3));
        assert!(render(&app, 100, 30).contains("100644 -> 100755"));
        let task = key(&mut app, KeyCode::Char('R')).unwrap();
        f.run(&mut app, task);
    }
    assert_eq!(f.patch_calls(), 0);
    assert!(!render(&app, 100, 30).contains("PRIVATE-PATCH"));
    f.readonly();
}
#[test]
fn summary_refusals_empty_and_missing_git_are_handled() {
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
    let task = key(&mut app, KeyCode::Char('R')).unwrap();
    f.run(&mut app, task);
    assert!(render(&app, 100, 30).contains("No changes"));
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
    let task = app.activate(super::shell::Action::GitDefault).unwrap();
    f.run(&mut app, task);
    assert_eq!(app.git_view.root.as_ref(), Some(&other));
    assert!(app.git_view.follows_files);
    f.readonly();
}
#[test]
fn git_selection_uses_raw_identity_across_refresh_and_compact_search_is_visible() {
    fn summary(names: &[&str]) -> super::git::Metadata {
        super::git::Metadata::Status(dort_core::git::Status {
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
        })
    }
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.accept(summary(&["first", "second\u{202e}"]));
    key(&mut app, KeyCode::Down);
    app.git_view.accept(summary(&["second\u{202e}", "first"]));
    for code in [KeyCode::PageDown, KeyCode::PageUp, KeyCode::Home] {
        assert!(key(&mut app, code).is_none());
        assert!(!app.git_view.browsing_details());
    }
    key(&mut app, KeyCode::Enter);
    let screen = render(&app, 100, 30);
    assert!(
        screen.contains("second\\u{202e}")
            && screen.contains("Original path: Some(\"old\\nname\")")
    );
    key(&mut app, KeyCode::Esc);
    assert!(render(&app, 48, 14).contains("› second\\u{202e}"));
    key(&mut app, KeyCode::Char('/'));
    text(&mut app, "q?/");
    assert!(render(&app, 48, 14).contains("[q?/"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Delete);
    app.git_view.accept(summary(&[]));
    assert!(render(&app, 100, 30).contains("0 entries"));
}
#[test]
fn compact_git_root_draft_is_visible_at_the_terminal_floor() {
    let mut app = App::new(true);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = "q?".repeat(2000);
    assert!(render(&app, 48, 14).contains("Exact Git root"));
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(app.git_view.editing_root);
}

fn wide_key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 140, 32)
}

#[test]
fn wide_git_preview_follows_keyboard_mouse_filter_and_comparison_without_confirmation() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    assert!(app.next_git_preview().is_none(), "untracked selection");
    assert!(render(&app, 140, 32).contains("untracked"));

    let summary = wide_key(&mut app, KeyCode::Char('u')).unwrap();
    f.run(&mut app, summary);
    let preview = app.next_git_preview().unwrap();
    assert!(render(&app, 140, 32).contains("Reading selected patch"));
    assert!(!app.busy && app.confirmation.is_none());
    f.run(&mut app, preview);
    let screen = render(&app, 140, 32);
    assert!(screen.contains("Selected patch") && screen.contains("Focus patch"));
    assert!(screen.contains("ALPHA-UNSTAGED\\x1b[31m") && !screen.contains("BETA-UNSTAGED"));
    assert!(!screen.contains('\x1b') && !screen.contains("View patch"));

    let (column, row) = (0..32)
        .flat_map(|y| (0..140).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x, y), Some(super::input::Target::Row(1))))
        .unwrap();
    assert!(
        app.mouse(
            ratatui::crossterm::event::MouseEvent {
                kind: ratatui::crossterm::event::MouseEventKind::Down(
                    ratatui::crossterm::event::MouseButton::Left
                ),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
            140,
            32,
        )
        .is_none()
    );
    assert!(!render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    let screen = render(&app, 140, 32);
    assert!(screen.contains("› beta") && screen.contains("BETA-UNSTAGED"));
    assert!(
        app.mouse(
            ratatui::crossterm::event::MouseEvent {
                kind: ratatui::crossterm::event::MouseEventKind::ScrollDown,
                column: 90,
                row,
                modifiers: KeyModifiers::NONE,
            },
            140,
            32,
        )
        .is_none()
    );
    let scrolled = render(&app, 140, 32);
    assert!(scrolled.contains("› beta") && scrolled.contains("BETA-UNSTAGED-END"));
    assert_eq!(scrolled.matches("[Active]").count(), 1, "{scrolled}");
    wide_key(&mut app, KeyCode::Up);
    assert!(render(&app, 140, 32).contains("› beta"));
    wide_key(&mut app, KeyCode::Tab);
    wide_key(&mut app, KeyCode::Up);
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    assert!(render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    wide_key(&mut app, KeyCode::Char('/'));
    text(&mut app, "beta");
    key(&mut app, KeyCode::Enter);
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    assert!(render(&app, 140, 32).contains("BETA-UNSTAGED"));
    let task = wide_key(&mut app, KeyCode::Char('s')).unwrap();
    f.run(&mut app, task);
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    let screen = render(&app, 140, 32);
    assert!(screen.contains("BETA-STAGED") && !screen.contains("BETA-UNSTAGED"));
    f.readonly();
}

#[test]
fn compact_status_opens_both_tracked_changes_and_f3_opens_metadata() {
    let f = Fixture::new();
    fs::write(f.root.join("tracked"), "").unwrap();
    let mut app = f.app();
    f.choose(&mut app);
    assert!(app.next_git_preview().is_none());
    key(&mut app, KeyCode::Down);
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    let list = render(&app, 80, 24);
    assert!(list.contains("Open patch") && !list.contains("ALPHA-STAGED"));
    key(&mut app, KeyCode::Enter);
    let start = render(&app, 80, 24);
    assert!(start.contains("Selected patch") && start.contains("ALPHA-STAGED"));
    key(&mut app, KeyCode::End);
    let end = render(&app, 80, 24);
    assert!(end.contains("ALPHA-UNSTAGED-END"));
    key(&mut app, KeyCode::F(1));
    let help = render(&app, 80, 24);
    assert!(help.contains("Help") && !help.contains("ALPHA-UNSTAGED"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::F(3));
    let details = render(&app, 80, 24);
    assert!(details.contains("Git details") && details.contains("XY: MM"));
    assert!(!details.contains("ALPHA-STAGED"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Enter);
    let untracked = render(&app, 80, 24);
    assert!(untracked.contains("Git details") && untracked.contains("untracked"));
    assert_eq!(f.patch_calls(), 2);
}

#[test]
fn stale_patch_reads_cannot_return_after_selection_refresh_page_or_size_changes() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    let task = key(&mut app, KeyCode::Char('u')).unwrap();
    f.run(&mut app, task);
    let first = app.next_git_preview().unwrap();
    wide_key(&mut app, KeyCode::Down);
    wide_key(&mut app, KeyCode::Up);
    let latest = app.next_git_preview().unwrap();
    f.run(&mut app, first);
    assert!(!render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    f.run(&mut app, latest);
    assert!(render(&app, 140, 32).contains("ALPHA-UNSTAGED"));

    let summary = wide_key(&mut app, KeyCode::Char('R')).unwrap();
    assert!(!render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    f.run(&mut app, summary);
    let before_refresh = app.next_git_preview().unwrap();
    let summary = wide_key(&mut app, KeyCode::Char('R')).unwrap();
    f.run(&mut app, summary);
    let after_refresh = app.next_git_preview().unwrap();
    f.run(&mut app, before_refresh);
    assert!(!render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    app.resize(47, 13);
    f.run(&mut app, after_refresh);
    assert!(app.next_git_preview().is_none());
    app.resize(140, 32);
    app.sync_git_preview();
    let before_page = app.next_git_preview().unwrap();
    wide_key(&mut app, KeyCode::Char('o'));
    assert!(app.tab == Tab::Overview);
    f.run(&mut app, before_page);
    assert!(!render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
    assert!(!app.busy && app.error.is_none());
}

#[test]
fn patch_errors_and_empty_output_fall_back_without_diagnostics_or_retries_until_refresh() {
    let f = Fixture::new();
    let mut app = f.app();
    f.choose(&mut app);
    for (marker, message) in [
        ("fail", "Could not read the selected patch"),
        ("no-hunks", "No patch for the selected change"),
    ] {
        let summary = wide_key(&mut app, KeyCode::Char('u')).unwrap();
        f.run(&mut app, summary);
        let preview = app.next_git_preview().unwrap();
        fs::write(f.root.join(marker), "").unwrap();
        f.run(&mut app, preview);
        let screen = render(&app, 140, 32);
        assert!(screen.contains("Git details") && screen.contains(message));
        assert!(!screen.contains("PRIVATE-PATCH-FAIL") && !screen.contains("Selected patch"));
        assert!(app.next_git_preview().is_none() && app.error.is_none());
        fs::remove_file(f.root.join(marker)).unwrap();
    }
    let summary = wide_key(&mut app, KeyCode::Char('R')).unwrap();
    f.run(&mut app, summary);
    let preview = app.next_git_preview().unwrap();
    f.run(&mut app, preview);
    assert!(render(&app, 140, 32).contains("ALPHA-UNSTAGED"));
}
