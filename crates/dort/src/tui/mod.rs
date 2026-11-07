mod actions;
#[cfg(test)]
#[path = "tests/adoption_defaults.rs"]
mod adoption_defaults_tests;
mod app;
pub(crate) mod appearance;
#[cfg(test)]
#[path = "tests/appearance.rs"]
mod appearance_tests;
#[cfg(test)]
#[path = "tests/batch.rs"]
mod batch_tests;
mod batch_ui;
mod components;
mod context;
#[cfg(test)]
#[path = "tests/design.rs"]
mod design_tests;
#[cfg(test)]
#[path = "tests/direct_input.rs"]
mod direct_input_tests;
mod discover_content;
#[cfg(test)]
#[path = "tests/discover.rs"]
mod discover_tests;
mod discovery_tree;
mod events;
mod files;
mod forms;
mod git;
#[cfg(test)]
#[path = "tests/git.rs"]
mod git_tests;
mod help;
#[cfg(test)]
#[path = "tests/help.rs"]
mod help_tests;
#[cfg(test)]
#[path = "tests/ignore.rs"]
mod ignore_tests;
mod ignored;
mod input;
mod inventory;
#[cfg(test)]
#[path = "tests/menus.rs"]
mod menu_tests;
mod navigation;
#[cfg(test)]
#[path = "tests/navigation.rs"]
mod navigation_tests;
mod observations;
mod palette;
#[cfg(test)]
#[path = "tests/palette.rs"]
mod palette_tests;
#[cfg(test)]
#[path = "tests/picker.rs"]
mod picker_tests;
mod pickers;
mod recovery;
#[cfg(test)]
#[path = "tests/recovery.rs"]
mod recovery_tests;
#[cfg(test)]
#[path = "tests/responsiveness.rs"]
mod responsiveness_tests;
mod review;
mod shell;
#[cfg(test)]
#[path = "tests/task_surface.rs"]
mod task_surface_tests;
mod task_surfaces;
#[cfg(test)]
mod tests;
mod tree;
mod views;
mod visibility;
mod workbench;
#[cfg(test)]
#[path = "tests/workflow.rs"]
mod workflow_tests;

use actions::Task;
use app::App;
use dort_core::{Error, Result, context::HostContext};
use events::Worker;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use std::{path::PathBuf, time::Duration};

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = ratatui::crossterm::execute!(std::io::stdout(), event::DisableMouseCapture);
        ratatui::restore();
    }
}
fn terminal_error(source: std::io::Error) -> Error {
    Error::Io {
        operation: "run interactive terminal",
        path: PathBuf::from("<terminal>"),
        source,
    }
}
pub fn run(
    host: HostContext,
    repository: Option<PathBuf>,
    dry_run: bool,
    launch_directory: PathBuf,
    git: std::result::Result<PathBuf, String>,
    appearance: appearance::Appearance,
) -> Result<()> {
    let mut app = App::new(dry_run);
    app.appearance = appearance;
    if repository.is_some() {
        app.origin = "--repo override";
    }
    app.repository = repository.map(|root| crate::services::resolve_root(&root, &launch_directory));
    app.launch_directory = launch_directory;
    app.git = git;
    terminal_session(|terminal, worker| run_loop(terminal, &host, app, worker))
}
fn terminal_session(
    action: impl FnOnce(&mut DefaultTerminal, &mut Option<Worker>) -> Result<()>,
) -> Result<()> {
    // Guard unwinds before the worker: restore mouse/terminal modes, then join.
    let mut worker = None;
    let guard = TerminalGuard;
    let mut terminal = ratatui::try_init().map_err(terminal_error)?;
    ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture)
        .map_err(terminal_error)?;
    let result = action(&mut terminal, &mut worker);
    // Restore immediately on input/render errors, then wait for any approved write.
    drop(guard);
    drop(worker);
    result
}
fn run_loop(
    terminal: &mut DefaultTerminal,
    host: &HostContext,
    mut app: App,
    worker: &mut Option<Worker>,
) -> Result<()> {
    launch(Task::Refresh, host, &mut app, worker)?;
    let mut queued = None;
    let mut dirty = true;
    let mut last_size = None;
    loop {
        let size = terminal.size().map_err(terminal_error)?;
        if last_size != Some(size) {
            dirty = true;
            last_size = Some(size);
            app.resize(size.width, size.height);
        }
        discover_content::sync(&mut app, size.width, size.height);
        if let Some(active) = worker.as_mut() {
            if let Some(progress) = active.progress()
                && !app.exit_requested
            {
                dirty = true;
                app.status = format!("{} · sequence {}", progress.boundary, progress.sequence);
                if let Some(activity) = &mut app.activity {
                    activity.progress = Some(progress);
                }
            }
            if active.finished() {
                dirty = true;
                let output = active.join().map_err(|_| {
                    Error::Invalid(
                        "TUI service worker panicked; inspect recovery history before writing"
                            .into(),
                    )
                })?;
                *worker = None;
                // A deferred quit must not hide a failure before it could be displayed.
                let output = match output {
                    Err(error) if app.exit_requested => return Err(actions::exit_error(error)),
                    other => other.map_err(|error| actions::diagnostic(&error)),
                };
                if let Some(task) = app.complete(output) {
                    queued = Some(task);
                }
                if let Some(task) = queued.take() {
                    launch(task, host, &mut app, worker)?;
                }
            }
        }
        if app.quit {
            return Ok(());
        }
        discover_content::sync(&mut app, size.width, size.height);
        if worker.is_none()
            && let Some(task) =
                discovery_tree::next(&app).or_else(|| discover_content::next(&mut app))
        {
            launch(task, host, &mut app, worker)?;
            dirty = true;
        }
        if dirty {
            terminal
                .draw(|frame| {
                    // Normalize against the frame actually rendered, not only queued
                    // resize events: a visible below-floor frame must clear all consent.
                    let area = frame.area();
                    app.resize(area.width, area.height);
                    views::draw(frame, &app);
                })
                .map_err(terminal_error)?;
            dirty = false;
        }
        // Coalesce a bounded burst before preparing the next passive view/read.
        for index in 0..64 {
            let timeout = if index == 0 {
                Duration::from_millis(50)
            } else {
                Duration::ZERO
            };
            if !event::poll(timeout).map_err(terminal_error)? {
                break;
            }
            dirty = true;
            match event::read().map_err(terminal_error)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let size = terminal.size().map_err(terminal_error)?;
                    if let Some(task) = app.key(key, size.width, size.height) {
                        dispatch(task, host, &mut app, worker, &mut queued)?;
                    }
                }
                Event::Mouse(mouse) => {
                    let size = terminal.size().map_err(terminal_error)?;
                    if let Some(task) = app.mouse(mouse, size.width, size.height) {
                        dispatch(task, host, &mut app, worker, &mut queued)?;
                    }
                }
                Event::Resize(width, height) => app.resize(width, height),
                _ => (),
            }
            if app.quit {
                break;
            }
        }
    }
}
fn dispatch(
    task: Task,
    host: &HostContext,
    app: &mut App,
    worker: &mut Option<Worker>,
    queued: &mut Option<Task>,
) -> Result<()> {
    if worker.is_some() {
        // Wait for the owned passive read; never detach or launch a second worker.
        app.start(matches!(task, Task::Execute(_)));
        *queued = Some(task);
        Ok(())
    } else {
        launch(task, host, app, worker)
    }
}
fn launch(
    task: Task,
    host: &HostContext,
    app: &mut App,
    worker: &mut Option<Worker>,
) -> Result<()> {
    if matches!(task, Task::DiscoverContent(_) | Task::DiscoveryTree(_)) {
        debug_assert!(worker.is_none());
        *worker = Some(
            Worker::start(task, host.clone(), app.repository.clone(), vec![])
                .map_err(terminal_error)?,
        );
        return Ok(());
    }
    app.retry = task.retry_intent();
    let writing = matches!(task, Task::Execute(_));
    let completed = matches!(task, Task::Refresh) && app.status.starts_with("Operation completed.");
    let activity = task_surfaces::Activity::for_task(&task, app);
    if !matches!(task, Task::Refresh) {
        app.show_result = false;
    }
    debug_assert!(worker.is_none(), "only one owned worker at a time");
    *worker = Some(
        Worker::start(
            task,
            host.clone(),
            app.repository.clone(),
            app.requested_profiles.clone(),
        )
        .map_err(terminal_error)?,
    );
    app.activity = Some(activity);
    app.start(writing);
    if completed {
        app.status = "Operation completed. Refreshing observations...".into();
    }
    Ok(())
}
