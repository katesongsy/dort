//! Normal multi-provider installation and explicit historical closure, without snapshot UI.
use super::{
    actions::Task,
    app::App,
    tests::render,
    tools_install::Mode,
    tools_provider_fixture::{checked, fixture},
    tools_tests::{app, check, key, run},
};
use crate::services::{declared_tools_tests::Fixture, installation};
use dort_core::{model::Provider, state::WriterLock, tools::install};
use ratatui::{Terminal, backend::TestBackend, crossterm::event::KeyCode};
use std::{fs, os::unix::fs::PermissionsExt};

fn configure_npm(f: &Fixture, app: &mut App) {
    let tools = app
        .data
        .as_ref()
        .unwrap()
        .tools
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    let index = tools.iter().position(|t| t.id == "demo").unwrap();
    if tools[index].install.provider != Provider::Npm {
        return;
    }
    app.tools_view.declared_selected = index;
    super::tools_tests::settings(f, app);
    app.tools_view.settings.fields[3] = f.host.home.join("globals").display().to_string();
    let task = key(app, KeyCode::Enter).unwrap();
    run(f, app, task);
    assert!(
        f.calls().is_empty(),
        "Necessary npm destination settings are process-free"
    );
}
fn install(f: &Fixture, app: &mut App) {
    configure_npm(f, app);
    check(f, app);
    let task = key(app, KeyCode::Char('i')).unwrap();
    run(f, app, task);
    assert!(
        app.tools_view
            .declared
            .install_preview
            .as_ref()
            .is_some_and(|p| p.len() == 1)
    );
    let task = key(app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolsInstallMissing(_)));
    run(f, app, task);
}
fn known_unresolved(f: &Fixture) {
    let checked = checked(f);
    let preview =
        installation::Request::selected(&checked.reconciliation, "demo", &checked.snapshots)
            .unwrap()
            .prepare(&f.host)
            .unwrap();
    let lock = WriterLock::acquire(&f.host).unwrap();
    install::execute(&f.host, &lock, preview.approve(), |_| {}).unwrap();
    let attempt = install::diagnose(&f.host).unwrap().remove(0);
    fs::remove_file(
        f.host
            .state_dir
            .join("tools-attempts")
            .join(attempt.id)
            .join("outcome.json"),
    )
    .unwrap();
}
fn damaged(f: &Fixture) {
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let record = root.join("a".repeat(64));
    fs::create_dir(&record).unwrap();
    fs::set_permissions(record, fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn all_providers_use_one_real_review_show_exact_scope_and_revalidate_without_replay() {
    for provider in [Provider::Brew, Provider::Npm] {
        let f = fixture(provider);
        let mut app = app(&f, false);
        configure_npm(&f, &mut app);
        check(&f, &mut app);
        let before = f.calls();
        let task = key(&mut app, KeyCode::Char('i')).unwrap();
        run(&f, &mut app, task);
        assert_eq!(f.calls(), before, "Review preparation is process-free");
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let text = render(&app, w, h);
            assert!(
                text.contains("Runs package-manager") && text.contains("no rollback"),
                "{provider:?}: {text}"
            );
            assert!(text.lines().last().unwrap().contains("F1 Help"));
        }
        let review = app
            .tools_view
            .declared
            .install_preview
            .as_ref()
            .unwrap()
            .review();
        assert_eq!(review.installations.len(), 1);
        assert!(!review.warning.contains("root hooks"));
        let task = key(&mut app, KeyCode::Enter).unwrap();
        run(&f, &mut app, task);
        let report = app.tools_view.declared.installed.as_ref().unwrap();
        assert!(report.succeeded(), "{provider:?}");
        assert!(f.host.home.join("installed").exists());
        let calls = f.calls();
        assert!(key(&mut app, KeyCode::Enter).is_none()); // Done.
        assert!(key(&mut app, KeyCode::Enter).is_none()); // Selected-tool details, never install again.
        assert_eq!(f.calls(), calls);
        assert!(app.tools_view.declared_details);
        assert!(!render(&app, 140, 40).contains("PRIVATE_OUTPUT"));
    }
}
#[test]
fn failed_process_with_package_present_stays_honest_in_result_and_history_for_every_provider() {
    for provider in [Provider::Brew, Provider::Npm] {
        let f = fixture(provider);
        fs::write(f.host.home.join("fail"), "fixture").unwrap();
        let mut app = app(&f, false);
        install(&f, &mut app);
        let report = app.tools_view.declared.installed.as_ref().unwrap().results[0]
            .report
            .as_ref()
            .unwrap();
        let summary = report.summary();
        assert_eq!(summary.status, install::Status::ProcessFailed);
        assert!(matches!(
            summary.package,
            Some(
                install::PackageState::ExactPresent | install::PackageState::ProviderCurrentPresent
            )
        ));
        key(&mut app, KeyCode::Enter);
        let task = key(&mut app, KeyCode::Char('h')).unwrap();
        run(&f, &mut app, task);
        let calls = f.calls();
        key(&mut app, KeyCode::Enter);
        let text = render(&app, 140, 40);
        assert!(
            text.contains("Process failed") && text.contains("reported present"),
            "{text}"
        );
        assert!(!text.contains("PRIVATE_OUTPUT") && !text.contains("Replay"));
        assert_eq!(f.calls(), calls);
    }
}
#[test]
fn known_history_scope_closes_only_after_one_fresh_review_and_never_reinstalls() {
    for provider in [Provider::Brew, Provider::Npm] {
        let f = fixture(provider);
        known_unresolved(&f);
        let calls = f.calls();
        let mut app = app(&f, false);
        let task = key(&mut app, KeyCode::Char('h')).unwrap();
        run(&f, &mut app, task);
        assert!(render(&app, 80, 24).contains("UNRESOLVED"));
        key(&mut app, KeyCode::Enter);
        assert!(app.tools_view.installation.details);
        let task = key(&mut app, KeyCode::Char('x')).unwrap();
        run(&f, &mut app, task);
        assert!(app.tools_view.installation.mode == Mode::CloseReview);
        assert_eq!(
            f.calls(),
            calls,
            "Retained scope needs no repeated configuration or query before approval"
        );
        assert!(render(&app, 140, 40).contains("Close installation record"));
        let task = key(&mut app, KeyCode::Enter).unwrap();
        assert!(matches!(task, Task::ToolClose(_)));
        run(&f, &mut app, task);
        let attempts = install::diagnose(&f.host).unwrap();
        assert!(!attempts[0].unresolved && attempts[0].explicitly_closed);
        assert_eq!(attempts.len(), 1);
        let after = f.calls();
        if provider == Provider::Brew {
            assert_eq!(
                after.matches("install --formula").count(),
                calls.matches("install --formula").count()
            );
        } else {
            assert_eq!(after.matches('I').count(), calls.matches('I').count());
        }
    }
}
#[test]
fn damaged_history_asks_only_necessary_scope_values_retains_drafts_and_respects_dry_run() {
    for provider in [Provider::Brew, Provider::Npm] {
        let f = fixture(provider);
        damaged(&f);
        let mut app = app(&f, false);
        let task = key(&mut app, KeyCode::Char('h')).unwrap();
        run(&f, &mut app, task);
        assert!(key(&mut app, KeyCode::Char('x')).is_none());
        assert!(app.tools_view.installation.mode == Mode::CloseInputs);
        app.tools_view
            .installation
            .choose_closure_provider(match provider {
                Provider::Npm => 0,
                Provider::Brew => 1,
            });
        let state = &mut app.tools_view.installation;
        state.fields[0] = f
            .bin
            .join(match provider {
                Provider::Npm => "node",
                Provider::Brew => "brew",
            })
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        state.fields[1] = f.bin.display().to_string();
        if provider == Provider::Npm {
            state.fields[2] = f
                .bin
                .parent()
                .unwrap()
                .join("lib/node_modules/npm/bin/npm-cli.js")
                .display()
                .to_string();
            state.fields[3] = f.bin.parent().unwrap().display().to_string();
        }
        if provider == Provider::Brew {
            state.fields[2] = f.host.home.join("prefix").display().to_string();
        }
        let draft = state.fields.clone();
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            for focus in 0..app.tools_view.installation.close_field_count() {
                app.tools_view.installation.focus = focus;
                super::tests::focus_form(&mut app);
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| super::views::draw(f, &app)).unwrap();
                let cursor = terminal.get_cursor_position().unwrap();
                super::tests::assert_text_focus(&terminal, (w, h), cursor);
            }
        }
        key(&mut app, KeyCode::F(1));
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.tools_view.installation.fields, draft);
        assert!(f.calls().is_empty());
        let task = key(&mut app, KeyCode::Enter).unwrap();
        run(&f, &mut app, task);
        assert!(app.tools_view.installation.close_preview.is_some());
        assert!(f.calls().is_empty());
        app.dry_run = true;
        assert!(key(&mut app, KeyCode::Enter).is_none());
        app.dry_run = false;
        let task = key(&mut app, KeyCode::Enter).unwrap();
        run(&f, &mut app, task);
        assert!(install::diagnose(&f.host).unwrap()[0].explicitly_closed);
        assert!(!f.host.home.join("installed").exists());
    }
}
#[test]
fn unavailable_history_is_not_a_healthy_empty_list_and_closure_navigation_clears_approval() {
    let f = fixture(Provider::Brew);
    let mut app = app(&f, false);
    fs::write(f.host.state_dir.join("tools-attempts"), "unavailable").unwrap();
    let task = key(&mut app, KeyCode::Char('h')).unwrap();
    run(&f, &mut app, task);
    assert!(app.tools_view.installation.error.is_some());
    assert!(!render(&app, 80, 24).contains("No installation"));
    assert!(f.calls().is_empty());
}
