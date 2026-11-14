use super::*;
use dort_core::tools::install::{self, BinaryState, Outcome, PackageState, ProcessOutcome};
fn failed_result(f: &Fixture, app: &mut App, package: PackageState, process: ProcessOutcome) {
    f.review(app);
    let scope = app.tools_view.installation.scope.as_ref().unwrap().clone();
    app.tools_view.installation.installed(Ok(install::Report {
        scope,
        attempt_id: None,
        no_op: false,
        outcome: Some(Outcome {
            process,
            package,
            binary: BinaryState::Candidates,
        }),
        observed_versions: vec![],
        possible_partial_effects: true,
        readiness_verified: false,
    }));
}
fn isolated_detect(task: Task, f: &Fixture) -> Task {
    let Task::ToolsDetect(mut inputs) = task else {
        panic!("fresh review must only discover metadata")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    inputs.overrides = vec![tools::Request {
        query: Query::MiseInstalled,
        executable: f.executable.clone(),
        search_dirs: vec![f.bin.clone()],
        manifest: None,
        runtime_tool: None,
        npm_cli: None,
        prefix: None,
    }];
    Task::ToolsDetect(inputs)
}
#[test]
fn all_provider_failures_keep_presence_separate_and_fresh_entry_is_navigation_only() {
    for provider in 0..4 {
        let f = match provider {
            1 => Fixture::new_npm(),
            2 => Fixture::new_brew(),
            3 => Fixture::new_apt(),
            _ => Fixture::new(),
        };
        let mut app = match provider {
            1 => f.npm_app(false),
            2 => f.app_query(false, Query::BrewFormulae),
            3 => f.app_query(false, Query::DpkgPackages),
            _ => f.app(false),
        };
        let package = if provider >= 2 {
            PackageState::ProviderCurrentPresent
        } else {
            PackageState::ExactPresent
        };
        failed_result(&f, &mut app, package, ProcessOutcome::Failed);
        app.requested_profiles = vec!["retained-profile-draft".into()];
        app.tools_view.reconciliation.fields[2] = "retained-profile-draft".into();
        let repository = app.repository.clone();
        let calls = f.calls();
        let snapshots = serde_json::to_string(&app.tools_view.snapshots).unwrap();
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let screen = render(&app, w, h);
            assert!(
                screen.contains("Process failed")
                    && screen.contains("reported present")
                    && screen.contains("Start fresh review"),
                "{screen}"
            );
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| views::draw(frame, &app)).unwrap();
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .any(|c| c.bg == Color::Cyan)
            );
        }
        key(&mut app, KeyCode::Char('d'));
        assert!(key(&mut app, KeyCode::Enter).is_none());
        assert_eq!(calls, f.calls());
        let task = key(&mut app, KeyCode::Char('f')).unwrap();
        assert!(matches!(task, Task::ToolsDetect(_)));
        assert!(!app.tools_view.install_open);
        assert!(app.tools_view.setup && app.tools_view.fresh_review);
        assert_eq!(app.repository, repository);
        assert_eq!(app.requested_profiles, ["retained-profile-draft"]);
        assert_eq!(
            app.tools_view.reconciliation.fields[2],
            "retained-profile-draft"
        );
        assert!(
            app.tools_view.installation.preview.is_none()
                && app.tools_view.installation.close_preview.is_none()
        );
        assert_eq!(calls, f.calls());
        assert_eq!(
            snapshots,
            serde_json::to_string(&app.tools_view.snapshots).unwrap()
        );
        for s in &app.tools_view.snapshots {
            assert!(
                app.tools_view
                    .reconciliation
                    .stale_for_install
                    .contains(&s.id)
            );
        }
        assert!(!f.host.state_dir.join("tools-attempts").exists());
    }
}
#[test]
fn unavailable_process_observations_and_deadlines_are_not_claimed_native_exit_failures() {
    let f = Fixture::new();
    for (process, label) in [
        (ProcessOutcome::Wait, "Process status unknown"),
        (ProcessOutcome::Pipe, "Process status unknown"),
        (ProcessOutcome::Unavailable, "Installer unavailable"),
        (ProcessOutcome::Timeout, "Deadline elapsed"),
    ] {
        let mut app = f.app(false);
        failed_result(&f, &mut app, PackageState::ExactPresent, process);
        let before = f.calls();
        for size in [(48, 14), (80, 24)] {
            let text = render(&app, size.0, size.1);
            assert!(
                text.contains(label) && text.contains("reported present"),
                "{text}"
            );
            assert!(!text.contains("Process failed"));
        }
        assert_eq!(before, f.calls());
    }
}
#[test]
fn fresh_review_advanced_preserves_explicitly_empty_inventory_drafts() {
    let f = Fixture::new();
    let mut app = f.app(false);
    app.tools_view.query = 2;
    app.tools_view.fields[0] = f.executable.display().to_string();
    app.tools_view.fields[1] = f.bin.display().to_string();
    let _metadata_only = app.tools_view.configure(); // retains this query draft
    app.tools_view.fields = Default::default(); // an explicit later edit
    app.tools_view.install_open = true;
    app.tools_view.installation.mode = Mode::Result;
    let calls = f.calls();
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, isolated_detect(task, &f));
    app.tools_view.default_focus = 2;
    app.tools_view.select_advanced();
    assert!(app.tools_view.fields.iter().all(String::is_empty));
    assert!(app.tools_view.configuring);
    assert!(key(&mut app, KeyCode::Char('f')).is_none());
    assert!(app.tools_view.configuring);
    assert_eq!(calls, f.calls());
}
#[test]
fn fresh_query_still_needs_approval_new_binding_and_new_install_review() {
    let f = Fixture::new();
    let mut app = f.app(false);
    failed_result(&f, &mut app, PackageState::Absent, ProcessOutcome::Success);
    let old = serde_json::to_string(&app.tools_view.snapshots).unwrap();
    let calls = f.calls();
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, isolated_detect(task, &f));
    assert_eq!(calls, f.calls());
    assert_eq!(
        old,
        serde_json::to_string(&app.tools_view.snapshots).unwrap()
    );
    for (w, h) in [(48, 14), (80, 24)] {
        let text = render(&app, w, h);
        assert!(
            text.contains("Fresh review") && text.contains("no replay"),
            "{text}"
        );
    }
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolsInspectSelected(_)));
    assert_eq!(calls, f.calls());
    f.run(&mut app, task);
    assert_eq!(f.calls(), format!("{calls}Q"));
    assert_eq!(app.tools_view.snapshots.len(), 2);
    assert!(
        app.tools_view.reconciliation.bindings.is_empty(),
        "collection must not bind new snapshots"
    );
    let request = reconciliation::Request {
        manifest: f.repo.join("dort.toml"),
        tool_search_dirs: vec![f.bin.clone()],
        profiles: vec![],
    };
    let prepared = request.prepare(&f.host).unwrap();
    let old = prepared
        .reconcile(
            &app.tools_view.snapshots,
            [("demo".into(), "s1".into())].into(),
        )
        .unwrap();
    app.tools_view.reconciliation.completed(Ok(old));
    app.tools_view.recon_open = true;
    assert!(key(&mut app, KeyCode::Char('i')).is_none());
    assert!(render(&app, 48, 14).contains("Collect fresh inventory"));
    let fresh = request
        .prepare(&f.host)
        .unwrap()
        .reconcile(
            &app.tools_view.snapshots,
            [("demo".into(), "s2".into())].into(),
        )
        .unwrap();
    app.tools_view.reconciliation.completed(Ok(fresh));
    let before = f.calls();
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    assert!(matches!(task, Task::ToolInstallPrepare(_)));
    f.run(&mut app, task);
    assert_eq!(before, f.calls());
    assert!(app.tools_view.installation.preview.is_some());
    assert!(!f.host.state_dir.join("tools-attempts").exists());
    // A later manifest change still invalidates this new approval; navigation did
    // not persist trust or excuse the independent writer guards.
    let manifest = f.repo.join("dort.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap() + "\n# changed after review\n",
    )
    .unwrap();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert!(app.tools_view.installation.error.is_some());
    assert_eq!(before, f.calls());
    assert!(app.tools_view.installation.preview.is_none());
}
#[test]
fn unresolved_and_closed_attempt_details_never_offer_replay_or_implied_success() {
    let f = Fixture::new();
    let mut app = f.app(false);
    app.tools_view.recon_open = false;
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let id = "a".repeat(64);
    fs::create_dir(root.join(&id)).unwrap();
    fs::set_permissions(root.join(&id), fs::Permissions::from_mode(0o700)).unwrap();
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.run(&mut app, task);
    for size in [(48, 14), (80, 24)] {
        let text = render(&app, size.0, size.1);
        assert!(
            text.contains("UNRESOLVED")
                && text.contains("Review closure")
                && text.contains("Start fresh review"),
            "{text}"
        );
    }
    let calls = f.calls();
    let before = fs::read_dir(&root).unwrap().count();
    key(&mut app, KeyCode::Char('d'));
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, isolated_detect(task, &f));
    assert_eq!(calls, f.calls());
    assert_eq!(before, fs::read_dir(&root).unwrap().count());
    assert!(install::diagnose(&f.host).unwrap()[0].unresolved);
    let query = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, query);
    let comparison = reconciliation::Request {
        manifest: f.repo.join("dort.toml"),
        tool_search_dirs: vec![f.bin.clone()],
        profiles: vec![],
    }
    .prepare(&f.host)
    .unwrap()
    .reconcile(
        &app.tools_view.snapshots,
        [("demo".into(), "s2".into())].into(),
    )
    .unwrap();
    app.tools_view.reconciliation.completed(Ok(comparison));
    app.tools_view.recon_open = true;
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.run(&mut app, task);
    let before_write = f.calls();
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    assert_eq!(before_write, f.calls());
    assert!(
        app.tools_view.installation.error.is_some()
            && app.tools_view.installation.preview.is_none()
    );
    assert!(install::diagnose(&f.host).unwrap()[0].unresolved);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    f.run(&mut app, task);
    key(&mut app, KeyCode::Char('c')); // damaged scope form
    app.tools_view.installation.fields[0] = f.executable.display().to_string();
    app.tools_view.installation.fields[1] = f.bin.display().to_string();
    app.tools_view.installation.focus = 2;
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    f.run(&mut app, task);
    let a = &app.tools_view.installation.attempts[0];
    assert_eq!(a.summary().status, install::Status::ExplicitlyClosed);
    assert!(!a.summary().readiness_verified);
    assert!(render(&app, 48, 14).contains("Explicitly closed"));
    key(&mut app, KeyCode::Char('d'));
    assert!(
        serde_json::to_string(&app.tools_view.installation.attempts[0])
            .unwrap()
            .contains("no install success")
    );
    assert!(!f.calls().contains('I'));
}
#[test]
fn fresh_entry_is_safe_in_preview_only_small_and_late_result_states() {
    let f = Fixture::new();
    let mut app = f.app(true);
    failed_result(
        &f,
        &mut app,
        PackageState::Unavailable,
        ProcessOutcome::Success,
    );
    let calls = f.calls();
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    f.run(&mut app, isolated_detect(task, &f));
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert_eq!(calls, f.calls());
    let mut app = f.app(false);
    failed_result(
        &f,
        &mut app,
        PackageState::Conflict,
        ProcessOutcome::Success,
    );
    let task = key(&mut app, KeyCode::Char('f')).unwrap();
    key(&mut app, KeyCode::Esc);
    f.run(&mut app, isolated_detect(task, &f));
    assert!(app.tools_view.installation.preview.is_none());
    assert_eq!(f.calls(), format!("{calls}Q"));
    let mut app = f.app(false);
    failed_result(&f, &mut app, PackageState::Absent, ProcessOutcome::Failed);
    assert!(
        app.key(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
            47,
            13
        )
        .is_none()
    );
    assert!(app.tools_view.installation.preview.is_none());
}
