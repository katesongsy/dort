//! Help is a non-authorizing, exclusive layer over retained typed surfaces.
use super::{
    actions::{Data, Output, Task},
    app::{App, Tab},
    batch_ui::Editor,
    configuration::{Draft, Kind},
    shell::{Form, Overlay},
    tests::render,
};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 80, 24)
}
fn inert(app: &mut App) {
    for code in [
        KeyCode::Enter,
        KeyCode::Char('y'),
        KeyCode::Char('e'),
        KeyCode::Char('s'),
        KeyCode::Char('a'),
        KeyCode::Char('1'),
    ] {
        assert!(key(app, code).is_none());
        assert!(app.help.is_some());
    }
}
#[test]
fn every_view_has_its_own_help_and_preserves_navigation_state() {
    for (tab, title, first_move) in [
        (Tab::Overview, "Overview", "Start with a repository"),
        (Tab::Files, "Managed files", "A mapping tells Dort"),
        (Tab::Recovery, "Recovery", "Select an operation record"),
        (Tab::Checks, "Manifest checks", "Choose Check repository"),
        (Tab::Git, "Git status", "See which paths changed"),
        (
            Tab::Tools,
            "Declared tools",
            "Review tools your repository lists",
        ),
    ] {
        let mut app = App::new(false);
        app.tab = tab;
        app.details_scroll = 7;
        key(&mut app, KeyCode::F(1));
        for (w, h) in [(48, 14), (80, 24), (140, 40)] {
            let s = render(&app, w, h);
            assert!(s.contains(title) && s.contains(first_move), "{s}");
            assert!(s.contains("Esc Close Help") && !s.contains("[p Scope]"));
            assert!(!s.contains("Related tasks") && !s.contains("Keys in this context"));
        }
        inert(&mut app);
        key(&mut app, KeyCode::F(1));
        assert!(app.help.is_none() && app.tab == tab && app.details_scroll == 7);
        key(&mut app, KeyCode::Char('?'));
        assert!(app.help.is_some());
        key(&mut app, KeyCode::Esc);
    }
}
#[test]
fn forms_actions_and_search_keep_exact_drafts_scroll_and_focus() {
    for (form, guidance) in [
        (Form::Choose, "root folder that contains dort.toml"),
        (Form::Init, "give an unused path"),
        (Form::Adopt, "repository file available at the old path"),
        (Form::KeepId, "leaves a regular copy"),
        (Form::RemoveId, "repository source stays"),
        (Form::Check, "Base is always active"),
        (Form::Clone, "HTTPS Git URL or absolute local Git path"),
    ] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(form));
        super::tests::focus_form(&mut app);
        for c in "private-draft-q?/".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        assert!(app.help.is_none() && !app.quit);
        let draft = app.shell.draft(form).to_owned();
        let target = app.shell.adoption.target.clone();
        app.shell.scroll = 3;
        key(&mut app, KeyCode::F(1));
        let screen = render(&app, 140, 40);
        assert!(
            screen.contains(guidance) && !screen.contains("private-draft"),
            "{screen}"
        );
        inert(&mut app);
        key(&mut app, KeyCode::Esc);
        assert!(matches!(app.shell.overlay, Some(Overlay::Form(f)) if f == form));
        assert_eq!(app.shell.draft(form), draft);
        assert_eq!(app.shell.adoption.target, target);
        assert_eq!(app.shell.scroll, 3);
    }
    let mut app = App::new(false);
    for tab in [Tab::Files, Tab::Recovery, Tab::Git] {
        app.tab = tab;
        key(&mut app, KeyCode::Char('/'));
        for c in "q?/".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::F(1));
        key(&mut app, KeyCode::Esc);
        assert!(app.text_entry() && !app.quit);
        let query = match tab {
            Tab::Files => &app.inventory.managed.query,
            Tab::Recovery => &app.history_search.query,
            _ => &app.git_view.search.query,
        };
        assert_eq!(query, "q?/");
    }
}
#[test]
fn configuration_help_explains_what_each_form_changes() {
    for (kind, title, guidance) in [
        (
            Kind::File,
            "Add file mapping",
            "Saving does not create the link",
        ),
        (
            Kind::EditFile,
            "Edit file mapping",
            "The Mapping ID stays fixed",
        ),
        (Kind::CreateProfile, "Create profile", "optional profile"),
        (
            Kind::EditProfileDefaults,
            "Edit profile defaults",
            "Local choice",
        ),
        (Kind::RenameProfile, "Rename profile", "instead of work"),
        (Kind::Startup, "Set startup repository", "future launches"),
        (Kind::Forget, "Forget repository", "keeps the repository"),
        (
            Kind::Reconnect,
            "Reconnect moved repository",
            "Previous registered path",
        ),
        (
            Kind::Repair,
            "Open a manifest repair draft",
            "Open repair draft loads editable lines",
        ),
    ] {
        let mut app = App::new(false);
        app.shell.overlay = Some(Overlay::Form(Form::Configure));
        app.shell.configuration = Some(Draft::new(
            kind,
            None,
            Some(std::path::Path::new("/private-path")),
        ));
        key(&mut app, KeyCode::F(1));
        let screen = render(&app, 140, 40);
        assert!(
            screen.contains(title) && screen.contains(guidance),
            "{screen}"
        );
        assert!(!screen.contains("/private-path"));
    }
}

#[test]
fn tool_help_follows_the_form_picker_and_package_details() {
    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.tools_view.declare();
    super::tests::focus_form(&mut app);
    key(&mut app, KeyCode::F(1));
    let screen = render(&app, 80, 24);
    assert!(screen.contains("Add tool declaration") && screen.contains("Tool ID"));
    key(&mut app, KeyCode::F(1));
    for _ in 0..3 {
        key(&mut app, KeyCode::Tab);
    }
    key(&mut app, KeyCode::Char('?'));
    assert!(render(&app, 80, 24).contains("Add tool declaration"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::F(1));
    assert!(render(&app, 80, 24).contains("Choose a package manager"));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.tools_view.declaration.fields[4], "brew");

    let mut app = App::new(false);
    app.tab = Tab::Tools;
    app.tools_view.installed_open = true;
    for (details, title) in [(false, "Installed packages"), (true, "Package details")] {
        app.tools_view.installed.details = details;
        key(&mut app, KeyCode::F(1));
        let screen = render(&app, 80, 24);
        assert!(screen.contains(title), "{screen}");
        key(&mut app, KeyCode::Esc);
    }
}

#[test]
fn compact_help_pages_keep_context_and_stop_at_the_end() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Configure));
    app.shell.configuration = Some(Draft::new(Kind::File, None, None));
    let key = |app: &mut App, code| app.key(KeyEvent::new(code, KeyModifiers::NONE), 48, 14);
    let screen = |app: &App| render(app, 48, 14);
    key(&mut app, KeyCode::F(1));
    let first = screen(&app);
    assert!(first.contains("Repository source") && !first.contains("Platform sources"));
    key(&mut app, KeyCode::PageDown);
    let second = screen(&app);
    assert!(second.contains("Home destination") && !second.contains("Declare a link"));
    key(&mut app, KeyCode::PageUp);
    assert_eq!(screen(&app), first);
    key(&mut app, KeyCode::End);
    let last = screen(&app);
    assert!(last.contains("What happens next") && last.contains("Esc Close Help"));
    for _ in 0..10 {
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::PageDown);
    }
    assert_eq!(screen(&app), last);
    key(&mut app, KeyCode::Home);
    assert_eq!(screen(&app), first);
}

#[test]
fn batch_item_help_does_not_edit_submit_or_render_private_drafts() {
    let mut app = App::new(false);
    app.tab = Tab::Files;
    let paths = [std::path::PathBuf::from(
        "/synthetic/home/.config/private-draft",
    )]
    .into_iter()
    .collect();
    app.batch_editor = Some(Editor::new(&paths, std::path::Path::new("/synthetic/home")));
    key(&mut app, KeyCode::Char('?')); // Review all is not text
    assert!(render(&app, 80, 24).contains("Review all"));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('e'));
    super::tests::focus_form(&mut app);
    key(&mut app, KeyCode::Char('?')); // target field is text
    let e = app.batch_editor.as_ref().unwrap();
    let (target, focused) = (e.drafts[0].target.clone(), e.focused);
    assert!(target.ends_with('?') && app.help.is_none());
    key(&mut app, KeyCode::F(1));
    let screen = render(&app, 140, 40);
    assert!(screen.contains("Done editing") && !screen.contains("private-draft"));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    let e = app.batch_editor.as_ref().unwrap();
    assert!(e.open && e.item_form);
    assert_eq!(e.drafts[0].target, target);
    assert_eq!(e.focused, focused);
}
#[test]
fn help_task_error_completion_and_picker_return_without_scope_changes() {
    let mut app = App::new(false);
    app.shell.overlay = Some(Overlay::Form(Form::Init));
    super::tests::focus_form(&mut app);
    app.shell.init_draft = "private-draft".into();
    app.start(true);
    key(&mut app, KeyCode::Char('?')); // background form is not the active text surface
    assert!(render(&app, 80, 24).contains("Work in progress"));
    inert(&mut app);
    assert!(app.busy && app.writing);
    app.complete(Ok(Output::Data(Data::default())));
    assert!(app.help.is_some() && !app.busy);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.shell.init_draft, "private-draft");
    app.error = Some("private-cause".into());
    key(&mut app, KeyCode::F(1));
    let screen = render(&app, 80, 24);
    assert!(screen.contains("Retry read") && !screen.contains("private-cause"));
    inert(&mut app);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.error.as_deref(), Some("private-cause"));
    app.error = None;
    app.show_result = true;
    key(&mut app, KeyCode::F(1));
    assert!(render(&app, 80, 24).contains("Check the outcome"));
    key(&mut app, KeyCode::Esc);
    assert!(app.show_result);
    app.show_result = false;
    app.shell.overlay = Some(Overlay::Ignored {
        paths: vec!["private-path".into()],
        selected: 0,
    });
    key(&mut app, KeyCode::F(1));
    inert(&mut app);
    let s = render(&app, 80, 24);
    assert!(s.contains("Unignore") && !s.contains("private-path"));
    // Only the explicit foreground Close control dismisses Help.
    let (column, row) = (0..24)
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .find(|&(x, y)| matches!(app.hits.target(x, y), Some(super::input::Target::Back)))
        .unwrap();
    assert!(
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE
            },
            80,
            24
        )
        .is_none()
    );
    assert!(app.help.is_none());
    assert!(matches!(
        app.shell.overlay,
        Some(Overlay::Ignored { selected: 0, .. })
    ));
    app.start(true);
    key(&mut app, KeyCode::F(1));
    app.key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        80,
        24,
    );
    assert!(app.exit_requested && !app.quit && app.busy);
}
