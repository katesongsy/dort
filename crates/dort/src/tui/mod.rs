mod actions;
mod app;
mod components;
#[cfg(test)]
mod design_tests;
mod events;
mod files;
mod forms;
mod git;
#[cfg(test)]
mod git_tests;
mod inventory;
mod observations;
mod recovery;
#[cfg(test)]
mod recovery_tests;
mod review;
mod shell;
#[cfg(test)]
mod tests;
mod views;
mod workbench;
#[cfg(test)]
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
) -> Result<()> {
    let mut app = App::new(dry_run);
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
    // Construct the guard first: partial initialization failures also restore the terminal.
    let guard = TerminalGuard;
    let mut terminal = ratatui::try_init().map_err(terminal_error)?;
    let mut worker = None;
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
    loop {
        if let Some(active) = worker.as_mut() {
            if let Some(progress) = active.progress()
                && !app.exit_requested
            {
                app.status = progress;
            }
            if active.finished() {
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
                    launch(task, host, &mut app, worker)?;
                }
            }
        }
        if app.quit {
            return Ok(());
        }
        terminal
            .draw(|frame| views::draw(frame, &app))
            .map_err(terminal_error)?;
        if event::poll(Duration::from_millis(50)).map_err(terminal_error)? {
            match event::read().map_err(terminal_error)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let size = terminal.size().map_err(terminal_error)?;
                    if let Some(task) = app.key(key, size.width, size.height) {
                        launch(task, host, &mut app, worker)?;
                    }
                }
                Event::Resize(width, height) => app.resize(width, height),
                _ => (),
            }
        }
    }
}
fn launch(
    task: Task,
    host: &HostContext,
    app: &mut App,
    worker: &mut Option<Worker>,
) -> Result<()> {
    app.retry = task.retry_intent();
    let writing = matches!(task, Task::Execute(_));
    let completed = matches!(task, Task::Refresh) && app.status.starts_with("Operation completed.");
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
    app.start(writing);
    if completed {
        app.status = "Operation completed. Refreshing observations...".into();
    }
    Ok(())
}
