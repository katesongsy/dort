//! End-to-end declared Tools reviews, freshness and actual-cell exports.
use super::{
    actions::{self, Task},
    app::Tab,
    shell::Action,
    tests::render,
    tools_tests::{app, check, key, run},
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::KeyCode;

#[test]
fn declaration_list_checks_and_installs_without_internal_choices_or_contextual_enter() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    assert!(f.calls().is_empty());
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let text = render(&app, w, h);
        assert!(text.contains("Not checked"));
        let strip = super::workbench::strip_actions(&app, w).0;
        assert!(
            strip
                .iter()
                .any(|a| a.key == "Enter" && a.label.to_lowercase().contains("details"))
        );
    }
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.tools_view.declared_details);
    check(&f, &mut app);
    assert_eq!(f.calls(), "Q");
    key(&mut app, KeyCode::Esc);
    assert!(!app.tools_view.declared_details);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.tools_view.declared_details);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    run(&f, &mut app, task);
    assert_eq!(f.calls(), "Q");
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let text = render(&app, w, h);
        assert!(text.contains("2 ready to install"));
        assert!(text.contains("Runs package-manager") && text.contains("no rollback"));
        assert!(!text.contains("root hooks"));
    }
    let task = super::tests::approve_ready(&mut app, 80, 24).unwrap();
    run(&f, &mut app, task);
    assert!(
        app.tools_view
            .declared
            .installed
            .as_ref()
            .unwrap()
            .succeeded()
    );
    assert_eq!(f.calls().matches('A').count(), 1);
    assert_eq!(f.calls().matches('B').count(), 1);
    assert!(!render(&app, 140, 40).contains("PRIVATE_PROVIDER_OUTPUT"));
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.declared_details);
    assert!(key(&mut app, KeyCode::Enter).is_none());
}
#[test]
fn empty_review_explains_why_and_opens_affected_details_or_edit_without_execution() {
    for edit in [false, true] {
        let f = Fixture::new();
        for name in ["a", "b"] {
            f.program(name, "exit 99");
        }
        let mut app = app(&f, false);
        check(&f, &mut app);
        let task = key(&mut app, KeyCode::Char('i')).unwrap();
        run(&f, &mut app, task);
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let text = render(&app, w, h);
            assert!(text.contains("Installation needs attention"));
            assert!(text.contains("A command exists"), "{text}");
            assert!(
                !text.contains("Runs package-manager")
                    && !text.contains("root hooks")
                    && !text.contains("0 missing")
            );
            assert!(text.lines().last().unwrap().contains("F1 Help"));
        }
        key(
            &mut app,
            if edit {
                KeyCode::Char('e')
            } else {
                KeyCode::Enter
            },
        );
        assert!(app.tools_view.declared.install_preview.is_none());
        if edit {
            assert!(app.tools_view.declaration_open);
            assert_eq!(app.tools_view.declaration.fields[0], "a");
        } else {
            assert!(app.tools_view.declared_details);
        }
        assert_eq!(f.calls(), "Q");
    }
}
#[test]
fn review_help_dry_run_and_shrink_keep_the_real_approval_boundary() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    let Some(Task::ToolsCheckPrepare(mut inputs)) = key(&mut app, KeyCode::Char('C')) else {
        panic!("Check")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    run(&f, &mut app, Task::ToolsCheckPrepare(inputs));
    key(&mut app, KeyCode::F(1));
    assert!(key(&mut app, KeyCode::Char('y')).is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared.check_preview.is_some() && f.calls().is_empty());
    app.dry_run = true;
    assert!(key(&mut app, KeyCode::Char('y')).is_none());
    app.dry_run = false;
    app.resize(47, 13);
    assert!(app.tools_view.declared.check_preview.is_none());
    app.resize(80, 24);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.tools_view.declared_details && f.calls().is_empty());
    check(&f, &mut app);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    run(&f, &mut app, task);
    key(&mut app, KeyCode::F(1));
    assert!(key(&mut app, KeyCode::Char('y')).is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.declared.install_preview.is_some());
    app.resize(47, 13);
    app.resize(80, 24);
    assert!(app.tools_view.declared.install_preview.is_none() && f.calls() == "Q");
}
#[test]
fn refresh_invalidates_changed_declarations_and_late_reviews_never_reopen_after_navigation() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    check(&f, &mut app);
    let path = f.repo.join("dort.toml");
    std::fs::write(
        &path,
        format!("{}\n# changed\n", std::fs::read_to_string(&path).unwrap()),
    )
    .unwrap();
    run(&f, &mut app, Task::Refresh);
    assert!(app.tools_view.declared.checked.is_none());
    assert_eq!(f.calls(), "Q");
    let Some(Task::ToolsCheckPrepare(mut inputs)) = key(&mut app, KeyCode::Char('C')) else {
        panic!("Check")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    let output = actions::perform_scoped(
        Task::ToolsCheckPrepare(inputs),
        &f.host,
        Some(f.repo.clone()),
        &[],
        |_| Ok(()),
    )
    .unwrap();
    app.activate(Action::Destination(Tab::Git));
    app.complete(Ok(output));
    assert!(app.tools_view.declared.check_preview.is_none() && f.calls() == "Q");
}
#[test]
fn combined_failure_keeps_prior_results_and_does_not_replay_or_start_later_items() {
    let f = Fixture::new();
    std::fs::write(f.host.home.join("fail-a"), "fixture").unwrap();
    let mut app = app(&f, false);
    check(&f, &mut app);
    let task = key(&mut app, KeyCode::Char('i')).unwrap();
    run(&f, &mut app, task);
    let task = super::tests::approve_ready(&mut app, 80, 24).unwrap();
    run(&f, &mut app, task);
    let result = app.tools_view.declared.installed.as_ref().unwrap();
    assert!(!result.succeeded());
    assert_eq!(result.not_started, vec!["b"]);
    let calls = f.calls();
    assert!(calls.contains('A') && !calls.contains('B'));
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    assert_eq!(f.calls(), calls);
}
#[test]
fn declared_workflow_gallery_export_probe() {
    let Ok(directory) = std::env::var("DORT_TUI_GALLERY_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(directory);
    assert!(path.is_absolute() && path.canonicalize().unwrap() == path);
    let allowed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../temp")
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(&allowed) && path != allowed);
    let f = Fixture::new();
    let mut app = app(&f, false);
    let mut output = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Tools workflow</title><style>body{background:#10141c;color:#e6edf3;padding:24px;font:14px monospace}pre{line-height:1.35;overflow:auto;border:1px solid #46556b;padding:8px}</style><h1>Declaration-led Tools</h1><p>Actual cells, disposable fixtures and fake providers. Human visual review remains required.</p>",
    );
    for stage in [
        "Declared tools",
        "Selected tool",
        "Check review",
        "Missing tools",
        "Install review",
        "Install results",
        "Installation history",
        "Tool settings",
        "Removal review",
    ] {
        match stage {
            "Selected tool" => {
                key(&mut app, KeyCode::Enter);
            }
            "Check review" => {
                let Some(Task::ToolsCheckPrepare(mut inputs)) = key(&mut app, KeyCode::Char('C'))
                else {
                    panic!("Check")
                };
                inputs.search_dirs = vec![f.bin.clone()];
                run(&f, &mut app, Task::ToolsCheckPrepare(inputs));
            }
            "Missing tools" | "Install results" => {
                let task = super::tests::approve_ready(&mut app, 80, 24).unwrap();
                run(&f, &mut app, task);
            }
            "Install review" => {
                let task = key(&mut app, KeyCode::Char('i')).unwrap();
                run(&f, &mut app, task);
            }
            "Installation history" => {
                key(&mut app, KeyCode::Enter);
                let task = key(&mut app, KeyCode::Char('3')).unwrap();
                run(&f, &mut app, task);
            }
            "Tool settings" => {
                key(&mut app, KeyCode::Esc);
                super::tools_tests::settings(&f, &mut app);
            }
            "Removal review" => {
                key(&mut app, KeyCode::Esc);
                let task = key(&mut app, KeyCode::Char('d')).unwrap();
                run(&f, &mut app, task);
            }
            _ => {}
        }
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| super::views::draw(frame, &app))
                .unwrap();
            output.push_str(&format!("<h2>{stage} — {w}×{h}</h2>"));
            output.push_str(&super::design_system_tests::html(
                terminal.backend().buffer(),
            ));
        }
    }
    let discovered = Fixture::new();
    discovered.program(
        "brew",
        "printf '{\"formulae\":[{\"name\":\"observed\",\"full_name\":\"observed\",\"tap\":\"homebrew/core\",\"installed\":[{\"version\":\"1.2.3\"}]}]}'",
    );
    let mut discovery = super::tools_tests::app(&discovered, false);
    let Some(Task::ToolsInstalledPrepare(mut inputs)) = discovery.activate(Action::ToolsInstalled)
    else {
        panic!("Installed packages")
    };
    inputs.search_dirs = vec![discovered.bin.clone()];
    run(
        &discovered,
        &mut discovery,
        Task::ToolsInstalledPrepare(inputs),
    );
    for stage in [
        "Inspection review",
        "Installed packages",
        "Selected package",
        "Prefilled declaration",
        "Seeded declaration review",
    ] {
        match stage {
            "Installed packages" => {
                let task = super::tests::approve_ready(&mut discovery, 80, 24).unwrap();
                run(&discovered, &mut discovery, task);
            }
            "Selected package" => {
                key(&mut discovery, KeyCode::Enter);
            }
            "Prefilled declaration" => {
                key(&mut discovery, KeyCode::Char('a'));
            }
            "Seeded declaration review" => {
                discovery.tools_view.declaration.fields[0] = "observed".into();
                discovery.tools_view.declaration.fields[1] = "observed".into();
                let task = key(&mut discovery, KeyCode::Enter).unwrap();
                run(&discovered, &mut discovery, task);
            }
            _ => {}
        }
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| super::views::draw(frame, &discovery))
                .unwrap();
            output.push_str(&format!("<h2>{stage} — {w}×{h}</h2>"));
            output.push_str(&super::design_system_tests::html(
                terminal.backend().buffer(),
            ));
        }
    }
    let mut blocked = super::app::App::new(false);
    let error = dort_core::Error::RecoveryRequired(Box::new(dort_core::journal::WriteBlocker {
        transaction: dort_core::journal::Diagnosis {
            repository_id: "b".repeat(64),
            transaction_id: "c".repeat(64),
            phase: Some(dort_core::journal::Phase::Preparing),
            journal_schema: Some(6),
            valid_checkpoints: 1,
            problem: None,
        },
        repository_root: Some("/synthetic/earlier-repository".into()),
    }));
    blocked.complete(super::actions::present(Err(error)));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| super::views::draw(frame, &blocked))
            .unwrap();
        output.push_str(&format!("<h2>Blocked earlier operation — {w}×{h}</h2>"));
        output.push_str(&super::design_system_tests::html(
            terminal.backend().buffer(),
        ));
    }
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join("declared-tools.html"))
        .unwrap()
        .write_all(output.as_bytes())
        .unwrap();
}
