use super::{
    actions::{self, Output, Task},
    app::{App, Tab},
    discover_content,
    shell::{Action, Overlay},
    tests::render,
};
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};
use std::fs;

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap().join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        fs::write(
            host.home.join(".a"),
            "synthetic-preview-A\n\u{1b}[31mescaped\n",
        )
        .unwrap();
        fs::write(host.home.join(".b"), "synthetic-preview-B\n").unwrap();
        Self { _temp: temp, host }
    }
    fn app(&self) -> App {
        let mut app = App::new(false);
        app.tab = Tab::Files;
        app.discovering = true;
        app.complete(Ok(actions::perform(
            Task::Discover,
            &self.host,
            None,
            |_| Ok(()),
        )
        .unwrap()));
        app.resize(140, 40);
        app
    }
    fn finish(&self, task: Task) -> Output {
        actions::perform(task, &self.host, None, |_| Ok(())).unwrap()
    }
}
fn key(app: &mut App, code: KeyCode) -> Option<Task> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 140, 40)
}
#[test]
fn contextual_content_help_withholds_payload_rejects_late_read_and_restores_focus() {
    let f = Fixture::new();
    let mut app = f.app();
    let read = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(read)));
    key(&mut app, KeyCode::Tab);
    assert!(app.content.focused);
    app.content.scroll = 2;
    let selected = app.candidate_selected;
    key(&mut app, KeyCode::F(1));
    assert!(matches!(
        app.help.as_ref().unwrap().topic,
        super::help::Topic::Content
    ));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        super::files::FORMATTED_ROWS.with(|r| r.set(0));
        let screen = render(&app, w, h);
        assert!(!screen.contains("synthetic-preview-A") && !screen.contains("Files / Discover"));
        assert_eq!(super::files::FORMATTED_ROWS.with(|r| r.get()), 0);
    }
    assert!(!app.content.text().contains("synthetic-preview-A"));
    assert!(discover_content::next(&mut app).is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.content.focused);
    assert_eq!(app.content.scroll, 2);
    assert_eq!(app.candidate_selected, selected);
    let late = discover_content::next(&mut app).unwrap();
    key(&mut app, KeyCode::F(1));
    app.complete(Ok(f.finish(late)));
    assert!(!app.content.text().contains("synthetic-preview-A"));
    key(&mut app, KeyCode::Esc);
    let fresh = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(fresh)));
    assert!(app.content.text().contains("synthetic-preview-A"));
    assert!(app.content.focused);
    app.content.scroll = 2;
    key(&mut app, KeyCode::F(10));
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        assert!(!render(&app, w, h).contains("synthetic-preview-A"));
    }
    assert!(!app.content.available() && discover_content::next(&mut app).is_none());
    key(&mut app, KeyCode::Esc);
    assert!(app.content.focused && app.content.scroll == 2);
    let late = discover_content::next(&mut app).unwrap();
    key(&mut app, KeyCode::F(10));
    app.complete(Ok(f.finish(late)));
    assert!(!app.content.text().contains("synthetic-preview-A"));
    key(&mut app, KeyCode::Esc);
    let fresh = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(fresh)));
    assert!(app.content.text().contains("synthetic-preview-A"));
}
#[test]
fn content_focus_uses_wrapped_viewport_without_selection_changes_or_rereads() {
    let f = Fixture::new();
    fs::write(
        f.host.home.join(".a"),
        "ordinary-wrapped-settings ".repeat(1500),
    )
    .unwrap();
    let mut app = f.app();
    let read = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(read)));
    render(&app, 140, 40);
    assert!(!app.content.focused);
    key(&mut app, KeyCode::Tab);
    assert!(app.content.focused);
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal
        .draw(|frame| super::views::draw(frame, &app))
        .unwrap();
    let (x, y) = (0..40)
        .flat_map(|y| (0..140).map(move |x| (x, y)))
        .find(|(x, y)| {
            matches!(
                app.hits.target(*x, *y),
                Some(super::input::Target::DiscoverFocus(true))
            )
        })
        .unwrap();
    assert_eq!(
        terminal.backend().buffer()[(x, y)].fg,
        ratatui::style::Color::Cyan
    );
    assert!(discover_content::next(&mut app).is_none());
    let selected = app.candidate_selected;
    let page = app.content.page.get();
    let end = app.content.end.get();
    assert!(
        page > 1 && end > page * 4,
        "one raw line must wrap into many rows"
    );
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.content.scroll, 1);
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(app.content.scroll, 0);
    for (code, expected) in [('f', page), ('d', page + page / 2), ('u', page), ('b', 0)] {
        assert!(
            app.key(
                KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL),
                140,
                40
            )
            .is_none()
        );
        assert_eq!(app.content.scroll, expected);
    }
    key(&mut app, KeyCode::Char('G'));
    assert_eq!(app.content.scroll, end);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.content.scroll, end);
    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.content.scroll, 0);
    assert_eq!(app.candidate_selected, selected);
    assert!(app.inventory.candidate_marks.is_empty());
    key(&mut app, KeyCode::BackTab);
    assert!(!app.content.focused);
    assert!(discover_content::next(&mut app).is_none());
    assert!(app.content.text().starts_with("ordinary-wrapped"));
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Esc);
    assert!(!app.content.focused && !app.inventory.pending_g);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.candidate_selected, selected + 1);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Enter);
    assert!(!app.content.focused); // Wide inspection leaves the visible list active.
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.candidate_selected, selected + 1);
}

#[test]
fn content_focus_mouse_compact_text_modal_and_floor_guards() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let f = Fixture::new();
    fs::write(f.host.home.join(".a"), "ordinary-settings ".repeat(1200)).unwrap();
    let mut app = f.app();
    let read = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(read)));
    render(&app, 140, 40);
    let click = |app: &mut App, content: bool| {
        let (x, y) = (0..40).flat_map(|y| (0..140).map(move |x| (x,y))).find(|(x,y)| matches!(app.hits.target(*x,*y), Some(super::input::Target::DiscoverFocus(focus)) if focus == content)).unwrap();
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            140,
            40,
        );
    };
    click(&mut app, true);
    assert!(app.content.focused);
    render(&app, 140, 40);
    click(&mut app, false);
    assert!(!app.content.focused);
    key(&mut app, KeyCode::Tab);
    app.resize(48, 14);
    assert!(app.content.focused);
    let screen = render(&app, 48, 14);
    assert!(screen.contains("Content ·"));
    assert!(!screen.contains("Space mark"));
    app.key(
        KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        48,
        14,
    );
    assert_eq!(app.content.scroll, app.content.end.get());
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 48, 14);
    assert!(!app.content.focused);
    assert!(render(&app, 48, 14).contains("Space mark"));
    key(&mut app, KeyCode::Char('/'));
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.inventory.candidates.query, "j");
    assert!(!app.content.focused);
    key(&mut app, KeyCode::Esc);
    app.inventory.candidates.query.clear();
    app.resize(140, 40);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('?'));
    assert!(!app.content.focused && !app.content.text().starts_with("ordinary-settings"));
    key(&mut app, KeyCode::Tab);
    assert!(!app.content.focused);
    key(&mut app, KeyCode::Esc);
    app.resize(140, 40);
    key(&mut app, KeyCode::Tab);
    app.resize(47, 13);
    assert!(!app.content.focused);
    assert!(discover_content::next(&mut app).is_none());
    app.resize(140, 40);
    key(&mut app, KeyCode::Tab);
    app.dry_run = true;
    app.resize(140, 40);
    assert!(!app.content.focused);
    assert!(discover_content::next(&mut app).is_none());
    super::tests::destination(&mut app, super::app::Tab::Git, 80, 24);
    key(&mut app, KeyCode::Tab);
    assert!(!app.content.focused && app.tab == Tab::Git);
}

#[test]
fn selected_content_is_passive_escaped_ephemeral_and_late_results_are_dropped() {
    let f = Fixture::new();
    let mut app = f.app();
    let read_a = discover_content::next(&mut app).unwrap();
    assert!(discover_content::next(&mut app).is_none());
    assert!(!app.busy && app.activity.is_none());
    key(&mut app, KeyCode::Down);
    assert_eq!(app.candidate_selected, 1);
    app.complete(Ok(f.finish(read_a)));
    assert!(!app.content.text().contains("synthetic-preview-A"));
    let read_b = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(read_b)));
    assert!(app.content.text().contains("synthetic-preview-B"));
    assert!(app.result.is_none() && app.receipt.is_none() && app.error.is_none());
    let screen = render(&app, 140, 40);
    assert!(screen.contains("synthetic-preview-B"));
    let content_row = screen.lines().position(|l| l.contains("Content")).unwrap();
    let details_row = screen
        .lines()
        .position(|l| l.contains("Metadata") && l.contains('┌'))
        .unwrap();
    assert!(details_row > content_row + 10);
    key(&mut app, KeyCode::Up);
    let read_a = discover_content::next(&mut app).unwrap();
    app.complete(Ok(f.finish(read_a)));
    let screen = render(&app, 140, 40);
    assert!(screen.contains("\\u{1b}[31mescaped"));
    key(&mut app, KeyCode::Char('?'));
    assert!(!app.content.text().contains("synthetic-preview-A"));
    assert!(discover_content::next(&mut app).is_none());
    key(&mut app, KeyCode::Esc);
    let read = discover_content::next(&mut app).unwrap();
    app.resize(47, 13);
    app.complete(Ok(f.finish(read)));
    assert!(!app.content.text().contains("synthetic-preview-A"));
    app.resize(140, 40);
    let read = discover_content::next(&mut app).unwrap();
    super::tests::destination(&mut app, super::app::Tab::Git, 80, 24);
    app.complete(Ok(f.finish(read)));
    assert!(!app.content.text().contains("synthetic-preview-A"));
}
#[test]
fn explicit_next_task_waits_for_the_same_owned_passive_worker() {
    let f = Fixture::new();
    let mut app = f.app();
    let read = discover_content::next(&mut app).unwrap();
    let mut worker =
        Some(super::events::Worker::start(read, f.host.clone(), None, vec![]).unwrap());
    let mut queued = None;
    super::dispatch(Task::History, &f.host, &mut app, &mut worker, &mut queued).unwrap();
    assert!(app.busy && worker.is_some());
    assert!(matches!(queued, Some(Task::History)));
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    worker = None;
    app.complete(Ok(output));
    assert!(app.busy); // Passive completion cannot consume the explicit task's state.
    super::launch(queued.take().unwrap(), &f.host, &mut app, &mut worker).unwrap();
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    worker = None;
    app.complete(Ok(output));
    assert!(!app.busy && worker.is_none());
}

#[test]
fn no_disclosure_in_dry_run_and_changed_or_sensitive_content_is_withheld() {
    let f = Fixture::new();
    let mut app = f.app();
    app.dry_run = true;
    app.resize(140, 40);
    assert!(discover_content::next(&mut app).is_none());
    app.dry_run = false;
    app.resize(140, 40);
    let read = discover_content::next(&mut app).unwrap();
    fs::write(f.host.home.join(".a"), "password=synthetic-private-value").unwrap();
    app.complete(Ok(f.finish(read)));
    assert!(!app.content.text().contains("synthetic-private-value"));
    assert!(app.error.is_none());
    assert!(!app.status.contains("synthetic-private-value"));
}
#[test]
fn discover_page_is_not_cluttered_and_ineligible_marking_is_prominent() {
    let f = Fixture::new();
    let mut app = f.app();
    assert!(!app.page_actions().contains(&Action::Inspect));
    assert!(!app.page_actions().contains(&Action::Adopt));
    // A single eligible selection needs no redundant inspection activation.
    key(&mut app, KeyCode::Char('e'));
    assert!(matches!(app.shell.overlay, Some(Overlay::Form(_))));
    key(&mut app, KeyCode::Esc);
    std::os::unix::fs::symlink(f.host.home.join(".a"), f.host.home.join(".c-link")).unwrap();
    app.complete(Ok(f.finish(Task::Discover)));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' '));
    assert!(app.inventory.candidate_marks.is_empty());
    assert!(render(&app, 140, 40).contains("Cannot mark"));
}
#[test]
fn files_viewport_stays_put_on_up_and_mouse_uses_actual_rendered_offset() {
    let f = Fixture::new();
    for i in 0..55 {
        fs::write(f.host.home.join(format!(".row-{i:02}")), "ordinary").unwrap();
    }
    let mut app = f.app();
    let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
    for _ in 0..30 {
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 140, 24);
        terminal
            .draw(|frame| super::views::draw(frame, &app))
            .unwrap();
    }
    let before = app.inventory.candidate_offset.get();
    assert!(before > 0);
    app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), 140, 24);
    terminal
        .draw(|frame| super::views::draw(frame, &app))
        .unwrap();
    assert_eq!(before, app.inventory.candidate_offset.get());
    let old = app.candidate_selected;
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 140, 24);
    terminal
        .draw(|frame| super::views::draw(frame, &app))
        .unwrap();
    assert_eq!(app.candidate_selected, old + 1);
    let (x, y) = (0..24).flat_map(|y| (0..60).map(move |x| (x, y))).find(|(x, y)| {
        matches!(app.hits.target(*x, *y), Some(super::input::Target::Row(index)) if index == before)
    }).unwrap();
    app.mouse(
        ratatui::crossterm::event::MouseEvent {
            kind: ratatui::crossterm::event::MouseEventKind::Down(
                ratatui::crossterm::event::MouseButton::Left,
            ),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        140,
        24,
    );
    assert_eq!(app.candidate_selected, before);
    // Filtering and resize clamp retained offsets, rather than leaving blank rows.
    app.inventory.candidates.query = ".row-01".into();
    app.candidate_selected = app
        .discovery
        .as_ref()
        .unwrap()
        .report
        .candidates
        .iter()
        .position(|c| c.entry.path.ends_with(".row-01"))
        .unwrap();
    terminal
        .draw(|frame| super::views::draw(frame, &app))
        .unwrap();
    assert_eq!(app.inventory.candidate_offset.get(), 0);
}
