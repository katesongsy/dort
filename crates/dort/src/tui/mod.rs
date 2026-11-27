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
mod full_check;
mod generated;
#[cfg(test)]
#[path = "tests/generated.rs"]
mod generated_tests;
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
mod template_buffer;
mod template_editor;
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
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            event::DisableMouseCapture,
            event::DisableBracketedPaste
        );
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
fn edit_template(terminal: &mut DefaultTerminal, paste_enabled: bool, app: &mut App) -> Result<()> {
    use ratatui::crossterm::{cursor, execute, terminal as tty};
    let Some(draft) = app.shell.generated.as_ref() else {
        return Ok(());
    };
    let prepare = || {
        let mut edit = template_editor::Edit::prepare(draft.template.text())?;
        let terminal = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .and_then(|file| edit.attach_tty(&file).map(|()| file))
            .map_err(|_| {
                Error::Invalid(
                    "Could not open the terminal for the editor. Use the Template field instead."
                        .into(),
                )
            })?;
        Ok((edit, terminal))
    };
    let (edit, editor_tty) = match prepare() {
        Ok(prepared) => prepared,
        Err(error) => {
            app.shell.finish_template_edit(Err(error));
            return Ok(());
        }
    };
    let _signals = template_editor::ForegroundSignals::retain_parent().map_err(terminal_error)?;
    let suspend = execute!(
        std::io::stdout(),
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
        cursor::Show
    )
    .and_then(|()| ratatui::try_restore())
    .and_then(|()| rustix::termios::tcgetattr(&editor_tty).map_err(Into::into));
    let result = match suspend {
        Ok(cooked) => {
            let result = edit.run();
            rustix::termios::tcsetattr(&editor_tty, rustix::termios::OptionalActions::Now, &cooked)
                .map_err(|error| terminal_error(error.into()))?;
            result
        }
        Err(error) => Err(terminal_error(error)),
    };
    tty::enable_raw_mode().map_err(terminal_error)?;
    execute!(
        std::io::stdout(),
        tty::LeaveAlternateScreen,
        tty::EnterAlternateScreen,
        tty::Clear(tty::ClearType::All),
        event::EnableMouseCapture
    )
    .map_err(terminal_error)?;
    if paste_enabled {
        execute!(std::io::stdout(), event::EnableBracketedPaste).map_err(terminal_error)?;
    }
    *terminal = DefaultTerminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))
        .map_err(terminal_error)?;
    app.shell.finish_template_edit(result);
    Ok(())
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
    let mut bracketed_paste = false;
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
                    other => other,
                };
                if let Some(task) = app.complete_task(output) {
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
            let generated_flow = matches!(
                app.shell.overlay,
                Some(shell::Overlay::Form(shell::Form::Generated))
            );
            if generated_flow != bracketed_paste {
                if generated_flow {
                    ratatui::crossterm::execute!(std::io::stdout(), event::EnableBracketedPaste)
                } else {
                    ratatui::crossterm::execute!(std::io::stdout(), event::DisableBracketedPaste)
                }
                .map_err(terminal_error)?;
                bracketed_paste = generated_flow;
            }
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
                Event::Paste(text) if bracketed_paste => {
                    let size = terminal.size().map_err(terminal_error)?;
                    app.paste_generated(&text, size.width, size.height);
                }
                Event::Resize(width, height) => app.resize(width, height),
                _ => (),
            }
            if std::mem::take(&mut app.shell.open_template_editor) {
                edit_template(terminal, bracketed_paste, &mut app)?;
                break;
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
        app.start(writes(&task));
        *queued = Some(task);
        Ok(())
    } else {
        launch(task, host, app, worker)
    }
}
fn writes(task: &Task) -> bool {
    match task {
        Task::Execute(actions::Approved::FullCheck(_)) => false,
        Task::Execute(_)
        | Task::ToolClose(_)
        | Task::ToolsInstallMissing(_)
        | Task::ToolsSettingsSave {
            preview_only: false,
            ..
        } => true,
        _ => false,
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
    let writing = writes(&task);
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
