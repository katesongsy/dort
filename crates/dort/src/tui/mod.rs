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
#[cfg(test)]
#[path = "tests/calm_controls.rs"]
mod calm_controls_tests;
mod choice;
mod components;
mod configuration;
#[cfg(test)]
#[path = "tests/consistency.rs"]
mod consistency_tests;
mod context;
#[cfg(test)]
#[path = "tests/declaration_changes.rs"]
mod declaration_changes_tests;
mod declared_tools;
#[cfg(test)]
#[path = "tests/declared_workflow.rs"]
mod declared_workflow_tests;
#[cfg(test)]
#[path = "tests/design_system.rs"]
mod design_system_tests;
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
#[path = "tests/action_hints.rs"]
mod hints;
#[cfg(test)]
#[path = "tests/hints.rs"]
mod hints_tests;
#[cfg(test)]
#[path = "tests/ignore.rs"]
mod ignore_tests;
mod ignored;
mod input;
mod installed_tools;
#[cfg(test)]
#[path = "tests/installed_tools.rs"]
mod installed_tools_tests;
#[cfg(test)]
#[path = "tests/interaction.rs"]
mod interaction_tests;
mod inventory;
mod more;
mod navigation;
#[cfg(test)]
#[path = "tests/navigation.rs"]
mod navigation_tests;
mod observations;
#[cfg(test)]
#[path = "tests/picker.rs"]
mod picker_tests;
mod pickers;
mod profiles;
mod recovery;
#[cfg(test)]
#[path = "tests/recovery.rs"]
mod recovery_tests;
#[cfg(test)]
#[path = "tests/responsiveness.rs"]
mod responsiveness_tests;
mod review;
mod shell;
mod surface;
#[cfg(test)]
#[path = "tests/task_surface.rs"]
mod task_surface_tests;
mod task_surfaces;
#[cfg(test)]
mod tests;
mod tool_declaration;
#[cfg(test)]
#[path = "tests/tool_declaration.rs"]
mod tool_declaration_tests;
mod tools;
mod tools_install;
#[cfg(test)]
#[path = "tests/tools_install.rs"]
mod tools_install_tests;
#[cfg(test)]
#[path = "tests/tools_provider_fixture.rs"]
mod tools_provider_fixture;
#[cfg(test)]
#[path = "tests/tools.rs"]
mod tools_tests;
#[cfg(test)]
#[path = "tests/tools_ux.rs"]
mod tools_ux_tests;
mod tree;
mod views;
mod visibility;
mod workbench;
#[cfg(test)]
#[path = "tests/workflow.rs"]
mod workflow_tests;
mod workspace;

use actions::Task;
use app::App;
use dort_core::{Error, Result, context::HostContext};
use events::Worker;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

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
    reduced_motion: bool,
) -> Result<()> {
    let mut app = App::new(dry_run);
    app.appearance = appearance;
    app.reduced_motion = reduced_motion;
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
    let mut deferred_event = None;
    let mut dirty = true;
    let animation_clock = Instant::now();
    let mut last_size = None;
    loop {
        let size = terminal.size().map_err(terminal_error)?;
        if last_size != Some(size) {
            dirty = true;
            last_size = Some(size);
            app.resize(size.width, size.height);
        }
        discover_content::sync(&mut app, size.width, size.height);
        app.sync_git_preview();
        if let Some(active) = worker.as_mut() {
            if let Some(progress) = active.progress()
                && !app.exit_requested
            {
                dirty = true;
                app.status = components::StatusText::chrome(format!(
                    "{} · sequence {}",
                    progress.boundary, progress.sequence
                ));
                if let Some(activity) = &mut app.activity {
                    activity.progress = Some(progress);
                }
            }
            if active.finished() {
                dirty = true;
                let output = active.join().map_err(|_| {
                    Error::Invalid(
                        "A background task failed. Check Recovery before writing.".into(),
                    )
                })?;
                *worker = None;
                // A deferred quit must not hide a failure before it could be displayed.
                let output = match output {
                    Err(error) if app.exit_requested => return Err(actions::exit_error(error)),
                    other => actions::present(other),
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
            && let Some(task) = discovery_tree::next(&app)
                .or_else(|| discover_content::next(&mut app))
                .or_else(|| app.next_git_preview())
        {
            launch(task, host, &mut app, worker)?;
            dirty = true;
        }
        dirty |= app.motion.update(
            animation_clock.elapsed(),
            app.busy && app.help.is_none() && size.width >= 48 && size.height >= 14,
            app.reduced_motion,
            app.appearance.ascii(),
        );
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
            let incoming = if index == 0
                && let Some(incoming) = deferred_event.take()
            {
                incoming
            } else {
                if !event::poll(timeout).map_err(terminal_error)? {
                    break;
                }
                event::read().map_err(terminal_error)?
            };
            dirty = true;
            // More consumes the current frame's typed controls. After a coalesced edit/
            // selection, draw that state first; never reuse a previous scope's actions.
            if index > 0
                && matches!(&incoming, Event::Key(key) if key.kind == KeyEventKind::Press && key.code == event::KeyCode::F(2))
            {
                deferred_event = Some(incoming);
                break;
            }
            match incoming {
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
        app.start(matches!(
            task,
            Task::Execute(_)
                | Task::ToolClose(_)
                | Task::ToolsInstallMissing(_)
                | Task::ToolsSettingsSave {
                    preview_only: false,
                    ..
                }
        ));
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
    if matches!(
        task,
        Task::DiscoverContent(_) | Task::DiscoveryTree(_) | Task::GitPreview(_)
    ) {
        debug_assert!(worker.is_none());
        *worker = Some(
            Worker::start(task, host.clone(), app.repository.clone(), vec![])
                .map_err(terminal_error)?,
        );
        return Ok(());
    }
    app.retry = task.retry_intent();
    let writing = matches!(
        task,
        Task::Execute(_)
            | Task::ToolClose(_)
            | Task::ToolsInstallMissing(_)
            | Task::ToolsSettingsSave {
                preview_only: false,
                ..
            }
    );
    let completed = matches!(task, Task::Refresh) && app.status.starts_with("Completed.");
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
        app.status = "Completed. Refreshing...".into();
    }
    Ok(())
}
