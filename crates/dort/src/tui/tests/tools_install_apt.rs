use super::*;
impl Fixture {
    pub(super) fn new_apt() -> Self {
        let mut f = Self::new();
        fs::write(f.repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nubuntu={provider='apt',package='demo'}\n").unwrap();
        let programs = f.host.home.join("programs");
        fs::create_dir(&programs).unwrap();
        f.executable = programs.join("dpkg-query");
        for (name, body) in [
            (
                "dpkg-query",
                "printf D >> \"$HOME/calls\"; if [ -f \"$HOME/installed\" ]; then printf 'demo\\t2.7\\tinstalled\\tok\\n'; fi",
            ),
            (
                "apt-cache",
                "printf C >> \"$HOME/calls\"; printf 'demo:\\n  Installed: (none)\\n  Candidate: 2.7\\n'",
            ),
            (
                "apt-get",
                "printf I >> \"$HOME/calls\"; printf installed > \"$HOME/installed\"",
            ),
            ("sudo", "printf S >> \"$HOME/calls\"; shift 6; \"$@\""),
        ] {
            let p = programs.join(name);
            fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        f
    }
}
#[test]
fn explicitly_empty_apt_closure_draft_is_not_rebound_from_snapshot_hints() {
    let f = Fixture::new_apt();
    let app = f.app_query(false, Query::DpkgPackages);
    let mut state = crate::tui::tools_install::State::default();
    state.open_attempts(&app.tools_view.snapshots);
    assert!(state.close_apt);
    state.mode = Mode::CloseInputs;
    state.fields[0].clear();
    state.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        false,
        &app.tools_view.snapshots,
    );
    state.open_attempts(&app.tools_view.snapshots);
    assert!(state.fields[0].is_empty() && state.close_apt);
    state.mode = Mode::CloseInputs;
    for expected in ["mise", "npm", "brew", "apt"] {
        state.choose_closure_provider((state.closure_provider() + 1) % 4);
        assert_eq!(
            if state.close_apt {
                "apt"
            } else if state.close_brew {
                "brew"
            } else if state.close_npm {
                "npm"
            } else {
                "mise"
            },
            expected
        );
        assert_eq!(state.focus, 0);
    }
    assert!(state.close_apt && state.fields[0].is_empty());
}
#[test]
fn apt_root_review_one_activation_and_immutable_stale_snapshots() {
    let f = Fixture::new_apt();
    let mut app = f.app_query(false, Query::DpkgPackages);
    let snapshots = serde_json::to_string(&app.tools_view.snapshots).unwrap();
    let calls = f.calls();
    f.review(&mut app);
    assert_eq!(calls, f.calls());
    for size in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, size.0, size.1);
        assert!(
            screen.contains("ROOT RISK")
                && screen.contains("CAN EXECUTE")
                && super::tests::has_action(&app, "Install with root risk"),
            "{screen}"
        );
        assert!(!super::tests::has_action(&app, "Install exact version"));
    }
    key(&mut app, KeyCode::Char('d'));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(calls, f.calls());
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolInstall(_)));
    f.run(&mut app, task);
    assert!(render(&app, 80, 24).contains("not exact-pinned"));
    assert_eq!(
        snapshots,
        serde_json::to_string(&app.tools_view.snapshots).unwrap()
    );
    assert!(
        app.tools_view
            .reconciliation
            .stale_for_install
            .contains("s1")
    );
    assert_eq!(f.calls().matches('I').count(), 1);
    assert!(app.tools_view.installation.preview.is_none());
}
#[test]
fn apt_advanced_retains_three_missing_inputs_and_compact_caret_without_processes() {
    let f = Fixture::new_apt();
    let mut app = f.app_query(false, Query::DpkgPackages);
    f.review(&mut app);
    let calls = f.calls();
    key(&mut app, KeyCode::Char('a'));
    assert!(app.tools_view.installation.mode == Mode::AptInputs);
    assert!(app.tools_view.installation.preview.is_none());
    // Co-located metadata hints fill fields; Shift-Tab focuses sudo.
    key(&mut app, KeyCode::BackTab);
    for size in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal.draw(|frame| views::draw(frame, &app)).unwrap();
        let cursor = terminal.get_cursor_position().unwrap();
        assert!(cursor.x < size.0 && cursor.y < size.1 - 2);
        super::tests::assert_text_focus(&terminal, size, cursor);
    }
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.installation.preview.is_none());
    let task = key(&mut app, KeyCode::Char('R')).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_some());
    assert_eq!(calls, f.calls());
    fs::remove_file(f.executable.parent().unwrap().join("sudo")).unwrap();
    key(&mut app, KeyCode::Char('a'));
    app.tools_view.installation.focus = 2;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(calls, f.calls());
    key(&mut app, KeyCode::Esc); // dismiss the owned read-failure surface
    key(&mut app, KeyCode::Char('a'));
    assert!(render(&app, 80, 24).contains("sudo"));
}
#[test]
fn apt_dry_floor_shrink_and_late_results_never_authorize() {
    let f = Fixture::new_apt();
    let mut app = f.app_query(true, Query::DpkgPackages);
    f.review(&mut app);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    let mut app = f.app_query(false, Query::DpkgPackages);
    f.review(&mut app);
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(matches!(
        key(&mut app, KeyCode::Enter),
        Some(Task::ToolsDetect(_))
    ));
    assert!(app.tools_view.installation.preview.is_none());
    assert!(app.tools_view.fresh_review);
    let mut app = f.app_query(false, Query::DpkgPackages);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    key(&mut app, KeyCode::Esc);
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_none());
    assert!(!f.calls().contains('I'));
}
#[test]
fn apt_damaged_closure_uses_only_two_dpkg_fields_and_retains_drafts() {
    let f = Fixture::new_apt();
    let mut app = f.app_query(false, Query::DpkgPackages);
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let id = "e".repeat(64);
    fs::create_dir(root.join(&id)).unwrap();
    fs::set_permissions(root.join(&id), fs::Permissions::from_mode(0o700)).unwrap();
    app.tools_view.recon_open = false;
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.run(&mut app, task);
    let task = key(&mut app, KeyCode::Char('C'));
    assert!(task.is_none());
    assert!(app.tools_view.installation.close_apt);
    for size in [(48, 14), (80, 24)] {
        let screen = render(&app, size.0, size.1);
        assert!(screen.contains("dpkg-query"));
        assert!(!screen.contains("sudo") && !screen.contains("npm CLI"));
    }
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Backspace);
    let fields = app.tools_view.installation.fields.clone();
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('C'));
    assert_eq!(fields, app.tools_view.installation.fields);
    app.tools_view.installation.focus = 2;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.mode == Mode::CloseReview);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(!app.tools_view.installation.attempts[0].unresolved);
    assert!(!f.calls().contains('S'));
}
