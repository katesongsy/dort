//! Task progress and result views.
use super::{
    actions::{Approved, Pending, Task},
    app::App,
    components as ui,
    input::{Target, Wheel},
};
use dort_core::{
    adoption::{Boundary, Event},
    planner::{Request, UnmanageMode},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::Paragraph,
};

#[derive(Clone)]
pub struct Progress {
    pub installation: bool,
    pub transaction_id: String,
    pub boundary: &'static str,
    pub sequence: usize,
}
impl Progress {
    pub fn from_install(event: dort_core::tools::install::Event) -> Self {
        use dort_core::tools::install::Event::*;
        let (boundary, sequence) = match event {
            FreshValidation => ("Checking fresh install scope", 1),
            FreshInspection => ("Checking selected package managers", 2),
            IntentDurable => ("Saving install record", 3),
            Installing => ("Installing package · 10-minute limit", 4),
            PostInspection => ("Checking package and command details", 5),
            OutcomeDurable => ("Saving result", 6),
        };
        Self {
            installation: true,
            transaction_id: String::new(),
            boundary,
            sequence,
        }
    }
    pub fn from_event(event: &Event) -> Self {
        Self {
            installation: false,
            transaction_id: event.transaction_id.clone(),
            boundary: match event.boundary {
                Boundary::Checkpoint => "Durable checkpoint",
                Boundary::Mutation => "Mutation boundary",
            },
            sequence: event.sequence,
        }
    }
}
#[derive(Clone)]
pub struct Activity {
    pub operation: &'static str,
    pub scope: Vec<(&'static str, String)>,
    pub progress: Option<Progress>,
    pub provider_inspection: bool,
}
pub struct Failure {
    pub cause: String,
    pub activity: Option<Activity>,
    pub recovery: Option<Box<dort_core::journal::WriteBlocker>>,
}
pub struct Receipt {
    pub operation: &'static str,
    pub transaction_id: Option<String>,
    pub activity: Option<Activity>,
}
#[derive(Clone, Copy)]
pub enum Control {
    Retry,
    Recovery,
    Back,
}

fn request_label(request: &Request) -> &'static str {
    match request {
        Request::Apply => "Apply active mappings",
        Request::Adopt { .. } => "Adopt file",
        Request::Unmanage {
            mode: UnmanageMode::Keep,
            ..
        } => "Keep local file",
        Request::Unmanage {
            mode: UnmanageMode::Remove,
            ..
        } => "Delete managed link",
    }
}
fn batch_label(action: dort_core::batch::Action) -> &'static str {
    match action {
        dort_core::batch::Action::Adopt => "Adopt selected files",
        dort_core::batch::Action::Apply => "Apply selected mappings",
        dort_core::batch::Action::Keep => "Keep selected local files",
        dort_core::batch::Action::Remove => "Delete selected managed links",
    }
}
impl Activity {
    pub fn for_task(task: &Task, app: &App) -> Self {
        let operation = match task {
            Task::ToolsInstalledPrepare(_) => "Prepare installed-package inspection (no execution)",
            Task::ToolsInstalledInspect(_) => "Inspect installed packages",
            Task::Environment { resume: true } => "Review environment resume",
            Task::Environment { resume: false } => "Review environment plan",
            Task::EnvironmentCheck { .. } => "Review environment check",
            Task::ToolsStandaloneCheckPrepare(_) => "Prepare standalone checks (no execution)",
            Task::ToolsCheckPrepare(_) => "Prepare declared-tool checks (no execution)",
            Task::ToolsCheck(_) => "Check declared tools",
            Task::ToolsInstallMissingPrepare(_) => {
                "Review missing-tool installations (no execution)"
            }
            Task::ToolsInstallMissing(_) => "Install reviewed missing tools serially · quit waits",
            Task::DiscoverContent(_) | Task::DiscoveryTree(_) | Task::GitPreview(_) => {
                unreachable!("private passive reads have no activity surface")
            }
            Task::PreviewGenerated(_) => "Preview generated file",
            Task::IgnoredPaths => "Manage ignored paths",
            Task::Ignore(..) => "Preview discovery preferences",
            Task::Refresh => "Refresh data",
            Task::History => "Inspect Recovery",
            Task::Diagnostics => "Read Diagnostics",
            Task::Discover => "Discover files",
            Task::Check { .. } => "Check manifest",
            Task::Profile(..) => "Review profile choice",
            Task::Profiles { .. } => "Read profile selection",
            Task::Select(_) => "Select session repository",
            Task::Configuration(request) => match request.as_ref() {
                super::configuration::Request::OpenRepair(..) => "Open manifest repair draft",
                super::configuration::Request::Repair(..) => "Review manifest diff",
                _ => "Preview configuration change",
            },
            Task::EnvironmentAbandon => "Review environment abandonment",
            Task::EnvironmentRecord => "Read environment run",
            Task::Clone { .. } => "Preview clone and registration",
            Task::Init { .. } => "Preview create/attach",
            Task::Files(request) => request_label(request),
            Task::Batch(request) => batch_label(request.action()),
            Task::Rollback(..) => "Preview guarded rollback",
            Task::Bootstrap(..) => "Preview repository setup recovery",
            Task::Git(_) => "Read Git",
            Task::ToolAttempts => "Read install records (no execution)",
            Task::ToolClosePrepare(_) => "Review fresh check and record close",
            Task::ToolClose(_) => "Check package manager and close an unchanged record",
            Task::ToolDeclaration(_) => "Preview declaration change (manifest only)",
            Task::ToolsSettingsDetect { .. } => "Find tool settings (no execution)",
            Task::ToolsSettingsSave {
                preview_only: true, ..
            } => "Validate tool settings (no execution)",
            Task::ToolsSettingsSave {
                preview_only: false,
                ..
            } => "Save host-local tool settings (no execution)",
            Task::Execute(approved) => {
                if let Some(reviewed) = &app.reviewed_activity {
                    return reviewed.clone();
                }
                match approved {
                    Approved::ToolDeclaration(_) => "Tool declaration",
                    Approved::Environment(_) => "Run environment",
                    Approved::FullCheck(_) => "Run checks",
                    Approved::EnvironmentAbandon(_) => "Abandon environment run",
                    Approved::Profile(_) => "Save profile choice",
                    Approved::Configuration(_) => "Save configuration change",
                    Approved::Ignore(_) => "Discovery preferences",
                    Approved::Files(_) => "File operation",
                    Approved::Batch(_) => "Batch file operation",
                    Approved::Clone(..) => "Clone and register",
                    Approved::Init(..) => "Create/attach",
                    Approved::Rollback(..) => "Rollback",
                    Approved::Bootstrap(..) => "Repository setup recovery",
                }
            }
        };
        let mut scope = vec![];
        match task {
            Task::ToolsInstallMissing(approval) => {
                scope.push(("Manifest", approval.scope().0.display().to_string()));
                scope.push((
                    "Profiles",
                    approval
                        .scope()
                        .1
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                scope.push(("Reviewed tools", approval.tools().join(", ")));
                scope.push(("Effects", "Serial native package changes; stops on failure, possible partial effects, no rollback".into()));
            }
            Task::ToolsInstallMissingPrepare(checked) => scope.push((
                "Manifest",
                checked.reconciliation.scope.manifest.display().to_string(),
            )),
            Task::ToolAttempts => scope.push(("Scope", "All install records on this host".into())),
            Task::ToolClosePrepare(request) => scope.push(("Record", request.id.clone())),
            Task::ToolClose(_) => scope.push((
                "Scope",
                "Fresh package-manager check; closes an unchanged record only".into(),
            )),
            Task::ToolsInstalledInspect(approval) => {
                scope.push((
                    "Programs",
                    approval
                        .review()
                        .inspections
                        .iter()
                        .map(|q| q.provider.executable.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                scope.push((
                    "Scope",
                    "Approved package-manager queries; no installation".into(),
                ));
            }
            Task::ToolsCheck(approval) => {
                let review = approval.review();
                scope.push(("Manifest", review.scope.manifest.display().to_string()));
                scope.push((
                    "Reviewed tools",
                    review
                        .bindings
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                scope.push((
                    "Scope",
                    "Approved package-manager checks; no installs".into(),
                ));
            }
            // Only public scope metadata; never inspect Read, disclosure state or Diff.patch.
            Task::Git(_) => {
                scope.push((
                    "Git root",
                    app.git_view
                        .root
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "Unavailable".into()),
                ));
                scope.push(("Comparison", app.git_view.comparison.label().into()));
            }
            Task::History | Task::Diagnostics => {
                scope.push(("Scope", "All local repositories".into()))
            }
            Task::Check {
                manifest: Some(path),
                requested,
            }
            | Task::Profiles {
                manifest: Some(path),
                requested,
                ..
            } => {
                scope.push(("Manifest", path.display().to_string()));
                scope.push(("Additions", requested.join(", ")));
            }
            Task::Rollback(repository, transaction)
            | Task::Bootstrap(repository, transaction, _) => {
                scope.push(("Repository ID", repository.clone()));
                scope.push(("Operation ID", transaction.clone()));
            }
            _ => {
                let root = match task {
                    Task::Init { root, .. } => Some(root),
                    Task::Select(root) => root.as_ref(),
                    _ => app.repository.as_ref(),
                };
                if let Some(root) = root {
                    scope.push(("Repository", root.display().to_string()));
                } else {
                    scope.push((
                        "Selection",
                        "Registered active (or none); resolved afresh by service".into(),
                    ));
                    if !matches!(task, Task::Select(_))
                        && let Some(root) = app.data.as_ref().and_then(|d| d.repository.as_ref())
                    {
                        scope.push(("Prior root", root.display().to_string()));
                    }
                }
                let additions = match task {
                    Task::Check { requested, .. } | Task::Profiles { requested, .. } => {
                        Some(requested)
                    }
                    Task::Refresh | Task::Files(_) | Task::Batch(_) => {
                        Some(&app.requested_profiles)
                    }
                    _ => None,
                };
                if let Some(additions) = additions {
                    scope.push(("Additions", additions.join(", ")));
                }
            }
        }
        match task {
            Task::Files(Request::Adopt { mapping }) => {
                scope.push(("Mapping ID", mapping.id.clone()))
            }
            Task::Files(Request::Unmanage { id, .. }) => scope.push(("Mapping ID", id.clone())),
            Task::Files(Request::Apply) => scope.push((
                "Scope",
                "Full active profile union; reviewed execution scope".into(),
            )),
            Task::Batch(request) => {
                let ids = match request {
                    dort_core::batch::Request::Adopt(items) => {
                        items.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
                    }
                    dort_core::batch::Request::Apply(ids)
                    | dort_core::batch::Request::Keep(ids)
                    | dort_core::batch::Request::Remove(ids) => ids.clone(),
                };
                scope.push(("Mapping IDs", ids.join(", ")));
            }
            _ => (),
        }
        Self {
            operation,
            scope,
            progress: None,
            provider_inspection: matches!(
                task,
                Task::ToolsCheck(_) | Task::ToolsInstalledInspect(_)
            ),
        }
    }
    pub fn for_pending(pending: &Pending) -> Self {
        let (operation, scope) = match pending {
            Pending::Configuration(super::configuration::Preview::Repair(p)) => (
                "Save repaired manifest",
                vec![("Repository", p.repository().display().to_string())],
            ),
            Pending::Configuration(change) => (change.label(), vec![]),
            Pending::ToolDeclaration(p) => (
                "Tool declaration · manifest only",
                vec![
                    ("Repository", p.repository.display().to_string()),
                    ("Tool ID", p.tool.id.clone()),
                    ("No-op", p.no_op.to_string()),
                ],
            ),
            Pending::FullCheck(_) => ("Check files and tools", vec![]),
            Pending::EnvironmentAbandon(p) => (
                "Abandon environment run",
                vec![("Repository", p.repository().display().to_string())],
            ),
            Pending::Clone(p) => (
                "Clone and register",
                vec![("New folder", p.destination().display().to_string())],
            ),
            Pending::Environment(_) => ("Environment", vec![]),
            Pending::Profile(_) => ("Save profile choice", vec![]),
            Pending::Ignore(_) => (
                "Discovery preferences",
                vec![("Scope", "Local HOME discovery only".into())],
            ),
            Pending::Files(plan) => (
                request_label(&plan.inputs.request),
                vec![
                    ("Repository", plan.inputs.repository.display().to_string()),
                    (
                        "Profiles",
                        plan.inputs
                            .active_profiles
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    (
                        "Mapping IDs",
                        plan.files
                            .iter()
                            .map(|f| f.id.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ],
            ),
            Pending::Batch(plan) => {
                let operation = batch_label(plan.inputs.action);
                let mut scope = plan
                    .items
                    .first()
                    .map(|p| {
                        vec![
                            ("Repository", p.inputs.repository.display().to_string()),
                            (
                                "Profiles",
                                p.inputs
                                    .active_profiles
                                    .iter()
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ),
                        ]
                    })
                    .unwrap_or_default();
                scope.push((
                    "Mapping IDs",
                    plan.items
                        .iter()
                        .flat_map(|p| p.files.iter().map(|f| f.id.clone()))
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                (operation, scope)
            }
            Pending::Init(preview) => (
                if preview.report.create {
                    "Create local repository"
                } else {
                    "Attach local repository"
                },
                vec![(
                    "Repository",
                    preview.report.repository.display().to_string(),
                )],
            ),
            Pending::Rollback(preview) => (
                "Rollback",
                vec![
                    ("Repository ID", preview.repository_id.clone()),
                    ("Operation ID", preview.transaction_id.clone()),
                ],
            ),
            Pending::Bootstrap(preview) => (
                match preview.action {
                    dort_core::bootstrap::RecoveryAction::Resume => "Resume setup",
                    dort_core::bootstrap::RecoveryAction::Abandon => "Abandon setup",
                },
                vec![
                    ("Repository", preview.repository.display().to_string()),
                    ("Operation ID", preview.transaction_id.clone()),
                ],
            ),
        };
        Self {
            operation,
            scope,
            progress: None,
            provider_inspection: matches!(pending, Pending::FullCheck(_)),
        }
    }
}
fn scope(lines: &mut Vec<Line<'static>>, activity: Option<&Activity>) {
    if let Some(activity) = activity {
        lines.push(ui::metadata("Operation", activity.operation));
        for (label, value) in &activity.scope {
            lines.push(ui::metadata(label, value));
        }
    }
}
fn panel(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    title: &str,
    banner: &str,
    danger: bool,
    lines: Vec<Line<'static>>,
) {
    let block = ui::pane(title, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(ui::chrome_clip(banner, inner.width)).style(if danger {
            ui::danger()
        } else {
            ui::heading()
        }),
        Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
    );
    ui::text_view(
        frame,
        Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        ),
        Text::from(lines),
        app.details_scroll,
    );
    app.hits.wheel(area, Wheel::Preview);
}
fn controls(_frame: &mut Frame, _area: Rect, app: &App, failed_write: bool) {
    let choices = if failed_write {
        vec![
            ("Recovery", "Enter", Control::Recovery),
            ("Refresh data", "r", Control::Retry),
            ("Back", "Esc", Control::Back),
        ]
    } else if app.error.is_some() {
        vec![
            ("Retry read", "Enter", Control::Retry),
            ("Recovery", "h", Control::Recovery),
            ("Back", "Esc", Control::Back),
        ]
    } else {
        vec![
            ("Done", "Enter", Control::Back),
            ("Refresh data", "r", Control::Retry),
            ("Recovery", "h", Control::Recovery),
            ("Back", "Esc", Control::Back),
        ]
    };
    for (label, key, control) in choices {
        app.presented(label, key, Target::Feedback(control));
    }
}
pub fn draw(frame: &mut Frame, area: Rect, actions: Rect, app: &App) {
    let width = area.width.min(88);
    let area = Rect::new(
        area.x + (area.width - width) / 2,
        area.y,
        width,
        area.height,
    );
    let actions = Rect::new(area.x, actions.y, width, actions.height);
    if let Some(error) = &app.error {
        if let Some(blocker) = app.failure.as_ref().and_then(|f| f.recovery.as_ref()) {
            let mut lines = vec![
                Line::raw(blocker.reason()),
                Line::raw("New changes are paused to protect your files."),
                Line::raw(""),
                ui::metadata("Operation", blocker.operation()),
            ];
            if let Some(root) = &blocker.repository_root {
                lines.push(ui::metadata("Repository", &root.display().to_string()));
            }
            lines.extend([
                ui::metadata("Repository ID", &blocker.transaction.repository_id),
                ui::metadata("Operation ID", &blocker.transaction.transaction_id),
                Line::raw(""),
                Line::raw("Open Recovery to inspect it and review the available actions."),
            ]);
            panel(
                frame,
                area,
                app,
                "Changes blocked",
                "An earlier operation needs attention",
                true,
                lines,
            );
            controls(frame, actions, app, true);
            return;
        }
        let mut lines = vec![Line::from(Span::styled("Cause", ui::heading()))];
        let cause = app
            .failure
            .as_ref()
            .map(|f| f.cause.as_str())
            .unwrap_or(error);
        lines.extend(
            super::views::safe(cause)
                .lines()
                .map(|l| Line::raw(l.to_owned())),
        );
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Retained task scope",
            ui::heading(),
        )));
        scope(
            &mut lines,
            app.failure.as_ref().and_then(|f| f.activity.as_ref()),
        );
        lines.push(Line::raw(if app.failed_write { "Open Recovery before trying another write. Refresh rereads current state." } else { "Your input is kept. Retry runs the read again or asks for a new preview. Back returns to it." }));
        panel(
            frame,
            area,
            app,
            if app.failed_write {
                "Write failed"
            } else {
                "Read failed"
            },
            if app.failed_write {
                "Some changes may have been made"
            } else {
                "No changes were made"
            },
            app.failed_write,
            lines,
        );
        controls(frame, actions, app, app.failed_write);
    } else if app.show_result
        && let Some(receipt) = &app.receipt
    {
        let mut lines = vec![Line::raw(format!("{} completed.", receipt.operation))];
        if let Some(id) = &receipt.transaction_id {
            lines.push(ui::metadata("Operation ID", id));
        } else {
            lines.push(Line::raw("Everything is up to date."));
        }
        scope(&mut lines, receipt.activity.as_ref());
        if app.busy {
            lines.push(Line::raw("Refreshing read-only data. Result kept."));
        }
        lines.push(Line::raw("Open Recovery for history. Rollback is manual."));
        panel(
            frame,
            area,
            app,
            "Operation result",
            "Completed",
            false,
            lines,
        );
        if !app.busy {
            controls(frame, actions, app, false);
        }
    } else if app.show_result
        && let Some(result) = &app.result
    {
        panel(
            frame,
            area,
            app,
            "Operation result",
            &app.status,
            false,
            result
                .lines()
                .map(|line| Line::raw(line.to_owned()))
                .collect(),
        );
        for key in ['m', 'a', 's', 'x'] {
            if let Some(action) = app.result_action(ratatui::crossterm::event::KeyCode::Char(key)) {
                let label = if action == super::shell::Action::EnvironmentResume {
                    "Review resume"
                } else {
                    action.label()
                };
                app.presented(
                    label,
                    match key {
                        'm' => "m",
                        'a' => "a",
                        's' => "s",
                        _ => "x",
                    },
                    Target::Action(action),
                );
            }
        }
        app.presented("Back", "Esc", Target::Back);
    } else if app.busy {
        let mut lines = vec![];
        let progress = app.activity.as_ref().and_then(|a| a.progress.as_ref());
        if let Some(progress) = progress.filter(|p| p.installation) {
            lines.push(ui::metadata("Installation phase", progress.boundary));
        }
        scope(&mut lines, app.activity.as_ref());
        if let Some(progress) = progress {
            if !progress.installation {
                lines.push(ui::metadata("Operation ID", &progress.transaction_id));
                lines.push(ui::metadata("Step", progress.boundary));
                lines.push(ui::metadata("Step number", &progress.sequence.to_string()));
            }
        } else {
            lines.push(Line::raw("Waiting for progress."));
        }
        lines.push(Line::raw(
            "This task keeps its scope while you navigate. Results update when it finishes.",
        ));
        let title = format!(
            "{} {}",
            app.motion.symbol(app.appearance.ascii()),
            if app.writing {
                "Approved work"
            } else {
                "Reading"
            }
        );
        panel(
            frame,
            area,
            app,
            &title,
            if app.exit_requested {
                "Exit requested; waiting for the worker"
            } else if app.writing {
                "Approved work. Quit waits for it to finish."
            } else if app.activity.as_ref().is_some_and(|a| a.provider_inspection) {
                "Approved package-manager query. It can read configuration and change caches."
            } else {
                "Read-only task in progress"
            },
            app.writing,
            lines,
        );
        app.presented(
            "Quit after finish",
            "q",
            super::input::Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char('q'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        );
    }
}
