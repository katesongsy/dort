//! Complete foreground-control journeys with disposable metadata/fake providers only.
use super::*;
use crate::tui::{input::Target, shell::Action};

#[test]
fn more_exposes_install_review_but_never_approval_and_dismissal_retains_the_real_preview() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let snapshots = serde_json::to_string(&app.tools_view.snapshots).unwrap();
    f.review(&mut app); // Real More -> process-free review preparation.
    assert_eq!(f.calls(), "Q");
    render(&app, 80, 24);
    assert!(key(&mut app, KeyCode::F(2)).is_none());
    let menu = app.more.as_ref().unwrap();
    assert!(menu.items.iter().all(|item| item.action.key != "Enter"
        && !matches!(
            item.action.target,
            Target::ToolInstallControl(_, true)
                | Target::ConfirmAction
                | Target::ProviderConfirm
                | Target::PatchConfirm
        )));
    assert!(app.tools_view.installation.preview.is_some());
    assert!(key(&mut app, KeyCode::Esc).is_none());
    assert!(app.tools_view.installation.preview.is_some());
    assert_eq!(f.calls(), "Q");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolInstall(_)));
    f.run(&mut app, task);
    assert_eq!(f.calls(), "QQIQ");
    assert_eq!(
        serde_json::to_string(&app.tools_view.snapshots).unwrap(),
        snapshots
    );
}

#[test]
fn historical_closure_is_discoverable_in_more_without_querying_or_approving() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let directory = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let damaged = directory.join("d".repeat(64));
    fs::create_dir(&damaged).unwrap();
    fs::set_permissions(&damaged, fs::Permissions::from_mode(0o700)).unwrap();
    let task = app.activate(Action::ToolAttempts).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.attempts[0].unresolved);
    assert!(
        tests::more_target(&mut app, |target| matches!(
            target,
            Target::ToolAttemptClose
        ))
        .is_none()
    );
    assert!(app.tools_view.installation.mode == Mode::CloseInputs);
    assert!(app.tools_view.installation.close_preview.is_none());
    assert_eq!(f.calls(), "Q");
    tests::cycle_closure_provider(&mut app); // Explicitly choose npm.
    app.tools_view.installation.fields = [
        "/draft/node".into(),
        "/draft/bin".into(),
        "/draft/npm-cli.js".into(),
        "/draft/changed-prefix".into(),
    ];
    let draft = app.tools_view.installation.fields.clone();
    assert!(key(&mut app, KeyCode::F(3)).is_none());
    assert!(app.tools_view.installation.draft_metadata());
    let screen = render(&app, 80, 24);
    assert!(screen.contains("changed-prefix") && screen.contains("Trusted Node"));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(key(&mut app, KeyCode::F(4)).is_none());
    assert!(app.choice.is_none());
    assert!(key(&mut app, KeyCode::Esc).is_none());
    assert!(!app.tools_view.installation.draft_metadata());
    assert_eq!(app.tools_view.installation.fields, draft);
    assert_eq!(f.calls(), "Q");
}

#[test]
fn draft_git_metadata_is_complete_escaped_and_enter_cannot_read_hidden_fields() {
    let mut app = App::new(false);
    app.tab = Tab::Git;
    app.git_view.editing_root = true;
    app.git_view.draft = format!("/PREFIX/{}/\u{202e}\u{1b}[31m/TAIL", "path/".repeat(30));
    let draft = app.git_view.draft.clone();
    assert!(key(&mut app, KeyCode::F(3)).is_none());
    assert!(app.git_view.root_metadata());
    let mut seen = String::new();
    for _ in 0..30 {
        seen.push_str(&render(&app, 48, 14));
        assert!(key(&mut app, KeyCode::PageDown).is_none());
    }
    assert!(seen.contains("PREFIX") && seen.contains("TAIL"));
    assert!(seen.contains("\\u{202e}") && seen.contains("\\u{1b}"));
    assert!(!seen.contains('\u{202e}') && !seen.contains('\u{1b}'));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.error.is_none() && app.git_view.metadata.is_none());
    assert!(key(&mut app, KeyCode::Esc).is_none());
    assert!(!app.git_view.root_metadata() && app.git_view.editing_root);
    assert_eq!(app.git_view.draft, draft);
}

#[test]
fn foreground_work_navigation_explains_pause_without_losing_the_parent_draft() {
    let mut app = App::new(false);
    app.shell.overlay = Some(crate::tui::shell::Overlay::Form(
        crate::tui::shell::Form::Init,
    ));
    app.shell.init_draft = "/retained/draft".into();
    app.busy = true;
    for code in [KeyCode::Right, KeyCode::Left, KeyCode::Esc, KeyCode::Tab] {
        assert!(key(&mut app, code).is_none());
        assert!(app.tab == Tab::Overview && app.busy);
        assert!(app.interaction_notice.is_some());
        assert!(app.shell.overlay.is_some());
        assert_eq!(app.shell.init_draft, "/retained/draft");
    }
    assert!(key(&mut app, KeyCode::Char('q')).is_none());
    assert!(app.exit_requested && !app.quit && app.busy);
}
