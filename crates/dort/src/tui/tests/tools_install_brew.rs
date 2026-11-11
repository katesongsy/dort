use super::*;
#[test]
fn native_brew_review_is_one_activation_current_not_exact_and_stales_catalogs() {
    let f = Fixture::new_brew();
    let mut app = f.app_query(false, Query::BrewFormulae);
    let snapshots = serde_json::to_string(&app.tools_view.snapshots).unwrap();
    let calls = f.calls();
    f.review(&mut app);
    assert_eq!(calls, f.calls());
    for size in [(48, 14), (80, 24), (140, 40)] {
        let screen = render(&app, size.0, size.1);
        assert!(
            screen.contains("CAN EXECUTE")
                && screen.contains("native links")
                && screen.contains("No rollback"),
            "{screen}"
        );
        assert!(screen.contains("Install core formula"));
        assert!(!screen.contains("Install exact version"));
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
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(f.calls().matches("install --formula").count(), 1);
}
#[test]
fn advanced_brew_prefix_is_retained_process_free_with_compact_focus_and_caret() {
    let f = Fixture::new_brew();
    let mut app = f.app_query(false, Query::BrewFormulae);
    f.review(&mut app);
    let calls = f.calls();
    key(&mut app, KeyCode::Char('a'));
    assert!(app.tools_view.installation.mode == Mode::PrefixInputs);
    assert!(app.tools_view.installation.preview.is_none());
    key(&mut app, KeyCode::BackTab);
    for size in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal.draw(|frame| views::draw(frame, &app)).unwrap();
        let cursor = terminal.get_cursor_position().unwrap();
        assert!(cursor.x < size.0 && cursor.y < size.1 - 2);
        assert_eq!(
            terminal.backend().buffer()[(cursor.x, cursor.y - 1)].fg,
            Color::Cyan
        );
    }
    // Printable global shortcuts remain input, not navigation/quit.
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Tab);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(calls, f.calls());
    assert!(app.tools_view.installation.preview.is_some());
    key(&mut app, KeyCode::Char('a'));
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.installation.preview.is_none());
    let task = key(&mut app, KeyCode::Char('r')).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_some());
    assert_eq!(calls, f.calls());
}
#[test]
fn brew_prerequisite_freshness_includes_npm_manifest_provenance() {
    let f = Fixture::new_brew();
    let manifest = "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\nrequires=['pkg']\n[tools.install]\nmacos={provider='brew',package='demo'}\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='20.1.0'}\n[[tools]]\nid='pkg'\nbinary='pkg'\n[tools.install]\nmacos={provider='npm',package='pkg',version='1.2.3',runtime_tool='node'}\n";
    fs::write(f.repo.join("dort.toml"), manifest).unwrap();
    let mise = f.host.home.join("mise");
    fs::write(
        &mise,
        "#!/bin/sh\nprintf '{\"node\":[{\"version\":\"20.1.0\",\"installed\":true}]}'\n",
    )
    .unwrap();
    let node = f.bin.join("node");
    fs::write(
        &node,
        "#!/bin/sh\nprintf '{\"dependencies\":{\"pkg\":{\"version\":\"1.2.3\"}}}'\n",
    )
    .unwrap();
    for p in [&mise, &node] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let cli = f.host.home.join("npm-cli.js");
    fs::write(&cli, "METADATA_ONLY").unwrap();
    let mut app = f.app_query(false, Query::BrewFormulae);
    for (id, query, executable) in [
        ("runtime", Query::MiseInstalled, mise),
        ("npm", Query::NpmGlobals, node),
    ] {
        let npm = query == Query::NpmGlobals;
        let report = tools::Request {
            query,
            executable,
            search_dirs: vec![f.bin.clone()],
            manifest: npm.then(|| f.repo.join("dort.toml")),
            runtime_tool: npm.then(|| "node".into()),
            npm_cli: npm.then(|| cli.clone()),
            prefix: npm.then(|| f.host.home.join("npm-prefix")),
        }
        .prepare(&f.host)
        .unwrap()
        .approve()
        .inspect();
        app.tools_view.snapshots.push(Snapshot {
            id: id.into(),
            report: std::sync::Arc::new(report),
        });
    }
    let report = reconciliation::Request {
        manifest: f.repo.join("dort.toml"),
        tool_search_dirs: vec![f.bin.clone()],
        profiles: vec![],
    }
    .prepare(&f.host)
    .unwrap()
    .reconcile(
        &app.tools_view.snapshots,
        [
            ("demo".into(), "s1".into()),
            ("node".into(), "runtime".into()),
            ("pkg".into(), "npm".into()),
        ]
        .into(),
    )
    .unwrap();
    let selected = report
        .plan
        .rows
        .iter()
        .position(|r| r.id == "demo")
        .unwrap();
    app.tools_view.reconciliation.completed(Ok(report));
    app.tools_view.reconciliation.selected = selected;
    app.tools_view
        .reconciliation
        .stale_for_install
        .insert("npm".into());
    assert!(render(&app, 80, 24).contains("Collect fresh inventory"));
    assert!(key(&mut app, KeyCode::Char('i')).is_none());
    app.tools_view.reconciliation.stale_for_install.clear();
    fs::write(f.repo.join("dort.toml"), format!("{manifest}\n# changed\n")).unwrap();
    let calls = f.calls();
    f.review(&mut app);
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(calls, f.calls());
    assert!(
        app.tools_view
            .installation
            .error
            .as_ref()
            .unwrap()
            .contains("npm snapshot manifest changed")
    );
}
#[test]
fn brew_dry_run_floor_and_late_results_never_authorize() {
    let f = Fixture::new_brew();
    let mut app = f.app_query(true, Query::BrewFormulae);
    let calls = f.calls();
    f.review(&mut app);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(calls, f.calls());
    let mut app = f.app_query(false, Query::BrewFormulae);
    f.review(&mut app);
    let calls = f.calls();
    assert!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 47, 13)
            .is_none()
    );
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(calls, f.calls());
    // Leaving drops authority; a delayed prepare cannot reopen its review.
    let mut app = f.app_query(false, Query::BrewFormulae);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    key(&mut app, KeyCode::Esc);
    f.run(&mut app, task);
    assert!(app.tools_view.installation.preview.is_none());
}
#[test]
fn damaged_brew_closure_requests_only_executable_path_and_prefix_with_retained_drafts() {
    let f = Fixture::new_brew();
    let mut app = f.app_query(false, Query::BrewFormulae);
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let dir = root.join("b".repeat(64));
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let task = app.tools_view.open_attempts();
    f.run(&mut app, task);
    key(&mut app, KeyCode::Char('c'));
    assert!(app.tools_view.installation.close_brew);
    assert!(app.tools_view.installation.mode == Mode::CloseInputs);
    let expected = app.tools_view.installation.fields.clone();
    for size in [(48, 14), (80, 24), (140, 40)] {
        for focus in 0..5 {
            app.tools_view.installation.focus = focus;
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
            terminal.draw(|frame| views::draw(frame, &app)).unwrap();
            if focus < 3 {
                let cursor = terminal.get_cursor_position().unwrap();
                assert!(cursor.x < size.0 && cursor.y < size.1 - 2);
                assert_eq!(
                    terminal.backend().buffer()[(cursor.x, cursor.y - 1)].fg,
                    Color::Cyan
                );
            } else {
                let screen = render(&app, size.0, size.1);
                assert!(screen.contains(if focus == 3 {
                    "Review closure"
                } else {
                    "Provider: brew"
                }));
            }
        }
    }
    app.tools_view.installation.focus = 4;
    key(&mut app, KeyCode::Enter); // apt
    app.tools_view.installation.focus = 3;
    key(&mut app, KeyCode::Enter); // mise
    app.tools_view.installation.focus = 3;
    key(&mut app, KeyCode::Enter); // npm
    app.tools_view.installation.focus = 5;
    key(&mut app, KeyCode::Enter); // brew
    assert_eq!(expected, app.tools_view.installation.fields);
    app.tools_view.installation.focus = 3;
    let calls = f.calls();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(calls, f.calls());
    assert!(app.tools_view.installation.mode == Mode::CloseReview);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.attempts[0].explicitly_closed);
    assert!(!f.calls().contains("install --formula"));
}
