//! Large metadata fixtures; no corresponding files are created or read.
use super::{
    actions::{Output, Task},
    app::{App, Tab},
    discovery_tree,
    tests::render,
    tree,
};
use dort_core::{
    context::HostContext,
    discovery::{Candidate, Discovery},
    observation::{Entry, Kind},
    platform::{Architecture, Platform},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap().join("home");
        std::fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        Self { _temp: temp, host }
    }
    fn app(&self, count: usize, nested: bool) -> App {
        let mut app = App::new(false);
        app.tab = Tab::Files;
        app.discovering = true;
        let candidates = (0..count)
            .map(|i| Candidate {
                entry: Entry {
                    path: self.host.home.join(if nested {
                        format!(".group-{}/row-{i:06}", i % 2)
                    } else {
                        format!(".row-{i:06}")
                    }),
                    kind: Kind::Regular,
                    fingerprint: None,
                    parents: vec![],
                    link_target: None,
                    sha256: None,
                    sensitive: false,
                },
                link_state: None,
                supported_for_adoption: true,
            })
            .collect();
        app.complete(Ok(Output::Discovery(crate::services::Discovery {
            home: self.host.home.clone(),
            repository: None,
            report: Discovery {
                candidates,
                warnings: vec![],
                sensitivity_notice: "synthetic metadata",
            },
        })));
        app.resize(140, 40);
        app
    }
    fn finish(&self, app: &mut App) {
        let task = discovery_tree::next(app).expect("passive preparation pending");
        let mut worker = None;
        super::launch(task, &self.host, app, &mut worker).unwrap();
        assert!(!app.busy && app.activity.is_none() && app.retry.is_none());
        let output = worker.as_mut().unwrap().join().unwrap().unwrap();
        app.complete(Ok(output));
    }
}
fn key(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), 140, 40);
}
fn bounded_render(app: &App, width: u16, height: u16) -> String {
    super::files::FORMATTED_ROWS.with(|n| n.set(0));
    let screen = render(app, width, height);
    super::files::FORMATTED_ROWS.with(|n| {
        assert!(
            n.get() <= usize::from(height),
            "formatted {} rows for height {height}",
            n.get()
        )
    });
    screen
}
#[test]
fn large_preparation_is_passive_coalesces_search_and_rejects_late_results() {
    let f = Fixture::new();
    let mut app = f.app(100_000, false);
    let empty = discovery_tree::snapshot(&app);
    assert!(empty.all.is_empty() && discovery_tree::pending(&app));
    assert!(bounded_render(&app, 140, 40).contains("Updating list"));
    assert!(Arc::ptr_eq(&empty, &discovery_tree::snapshot(&app)));
    let task = discovery_tree::next(&app).unwrap();
    assert!(discovery_tree::next(&app).is_none());
    let mut worker = None;
    super::launch(task, &f.host, &mut app, &mut worker).unwrap();
    assert!(!app.busy && app.activity.is_none());
    key(&mut app, KeyCode::Char('?'));
    assert!(app.help.is_some() && app.shell.overlay.is_none());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('/'));
    for ch in ".row-099999".chars() {
        key(&mut app, KeyCode::Char(ch));
    }
    key(&mut app, KeyCode::Enter);
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    app.complete(Ok(output));
    assert!(tree::visible(&app).is_empty());
    assert!(
        app.action_reason(super::shell::Action::AdoptSelected)
            .is_some()
    );
    f.finish(&mut app);
    assert_eq!(&*discovery_tree::snapshot(&app).matching, &[99_999]);
    assert_eq!(app.candidate_selected, 99_999);
    let current = discovery_tree::snapshot(&app);
    key(&mut app, KeyCode::Char(' '));
    assert!(
        app.inventory
            .candidate_marks
            .contains(&f.host.home.join(".row-099999"))
    );
    app.resize(70, 20);
    bounded_render(&app, 70, 20);
    assert!(Arc::ptr_eq(&current, &discovery_tree::snapshot(&app)));
    key(&mut app, KeyCode::Char('q'));
    assert!(app.quit);
    assert!(discovery_tree::next(&app).is_none());
}
#[test]
fn cached_large_navigation_virtualizes_rows_and_preserves_raw_mouse_identity() {
    let f = Fixture::new();
    let mut app = f.app(100_000, false);
    f.finish(&mut app);
    let snapshot = discovery_tree::snapshot(&app);
    assert_eq!(tree::visible(&app).len(), 100_000);
    app.candidate_selected = 90_000;
    bounded_render(&app, 140, 40);
    let offset = app.inventory.candidate_offset.get();
    assert!(offset > 89_900);
    key(&mut app, KeyCode::Up);
    bounded_render(&app, 140, 40);
    assert_eq!(app.inventory.candidate_offset.get(), offset);
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
        bounded_render(&app, 140, 40);
        assert!(Arc::ptr_eq(&snapshot, &discovery_tree::snapshot(&app)));
        assert!(Arc::ptr_eq(&snapshot.all, &tree::visible(&app).all));
    }
    let expected = app.inventory.candidate_offset.get();
    let (x, y) = (0..40).flat_map(|y| (0..60).map(move |x| (x, y))).find(|&(x,y)| matches!(app.hits.target(x,y), Some(super::input::Target::Row(i)) if i == expected)).unwrap();
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
        40,
    );
    assert_eq!(app.candidate_selected, expected);
    assert!(Arc::ptr_eq(&snapshot, &discovery_tree::snapshot(&app)));
    app.resize(48, 14);
    bounded_render(&app, 48, 14);
    assert!(Arc::ptr_eq(&snapshot, &discovery_tree::snapshot(&app)));
}
#[test]
fn local_folds_keep_the_snapshot_and_marks_and_scope_changes_drop_old_preparations() {
    let f = Fixture::new();
    let mut app = f.app(10_000, true);
    f.finish(&mut app);
    assert!(tree::visible(&app).iter().all(|n| n.branch));
    app.inventory.candidate_folded = Arc::new(super::visibility::Folds::all(false));
    tree::select(&mut app, 9999);
    key(&mut app, KeyCode::Char(' '));
    let marked = app.inventory.candidate_marks.clone();
    assert_eq!(discovery_tree::shown_marks(&app), 1);
    let snapshot = discovery_tree::snapshot(&app);
    tree::fold(&mut app, true);
    let group = app.inventory.group.unwrap();
    assert!(Arc::ptr_eq(&snapshot, &discovery_tree::snapshot(&app)));
    assert!(!tree::visible(&app).is_empty());
    assert!(!discovery_tree::pending(&app));
    assert!(discovery_tree::next(&app).is_none());
    assert!(!app.busy);
    assert_eq!(app.file_selection(), group);
    assert!(
        tree::visible(&app)
            .iter()
            .any(|n| n.key == group && n.branch)
    );
    assert!(!tree::visible(&app).iter().any(|n| n.raw == Some(9999)));
    assert_eq!(discovery_tree::shown_marks(&app), 0);
    assert_eq!(app.inventory.candidate_marks, marked);
    tree::fold(&mut app, false);
    assert!(Arc::ptr_eq(&snapshot, &discovery_tree::snapshot(&app)));
    assert!(tree::visible(&app).iter().any(|n| n.raw == Some(9999)));
    assert_eq!(discovery_tree::shown_marks(&app), 1);
    assert_eq!(app.inventory.candidate_marks, marked);
    app.inventory.candidates.query = "row-009999".into();
    let Task::DiscoveryTree(read) = discovery_tree::next(&app).unwrap() else {
        panic!("wrong task")
    };
    // A refreshed scope is a different immutable source even with identical names.
    app.discovery = f.app(600, false).discovery;
    app.complete(Ok(Output::DiscoveryTree(read.perform())));
    assert!(tree::visible(&app).is_empty());
    app.inventory.candidates.query.clear();
    f.finish(&mut app);
    assert_eq!(discovery_tree::snapshot(&app).matching.len(), 600);
}
#[test]
fn quit_keeps_the_passive_metadata_worker_owned_until_join() {
    let f = Fixture::new();
    let mut app = f.app(10_000, false);
    let mut worker = None;
    super::launch(
        discovery_tree::next(&app).unwrap(),
        &f.host,
        &mut app,
        &mut worker,
    )
    .unwrap();
    key(&mut app, KeyCode::Char('q'));
    assert!(app.quit && !app.busy && worker.is_some());
    assert!(discovery_tree::next(&app).is_none());
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    assert!(matches!(output, Output::DiscoveryTree(_)));
    // Normal terminal-session cleanup drops this owned worker after restoration.
}
#[test]
fn explicit_tasks_wait_for_the_owned_metadata_worker_without_consuming_busy_state() {
    let f = Fixture::new();
    let mut app = f.app(10_000, false);
    let mut worker = None;
    super::launch(
        discovery_tree::next(&app).unwrap(),
        &f.host,
        &mut app,
        &mut worker,
    )
    .unwrap();
    let mut queued = None;
    super::dispatch(Task::History, &f.host, &mut app, &mut worker, &mut queued).unwrap();
    assert!(app.busy && matches!(queued, Some(Task::History)));
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    worker = None;
    app.complete(Ok(output));
    assert!(app.busy);
    super::launch(queued.take().unwrap(), &f.host, &mut app, &mut worker).unwrap();
    let output = worker.as_mut().unwrap().join().unwrap().unwrap();
    app.complete(Ok(output));
    assert!(!app.busy);
}
