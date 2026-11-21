use super::{
    actions::{self, Task},
    shell::Action,
    tests::render,
    tools_tests::{app, key, run},
};
use crate::services::declared_tools_tests::Fixture;
use ratatui::crossterm::event::KeyCode;
fn review(f: &Fixture, app: &mut super::app::App) {
    let task = app.activate(Action::ToolsInstalled).unwrap();
    let Task::ToolsInstalledPrepare(mut input) = task else {
        panic!("process-free preparation");
    };
    input.search_dirs = vec![f.bin.clone()];
    run(f, app, Task::ToolsInstalledPrepare(input));
    assert!(app.tools_view.installed.preview.is_some());
}
fn inspect(f: &Fixture, app: &mut super::app::App) {
    review(f, app);
    let task = key(app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::ToolsInstalledInspect(_)));
    run(f, app, task);
}
#[test]
fn npm_from_path_needs_no_repository_and_shows_actual_global_scope_without_install_authority() {
    use super::app::{App, Tab};
    use dort_core::tools::inspection::Query;
    let f = Fixture::new();
    std::fs::remove_file(f.host.local_config_path()).unwrap();
    std::fs::remove_file(f.bin.join("brew")).unwrap();
    std::fs::write(f.repo.join("dort.toml"), "PRIVATE_INVALID_MANIFEST").unwrap();
    f.program("npm", "exec node \"$@\"");
    f.program(
        "node",
        r#"if [ "$1" = prefix ]; then
printf P >> "$HOME/calls"
printf '%s\n' "$HOME/custom npm globals"
else
printf L >> "$HOME/calls"
printf '{"dependencies":{"formatter":{"version":"1.2.3"}}}'
fi"#,
    );
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    let Task::ToolsInstalledPrepare(mut inputs) = app.activate(Action::ToolsInstalled).unwrap()
    else {
        panic!("Installed packages")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    let output = actions::perform_scoped(
        Task::ToolsInstalledPrepare(inputs),
        &f.host,
        None,
        &[],
        |_| Ok(()),
    )
    .unwrap();
    app.complete(Ok(output));
    let review = app.tools_view.installed.preview.as_ref().unwrap().review();
    assert_eq!(review.inspections.len(), 1);
    assert!(review.inspections[0].prefix_probe.is_some());
    assert!(review.inspections[0].provider.npm_binding.is_none());
    assert!(f.calls().is_empty());
    let text = render(&app, 140, 40);
    assert!(
        text.contains("prefix --global")
            && !text.contains("declaring one Node")
            && !text.contains("PRIVATE_INVALID")
    );
    let task = key(&mut app, KeyCode::Enter).unwrap();
    let output = actions::perform_scoped(task, &f.host, None, &[], |_| Ok(())).unwrap();
    app.complete(Ok(output));
    assert_eq!(f.calls(), "PL");
    let current = app.tools_view.installed.current.as_ref().unwrap();
    let scope = &current.reports[0].provider.description;
    assert_eq!(scope.query, Query::NpmGlobals);
    assert!(scope.npm_binding.is_none());
    assert_eq!(
        scope.npm_global_prefix,
        Some(f.host.home.join("custom npm globals"))
    );
    key(&mut app, KeyCode::Enter);
    assert!(render(&app, 140, 40).contains("Global prefix"));
    f.program(
        "node",
        "printf P >> \"$HOME/calls\"; printf PRIVATE_PREFIX; exit 9",
    );
    let Task::ToolsInstalledPrepare(mut inputs) = key(&mut app, KeyCode::Char('R')).unwrap() else {
        panic!("fresh inspection")
    };
    inputs.search_dirs = vec![f.bin.clone()];
    let output = actions::perform_scoped(
        Task::ToolsInstalledPrepare(inputs),
        &f.host,
        None,
        &[],
        |_| Ok(()),
    )
    .unwrap();
    app.complete(Ok(output));
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(
        app.tools_view.installed.current.is_none(),
        "Accepting refresh discards old results before querying"
    );
    let output = actions::perform_scoped(task, &f.host, None, &[], |_| Ok(())).unwrap();
    app.complete(Ok(output));
    assert!(app.tools_view.installed.rows().is_empty());
    let text = render(&app, 140, 40);
    assert!(
        text.contains("unavailable")
            && !text.contains("formatter")
            && !text.contains("PRIVATE_PREFIX")
    );
    assert_eq!(f.calls(), "PLP");
}
#[test]
fn current_inspection_details_and_prefilled_add_have_separate_approvals_and_no_replay() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    let original = std::fs::read(f.repo.join("dort.toml")).unwrap();
    review(&f, &mut app);
    assert!(f.calls().is_empty());
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let text = render(&app, w, h);
        assert!(
            text.contains("Check packages") && text.contains("change caches"),
            "{text}"
        );
        assert!(text.lines().last().unwrap().contains("F1 Help"), "{text}");
    }
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run(&f, &mut app, task);
    assert_eq!(f.calls(), "Q");
    assert_eq!(app.tools_view.installed.rows().len(), 1);
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.installed.details);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    key(&mut app, KeyCode::Char('a'));
    assert!(app.tools_view.declaration_open);
    let draft = &mut app.tools_view.declaration.fields;
    assert!(draft[0].is_empty() && draft[1].is_empty());
    assert_eq!(
        (&draft[4], &draft[5], &draft[6]),
        (&"brew".to_owned(), &"unrelated".to_owned(), &String::new())
    );
    draft[0] = "new-tool".into();
    draft[1] = "new-command".into();
    // Help dismissal preserves the draft and inspection; it grants no authority.
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::Esc);
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run(&f, &mut app, task);
    assert!(app.preview.as_ref().unwrap().text.contains("unrelated"));
    assert_eq!(std::fs::read(f.repo.join("dort.toml")).unwrap(), original);
    assert_eq!(f.calls(), "Q");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    assert!(matches!(task, Task::Execute(_)) && app.confirmation.is_none());
    run(&f, &mut app, task);
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.installed_open && !app.tools_view.declaration_open);
    assert!(
        std::fs::read_to_string(f.repo.join("dort.toml"))
            .unwrap()
            .contains("new-command")
    );
    assert_eq!(f.calls(), "Q");
}
#[test]
fn only_current_results_survive_refresh_and_failed_queries_never_fall_back() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    inspect(&f, &mut app);
    key(&mut app, KeyCode::Esc);
    assert!(app.activate(Action::ToolsInstalled).is_none());
    assert_eq!(f.calls(), "Q");
    f.program(
        "brew",
        "printf Q >> \"$HOME/calls\"\nprintf PRIVATE_PROVIDER_PAYLOAD\nexit 0",
    );
    let Task::ToolsInstalledPrepare(mut input) = key(&mut app, KeyCode::Char('R')).unwrap() else {
        panic!("fresh review");
    };
    input.search_dirs = vec![f.bin.clone()];
    run(&f, &mut app, Task::ToolsInstalledPrepare(input));
    assert_eq!(f.calls(), "Q");
    let task = key(&mut app, KeyCode::Enter).unwrap();
    run(&f, &mut app, task);
    assert!(app.tools_view.installed.rows().is_empty());
    assert!(
        !app.tools_view
            .installed
            .current
            .as_ref()
            .unwrap()
            .available()
    );
    let text = render(&app, 80, 24);
    assert!(!text.contains("unrelated") && !text.contains("PRIVATE_PROVIDER_PAYLOAD"));
    assert_eq!(f.calls(), "QQ");
}
#[test]
fn dry_run_shrink_navigation_and_late_preparation_never_execute_inspection() {
    for dry in [true, false] {
        let f = Fixture::new();
        let mut app = app(&f, dry);
        review(&f, &mut app);
        if dry {
            assert!(key(&mut app, KeyCode::Enter).is_none());
        }
        app.resize(47, 13);
        app.resize(80, 24);
        assert!(app.tools_view.installed.preview.is_none());
        assert!(key(&mut app, KeyCode::Enter).is_none());
        assert!(f.calls().is_empty());
    }
    let f = Fixture::new();
    let mut app = app(&f, false);
    let Task::ToolsInstalledPrepare(mut input) = app.activate(Action::ToolsInstalled).unwrap()
    else {
        panic!("prepare");
    };
    input.search_dirs = vec![f.bin.clone()];
    let task = Task::ToolsInstalledPrepare(input);
    app.activate(Action::Destination(super::app::Tab::Git));
    // Preparation itself is process-free even if it finishes after navigation.
    let output = actions::perform_scoped(task, &f.host, Some(f.repo.clone()), &[], |_| Ok(()));
    app.complete(output.map_err(|e| actions::diagnostic(&e)));
    assert!(app.tools_view.installed.preview.is_none() && f.calls().is_empty());
}
#[test]
fn all_supported_providers_seed_only_known_package_version_and_runtime() {
    use dort_core::{model::Provider, tools::inspection::Query};
    for (provider, query, name, exact) in [
        (Provider::Npm, Query::NpmGlobals, "npm", true),
        (Provider::Brew, Query::BrewFormulae, "brew", false),
    ] {
        let f = super::tools_provider_fixture::fixture(provider);
        std::fs::write(f.host.home.join("installed"), "fixture").unwrap();
        let mut app = app(&f, false);
        inspect(&f, &mut app);
        let rows = app.tools_view.installed.rows();
        let current = app.tools_view.installed.current.as_ref().unwrap();
        app.tools_view.installed.selected = rows
            .iter()
            .position(|row| {
                current
                    .package(*row)
                    .is_some_and(|(scope, p)| scope.query == query && p.package == "demo")
            })
            .unwrap();
        key(&mut app, KeyCode::Char('a'));
        assert!(app.tools_view.declaration_open);
        let fields = &app.tools_view.declaration.fields;
        let i = 4;
        assert_eq!(fields[i], name);
        assert_eq!(fields[i + 1], "demo");
        assert_eq!(!fields[i + 2].is_empty(), exact);
        assert_eq!(
            fields[i + 3],
            if provider == Provider::Npm {
                "runtime"
            } else {
                ""
            }
        );
        assert!(fields[0].is_empty() && fields[1].is_empty());
        assert!(!f.calls().contains('I') && !f.calls().contains('S'));
    }
}
#[test]
fn installed_filter_is_display_only_and_menu_back_preserves_selection() {
    let f = Fixture::new();
    let mut app = app(&f, false);
    inspect(&f, &mut app);
    key(&mut app, KeyCode::Char('/'));
    for c in "no-match".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    assert!(app.tools_view.installed.rows().is_empty());
    assert_eq!(f.calls(), "Q");
    key(&mut app, KeyCode::Char('/'));
    for _ in 0..8 {
        key(&mut app, KeyCode::Backspace);
    }
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.tools_view.installed.rows().len(), 1);
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::Esc);
    assert!(app.tools_view.installed_open);
    assert_eq!(f.calls(), "Q");
}
