//! Presentation-only task metadata. Never retain an approval or a Git payload here.
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
    pub transaction_id: String,
    pub boundary: &'static str,
    pub sequence: usize,
}
impl Progress {
    pub fn from_event(event: &Event) -> Self {
        Self {
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
}
pub struct Failure {
    pub cause: String,
    pub activity: Option<Activity>,
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
            Task::DiscoverContent(_) | Task::DiscoveryTree(_) => {
                unreachable!("private passive reads have no activity surface")
            }
            Task::IgnoredPaths => "Manage ignored paths",
            Task::Ignore(..) => "Preview discovery preferences",
            Task::Refresh => "Refresh observations",
            Task::History => "Inspect Recovery",
            Task::Diagnostics => "Read Diagnostics",
            Task::Discover => "Discover file metadata",
            Task::Check { .. } => "Check manifest",
            Task::Profiles { .. } => "Read profile selection",
            Task::Select(_) => "Select session repository",
            Task::Init { .. } => "Preview create/attach",
            Task::Files(request) => request_label(request),
            Task::Batch(request) => batch_label(request.action()),
            Task::Rollback(..) => "Preview guarded rollback",
            Task::Bootstrap(..) => "Preview bootstrap recovery",
            Task::Git(_) => "Read Git",
            Task::Execute(approved) => {
                if let Some(reviewed) = &app.reviewed_activity {
                    return reviewed.clone();
                }
                match approved {
                    Approved::Ignore(_) => "Discovery preferences",
                    Approved::Files(_) => "File operation",
                    Approved::Batch(_) => "Batch file operation",
                    Approved::Init(..) => "Create/attach",
                    Approved::Rollback(..) => "Rollback",
                    Approved::Bootstrap(..) => "Bootstrap recovery",
                }
            }
        };
        let mut scope = vec![];
        match task {
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
                scope.push(("Transaction", transaction.clone()));
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
        }
    }
    pub fn for_pending(pending: &Pending) -> Self {
        let (operation, scope) = match pending {
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
                    ("Transaction", preview.transaction_id.clone()),
                ],
            ),
            Pending::Bootstrap(preview) => (
                match preview.action {
                    dort_core::bootstrap::RecoveryAction::Resume => "Resume bootstrap",
                    dort_core::bootstrap::RecoveryAction::Abandon => "Abandon bootstrap",
                },
                vec![
                    ("Repository", preview.repository.display().to_string()),
                    ("Transaction", preview.transaction_id.clone()),
                ],
            ),
        };
        Self {
            operation,
            scope,
            progress: None,
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
        Paragraph::new(ui::clip(banner, inner.width)).style(if danger {
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
fn controls(frame: &mut Frame, area: Rect, app: &App, failed_write: bool) {
    let choices = if failed_write {
        vec![
            ("Recovery", "h", Control::Recovery),
            ("Refresh", "r", Control::Retry),
            ("Back", "Esc", Control::Back),
        ]
    } else {
        vec![
            (
                if app.error.is_some() {
                    "Retry read"
                } else {
                    "Refresh"
                },
                "r",
                Control::Retry,
            ),
            ("Recovery", "h", Control::Recovery),
            ("Back", "Esc", Control::Back),
        ]
    };
    let (mut x, mut y) = (area.x, area.y);
    for (label, key, control) in choices {
        let button = ui::button(label, key, false);
        let width = button.width() as u16;
        if x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        app.hits
            .click(Rect::new(x, y, width, 1), Target::Feedback(control));
        frame.render_widget(
            Paragraph::new(Line::from(button)),
            Rect::new(x, y, width, 1),
        );
        x += width;
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
    if app.error.is_some() {
        app.hits.clear();
    }
    if let Some(error) = &app.error {
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
        lines.push(Line::raw(if app.failed_write { "Inspect Recovery before another write. Refresh rereads observations." } else { "Input and scope retained. Retry repeats a read or requests a fresh preview. Back returns to retained input." }));
        panel(
            frame,
            area,
            app,
            if app.failed_write {
                "Write failed"
            } else {
                "Read failed: Input retained"
            },
            if app.failed_write {
                "Possible partial effects"
            } else {
                "Read failed: no write approved by this read"
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
            lines.push(ui::metadata("Transaction ID", id));
        } else {
            lines.push(Line::raw("Everything is up to date."));
        }
        scope(&mut lines, receipt.activity.as_ref());
        if app.busy {
            lines.push(Line::raw(
                "Refreshing read-only observations; result retained.",
            ));
        }
        lines.push(Line::raw("Inspect Recovery for retained history. Back returns to the workbench; no automatic undo."));
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
    } else if app.busy {
        let mut lines = vec![];
        scope(&mut lines, app.activity.as_ref());
        if let Some(progress) = app.activity.as_ref().and_then(|a| a.progress.as_ref()) {
            lines.push(ui::metadata("Transaction", &progress.transaction_id));
            lines.push(ui::metadata("Boundary", progress.boundary));
            lines.push(ui::metadata("Sequence", &progress.sequence.to_string()));
        } else {
            lines.push(Line::raw(
                "Waiting for service metadata; no percentage is estimated.",
            ));
        }
        lines.push(Line::raw("Scope above is fixed for this task. Navigation preserves this scope. Observations update when the read finishes."));
        panel(
            frame,
            area,
            app,
            if app.writing {
                "Approved execution"
            } else {
                "Reading"
            },
            if app.exit_requested {
                "Exit requested; waiting for worker"
            } else if app.writing {
                "Approved work; quit waits for completion"
            } else {
                "Read-only task in progress"
            },
            app.writing,
            lines,
        );
        frame.render_widget(
            Paragraph::new(ui::keys(&[("q", "Quit waits"), ("PgDn", "Scroll")])),
            actions,
        );
    }
}
