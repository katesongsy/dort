mod actions;
mod app;
mod events;
#[cfg(test)]
mod tests;
mod views;
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
pub fn run(host: HostContext, repository: Option<PathBuf>, dry_run: bool) -> Result<()> {
    terminal_session(|terminal, worker| run_loop(terminal, &host, repository, dry_run, worker))
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
    repository: Option<PathBuf>,
    dry_run: bool,
    worker: &mut Option<Worker>,
) -> Result<()> {
    let mut app = App::new(dry_run);
    launch(Task::Refresh, host, &repository, &mut app, worker)?;
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
                    Err(error) if app.exit_requested => return Err(error),
                    other => other.map_err(|error| error.to_string()),
                };
                if let Some(task) = app.complete(output) {
                    launch(task, host, &repository, &mut app, worker)?;
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
                        launch(task, host, &repository, &mut app, worker)?;
                    }
                }
                Event::Resize(_, _) => (),
                _ => (),
            }
        }
    }
}
fn launch(
    task: Task,
    host: &HostContext,
    repository: &Option<PathBuf>,
    app: &mut App,
    worker: &mut Option<Worker>,
) -> Result<()> {
    let writing = matches!(task, Task::Execute(_));
    let completed = matches!(task, Task::Refresh) && app.status.starts_with("Operation completed.");
    *worker = Some(Worker::start(task, host.clone(), repository.clone()).map_err(terminal_error)?);
    app.start(writing);
    if completed {
        app.status = "Operation completed. Refreshing observations...".into();
    }
    Ok(())
}
