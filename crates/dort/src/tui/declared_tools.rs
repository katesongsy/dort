//! Declared tools, approved automatic checks and serial missing-package reviews.
use super::input::Hits;
use super::{actions::Task, app::App, components as ui, input::Target, shell::Action};
use crate::services::declared_tools as workflow;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::Line,
};
use std::sync::Arc;
use ui::reader::{ReviewScroll, ReviewStage};

#[derive(Default)]
pub struct State {
    pub check_waiting: bool,
    pub install_waiting: bool,
    pub check_preview: Option<workflow::CheckPreview>,
    pub checked: Option<Arc<workflow::Checked>>,
    pub install_preview: Option<workflow::InstallPreview>,
    pub installed: Option<workflow::Installed>,
    pub result_open: bool,
    pub stale: bool,
    pub reader: ReviewScroll,
    viewport: super::navigation::ListViewport,
}
impl State {
    fn stage(&self) -> ReviewStage {
        if self.check_preview.is_some() {
            ReviewStage::DeclaredCheck
        } else {
            ReviewStage::InstallMissing
        }
    }
    fn approval(&self, dry_run: bool, hits: &Hits) -> ui::approval::Approval {
        ui::approval::Approval::review(
            hits,
            self.stage(),
            dry_run,
            self.check_preview.is_none()
                && self.install_preview.as_ref().is_none_or(|p| p.is_empty()),
        )
    }
    pub fn clear_reviews(&mut self) {
        self.check_waiting = false;
        self.install_waiting = false;
        self.reader = ReviewScroll::default();
        self.check_preview = None;
        self.install_preview = None;
    }
    pub fn invalidate(&mut self) {
        self.clear_reviews();
        self.checked = None;
        self.installed = None;
        self.result_open = false;
        self.stale = false;
    }
    pub fn title(&self) -> Option<&'static str> {
        if self.result_open {
            Some("Tools / Install results")
        } else if self.check_preview.is_some() {
            Some("Tools / Review installed checks")
        } else if let Some(preview) = &self.install_preview {
            Some(if preview.is_empty() {
                "Tools / Installation needs attention"
            } else {
                "Tools / Review missing installs"
            })
        } else {
            None
        }
    }
    pub fn row_status(&self, id: &str) -> &'static str {
        if let Some(result) = self
            .installed
            .as_ref()
            .and_then(|r| r.results.iter().find(|r| r.tool == id))
        {
            return if result
                .report
                .as_ref()
                .is_some_and(crate::services::installation::succeeded)
            {
                "Installed"
            } else {
                "Install failed"
            };
        }
        if self.stale {
            return if self
                .checked
                .as_ref()
                .is_some_and(|c| !c.reconciliation.plan.rows.iter().any(|r| r.id == id))
            {
                "Inactive profile"
            } else {
                "Check needed"
            };
        }
        match &self.checked {
            None => "Not checked",
            Some(checked) => checked
                .reconciliation
                .plan
                .rows
                .iter()
                .find(|r| r.id == id)
                .map_or("Inactive profile", |r| workflow::status(r).label()),
        }
    }
}
fn scroll(app: &mut App, code: KeyCode) {
    ui::reader::ScrollKeys::new(1, 6)
        .with_end()
        .apply(&mut app.details_scroll, code);
}
pub fn key(app: &mut App, key: KeyEvent) -> Option<Option<Task>> {
    let code = key.code;
    let state = &mut app.tools_view.declared;
    if state.result_open {
        match code {
            KeyCode::Enter | KeyCode::Esc => {
                state.result_open = false;
                app.details_scroll = 0;
            }
            _ => scroll(app, code),
        }
        return Some(None);
    }
    if state.install_preview.as_ref().is_some_and(|p| p.is_empty())
        && matches!(code, KeyCode::Enter | KeyCode::Char('e'))
    {
        let Some(id) = state
            .install_preview
            .as_ref()?
            .deferred
            .first()
            .map(|i| i.tool.clone())
        else {
            state.clear_reviews();
            return Some(None);
        };
        let tools = app.data.as_ref()?.tools.as_ref()?.as_ref().ok()?;
        if let Some(index) = tools.iter().position(|t| t.id == id) {
            let tool = tools[index].clone();
            app.tools_view.declared_selected = index;
            app.tools_view.declared.clear_reviews();
            app.details_scroll = 0;
            if code == KeyCode::Char('e') {
                app.tools_view
                    .change_declaration(&tool, dort_core::tools::declaration::Change::Edit);
                app.form_focus = None;
            } else {
                app.tools_view.declared_details = true;
            }
        }
        return Some(None);
    }
    if state.check_preview.is_some() || state.install_preview.is_some() {
        match code {
            KeyCode::Esc => {
                app.hits.invalidate_review();
                state.clear_reviews();
            }
            _ if state.approval(app.dry_run, &app.hits).accepts(key) => {
                if let Some(preview) = state.check_preview.take() {
                    return Some(Some(Task::ToolsCheck(Box::new(preview.approve()))));
                }
                if state
                    .install_preview
                    .as_ref()
                    .is_some_and(|p| !p.is_empty())
                {
                    return Some(
                        state
                            .install_preview
                            .take()
                            .and_then(|p| p.approve().ok())
                            .map(Task::ToolsInstallMissing),
                    );
                }
            }
            _ => state.reader.key(&app.hits, state.stage(), code),
        }
        return Some(None);
    }
    let named = match code {
        KeyCode::Char('c') => Some(Action::ToolsCheck),
        KeyCode::Char('i') => Some(Action::ToolsInstallMissing),
        KeyCode::Char('a') => Some(Action::ToolDeclaration),
        KeyCode::Char('s') if app.tools_view.declared_details => Some(Action::ToolsConfigure),
        _ => None,
    };
    if let Some(action) = named {
        return Some(app.activate(action));
    }
    if matches!(code, KeyCode::Char('e') | KeyCode::Char('d')) {
        let tool = app
            .data
            .as_ref()?
            .tools
            .as_ref()?
            .as_ref()
            .ok()?
            .get(app.tools_view.declared_selected)?
            .clone();
        let change = if code == KeyCode::Char('e') {
            dort_core::tools::declaration::Change::Edit
        } else {
            dort_core::tools::declaration::Change::Remove
        };
        app.tools_view.change_declaration(&tool, change);
        app.form_focus = None;
        app.details_scroll = 0;
        return Some(if change == dort_core::tools::declaration::Change::Remove {
            app.tools_view.declaration.prepare()
        } else {
            None
        });
    }
    if app.tools_view.declared_details {
        match code {
            KeyCode::Esc => app.tools_view.declared_details = false,
            _ => scroll(app, code),
        }
        return Some(None);
    }
    let count = app
        .data
        .as_ref()
        .and_then(|d| d.tools.as_ref()?.as_ref().ok())
        .map_or(0, Vec::len);
    let state = &mut app.tools_view;
    state.declared_selected = state.declared_selected.min(count.saturating_sub(1));
    match code {
        KeyCode::Enter if count > 0 => {
            state.declared_details = true;
            app.details_scroll = 0;
        }
        KeyCode::Enter | KeyCode::F(3) | KeyCode::F(4) => (),
        KeyCode::Down | KeyCode::PageDown => {
            let n = if code == KeyCode::Down {
                1
            } else {
                state.declared.viewport.page()
            };
            state.declared_selected = (state.declared_selected + n).min(count.saturating_sub(1));
        }
        KeyCode::Up | KeyCode::PageUp => {
            let n = if code == KeyCode::Up {
                1
            } else {
                state.declared.viewport.page()
            };
            state.declared_selected = state.declared_selected.saturating_sub(n);
        }
        KeyCode::Home => state.declared_selected = 0,
        KeyCode::End => state.declared_selected = count.saturating_sub(1),
        _ => return None,
    }
    Some(None)
}
fn action(app: &App, label: &'static str, key: &'static str, code: KeyCode) {
    app.presented(
        label,
        key,
        Target::Key(KeyEvent::new(code, KeyModifiers::NONE)),
    );
}
fn commands(app: &App) {
    let tools = app
        .data
        .as_ref()
        .and_then(|d| d.tools.as_ref())
        .and_then(|t| t.as_ref().ok());
    let tool = tools.and_then(|t| t.get(app.tools_view.declared_selected));
    let settings = app.tools_view.declared_details;
    let needs_settings = settings
        && (app.tools_view.declared.check_waiting
            || tool.is_some_and(|t| {
                app.tools_view.declared.checked.as_ref().is_some_and(|c| {
                    c.needs_setup.iter().any(|i| i.tool == t.id)
                        || c.reconciliation.plan.rows.iter().any(|r| {
                            r.id == t.id && workflow::status(r) == workflow::Status::CouldNotCheck
                        })
                })
            }));
    if needs_settings {
        app.presented("Tool settings", "s", Target::Action(Action::ToolsConfigure));
    }
    if tool.is_some() {
        if app
            .tools_view
            .declared
            .checked
            .as_ref()
            .is_some_and(|c| c.missing() > 0)
            && !app.tools_view.declared.stale
        {
            app.presented(
                "Install missing",
                "i",
                Target::Action(Action::ToolsInstallMissing),
            );
        }
        app.presented("Check installed", "C", Target::Action(Action::ToolsCheck));
        action(app, "Edit declaration", "e", KeyCode::Char('e'));
    }
    app.presented(
        "Add declaration",
        "a",
        Target::Action(Action::ToolDeclaration),
    );
    if tool.is_some() {
        action(app, "Remove declaration", "d", KeyCode::Char('d'));
    }
    if settings && !needs_settings {
        app.presented("Tool settings", "s", Target::Action(Action::ToolsConfigure));
    }
}
fn inspection_lines(body: &mut Vec<Line<'static>>, d: &dort_core::tools::inspection::Description) {
    body.push(ui::metadata("Program", &d.executable.display().to_string()));
    body.push(ui::metadata("Arguments", &d.arguments.join(" ")));
    body.push(ui::metadata("HOME", &d.home.display().to_string()));
    body.push(ui::metadata(
        "Search path",
        &d.trusted_search_path
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":"),
    ));
    if let Some(b) = &d.npm_binding {
        body.push(ui::metadata("Node", &b.node.display().to_string()));
        body.push(ui::metadata("npm CLI", &b.npm_cli.display().to_string()));
        body.push(ui::metadata(
            "Global prefix",
            &b.prefix.display().to_string(),
        ));
    }
}
fn review(frame: &mut Frame, area: Rect, app: &App) -> bool {
    let state = &app.tools_view.declared;
    let (heading, warning, primary, empty, body) = if let Some(preview) = &state.check_preview {
        let review = preview.review();
        let mut body = vec![
            Line::raw("Check the tools declared for your active profiles."),
            ui::metadata("Manifest", &review.scope.manifest.display().to_string()),
        ];
        for scope in &review.inspections {
            body.push(ui::metadata("Tools", &scope.tools.join(", ")));
            body.push(ui::metadata(
                "Package manager",
                crate::services::installed_tools::label(scope.query.provider.query),
            ));
            inspection_lines(&mut body, scope.query.provider);
        }
        for issue in review.needs_setup {
            body.push(ui::metadata(&issue.tool, &issue.reason));
            body.push(ui::metadata("Next", &issue.next_step));
        }
        (
            "Check installed",
            "Checks can run package-manager configuration and plugins, and change caches. No packages are installed.",
            "Run checks",
            false,
            body,
        )
    } else if let Some(preview) = &state.install_preview {
        let review = preview.review();
        let mut body = vec![Line::raw(format!(
            "{} ready to install in this review:",
            preview.len()
        ))];
        for (tool, install) in &review.installations {
            body.push(ui::metadata(
                tool,
                &format!("{} {}", install.scope.package, install.scope.version),
            ));
            body.push(ui::metadata(
                "Package manager",
                &install.scope.executable.display().to_string(),
            ));
            body.push(ui::metadata(
                "Installer",
                &install.installer.display().to_string(),
            ));
            body.push(ui::metadata("Arguments", &install.arguments.join(" ")));
            body.push(ui::metadata(
                "Repository",
                &install.scope.repository.display().to_string(),
            ));
            body.push(ui::metadata(
                "Command directories",
                &install
                    .scope
                    .tool_search_dirs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
            ));
            body.push(ui::metadata("Effects", install.warning));
            if let Some(b) = &install.scope.brew {
                body.push(ui::metadata(
                    "Homebrew prefix",
                    &b.prefix.display().to_string(),
                ));
                body.push(ui::metadata(
                    "Homebrew repository",
                    &b.repository.display().to_string(),
                ));
                body.push(ui::metadata("Cellar", &b.cellar.display().to_string()));
            }
            for query in &install.fresh_inspections {
                inspection_lines(&mut body, query);
            }
            for args in &install.fresh_probes {
                body.push(ui::metadata("Fresh probe arguments", &args.join(" ")));
            }
        }
        if !review.deferred.is_empty() {
            body.push(Line::raw("Needs attention (not included):"));
        } else if preview.is_empty() {
            body.push(Line::raw(
                "No missing packages. Open a tool for version or check problems.",
            ));
        }
        for issue in review.deferred {
            body.push(ui::metadata("Tool", &issue.tool));
            body.push(ui::metadata("Why", &issue.reason));
            body.push(ui::metadata("Next", &issue.next_step));
        }
        (
            if preview.is_empty() {
                "Installation needs attention"
            } else {
                "Install missing tools"
            },
            if preview.is_empty() {
                ""
            } else {
                review.warning
            },
            "Install missing",
            preview.is_empty(),
            body,
        )
    } else {
        return false;
    };
    let top = if warning.is_empty() {
        0
    } else {
        4.min(area.height)
    };
    ui::chrome_prose(
        frame,
        Rect::new(area.x, area.y, area.width, top),
        warning,
        ui::warning(),
    );
    ui::reader::Reader::new(&app.hits)
        .with_wheel(super::input::Wheel::Tools)
        .review(
            frame,
            Rect::new(
                area.x,
                area.y + top,
                area.width,
                area.height.saturating_sub(top),
            ),
            Some(heading),
            body.into(),
            state.stage(),
            &state.reader,
        );
    state
        .approval(app.dry_run, &app.hits)
        .present_key(&app.hits, primary);
    if empty
        && state
            .install_preview
            .as_ref()
            .is_some_and(|p| !p.deferred.is_empty())
    {
        action(app, "First tool details", "Enter", KeyCode::Enter);
        action(app, "Edit first declaration", "e", KeyCode::Char('e'));
    } else if empty {
        action(app, "Declared tools", "Enter", KeyCode::Enter);
    }
    action(app, "Back", "Esc", KeyCode::Esc);
    true
}
fn result(frame: &mut Frame, area: Rect, app: &App) -> bool {
    let state = &app.tools_view.declared;
    if !state.result_open {
        return false;
    }
    let Some(installed) = &state.installed else {
        return false;
    };
    let mut lines = vec![Line::raw(if installed.succeeded() {
        "Reviewed installations finished."
    } else {
        "Stopped. Earlier installs may remain. No rollback."
    })];
    for result in &installed.results {
        lines.push(ui::metadata("Tool", &result.tool));
        if let Some(error) = &result.error {
            lines.push(ui::metadata("Problem", error));
        }
        if let Some(report) = &result.report {
            let summary = report.summary();
            lines.push(ui::metadata("Outcome", summary.status.label()));
            lines.push(ui::metadata("Package", summary.package_note()));
            lines.push(ui::metadata("Command", summary.binary_label()));
            if let Some(id) = &report.attempt_id {
                lines.push(ui::metadata("Record", id));
            }
        }
    }
    if !installed.not_started.is_empty() {
        lines.push(ui::metadata(
            "Not started",
            &installed.not_started.join(", "),
        ));
    }
    lines.push(Line::raw("Check installed again before another install."));
    ui::reader::Reader::new(&app.hits)
        .with_wheel(super::input::Wheel::Tools)
        .report(
            frame,
            area,
            "Install results",
            lines.into(),
            app.details_scroll,
            false,
        );
    action(app, "Done", "Enter", KeyCode::Enter);
    true
}
fn draw_tool(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    tool: &dort_core::model::Tool,
    focused: bool,
) {
    let mut lines = vec![
        ui::metadata("Tool ID", &tool.id),
        ui::metadata("Status", app.tools_view.declared.row_status(&tool.id)),
        ui::metadata("Command", &tool.binary),
        ui::metadata("Profiles", &tool.profiles.join(", ")),
        ui::metadata("Requires", &tool.requires.join(", ")),
    ];
    if let Some(result) = app
        .tools_view
        .declared
        .installed
        .as_ref()
        .and_then(|r| r.results.iter().find(|r| r.tool == tool.id))
    {
        if let Some(report) = &result.report {
            let summary = report.summary();
            lines.push(ui::metadata("Latest install", summary.status.label()));
            lines.push(ui::metadata("Package", summary.package_note()));
            lines.push(ui::metadata("Command", summary.binary_label()));
        }
        if let Some(error) = &result.error {
            lines.push(ui::metadata("Problem", error));
        }
    }
    if let Some(checked) = &app.tools_view.declared.checked
        && !app.tools_view.declared.stale
    {
        for issue in checked.needs_setup.iter().filter(|i| i.tool == tool.id) {
            lines.push(ui::metadata("Setup", &issue.reason));
        }
        if let Some(row) = checked
            .reconciliation
            .plan
            .rows
            .iter()
            .find(|r| r.id == tool.id)
        {
            if matches!(
                row.package,
                dort_core::tools::planning::PackageFinding::Absent
            ) && !crate::services::installation::eligible(row)
            {
                for issue in workflow::install_issues(row) {
                    lines.push(ui::metadata("Why", &issue.reason));
                    lines.push(ui::metadata("Next", &issue.next_step));
                }
            }
            use dort_core::tools::planning::{BinaryObservation, Blocker, PackageFinding};
            if let PackageFinding::Present { versions } = &row.package {
                lines.push(ui::metadata(
                    "Reported versions",
                    &versions
                        .iter()
                        .map(|v| v.version.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
            lines.push(ui::metadata(
                "Command lookup",
                match row.binary_observation {
                    BinaryObservation::Missing => "Not found in checked folders",
                    BinaryObservation::Candidates { .. } => {
                        "Found, but version and owner are unverified"
                    }
                    _ => "Unavailable or not checked",
                },
            ));
            for blocker in &row.blockers {
                let reason = match blocker {
                    Blocker::DependencyNeedsReview { tool } => {
                        format!("Install {tool} first, then check again")
                    }
                    Blocker::DependencyBlocked { tool } => format!("Resolve {tool} first"),
                    Blocker::DependencyInactive { tool } => {
                        format!("Select the profile containing {tool}")
                    }
                    Blocker::VersionMismatch => {
                        "Installed version differs. Dort does not upgrade automatically.".into()
                    }
                    Blocker::VersionUnverifiable => {
                        "Reported version does not match this declaration.".into()
                    }
                    Blocker::UnattributedBinaryCandidates => {
                        "An existing command needs a check before install.".into()
                    }
                    Blocker::BinaryObservationUnavailable | Blocker::BinaryNotObserved => {
                        "Check command search folders in Tool settings.".into()
                    }
                    Blocker::RuntimeBindingUnverified => {
                        "Check the runtime's Node and npm settings.".into()
                    }
                    _ => "Package manager data is unavailable. Check Tool settings.".into(),
                };
                lines.push(ui::metadata("Needs attention", &reason));
            }
        }
    }
    {
        let install = &tool.install;
        lines.push(ui::metadata(
            "Package manager",
            workflow::provider_label(install.provider),
        ));
        lines.push(ui::metadata("Package", &install.package));
        lines.push(ui::metadata(
            "Version",
            install.version.as_deref().unwrap_or("Default"),
        ));
        if let Some(runtime) = &install.runtime_tool {
            lines.push(ui::metadata("npm runtime", runtime));
        }
    }
    ui::reader::Reader::new(&app.hits)
        .with_wheel(super::input::Wheel::Tools)
        .report(
            frame,
            area,
            "Declared tool",
            lines.into(),
            app.details_scroll,
            focused,
        );
}
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    app.hits.wheel(area, super::input::Wheel::Tools);
    if review(frame, area, app) || result(frame, area, app) {
        return;
    }
    if !app.tools_view.declared_details {
        if app
            .data
            .as_ref()
            .and_then(|d| d.tools.as_ref())
            .is_some_and(|t| t.as_ref().is_ok_and(|t| !t.is_empty()))
        {
            action(app, "Tool details", "Enter", KeyCode::Enter);
        }
        commands(app);
    }
    let Some(data) = &app.data else {
        return;
    };
    let Some(declarations) = &data.tools else {
        ui::prose(
            frame,
            area,
            "Choose or attach a repository to view declared tools.",
            ui::muted(),
        );
        return;
    };
    let tools = match declarations {
        Ok(tools) => tools,
        Err(error) => {
            ui::design::notice(
                frame,
                area,
                "Declarations unavailable",
                error,
                ui::design::Tone::Attention,
            );
            return;
        }
    };
    let selected = app
        .tools_view
        .declared_selected
        .min(tools.len().saturating_sub(1));
    let intro = if app.tools_view.declared.stale {
        "Reviewed installation results are shown.\nCheck installed again to refresh other tools."
            .to_owned()
    } else {
        app.tools_view.declared.checked.as_ref().map_or_else(
        || "Check declared tools; install what is missing.\nInstalled means the package manager reports the package.".to_owned(),
        |c| format!("{} missing; {} ready to install.\nInstalled means the package manager reports the package.\nReview missing tools to see install issues.", c.missing(), c.installable()),
    )
    };
    let top = (intro.lines().count() as u16).min(area.height);
    ui::chrome_prose(
        frame,
        Rect::new(area.x, area.y, area.width, top),
        &intro,
        ui::muted(),
    );
    let rows = Rect::new(
        area.x,
        area.y + top,
        area.width,
        area.height.saturating_sub(top),
    );
    if tools.is_empty() {
        ui::design::notice(
            frame,
            rows,
            "No declared tools",
            "Add a declaration to record a tool you want. Saving it does not install packages.",
            ui::design::Tone::Neutral,
        );
        return;
    }
    use super::workspace::{Region, Scope, Workspace};
    let active = Region::from_details(app.tools_view.declared_details);
    let workspace = Workspace::new(Scope::DeclaredTools, rows, active, true);
    workspace.bind(&app.hits);
    if let (Some(area), Some(tool)) = (workspace.details, tools.get(selected)) {
        draw_tool(frame, area, app, tool, active == Region::Details);
    }
    if active == Region::Details {
        commands(app);
        action(app, "Back", "Esc", KeyCode::Esc);
    }
    let Some(list) = workspace.queue_content(frame, "Declared tools") else {
        return;
    };
    let capacity = ui::design::pair_capacity(list);
    let first = app
        .tools_view
        .declared
        .viewport
        .start(tools.len(), capacity, Some(selected));
    let visible: Vec<_> = tools
        .iter()
        .enumerate()
        .skip(first)
        .take(capacity)
        .map(|(identity, tool)| ui::design::PairRow {
            identity,
            values: [&tool.id, app.tools_view.declared.row_status(&tool.id)],
        })
        .collect();
    ui::design::pairs(
        frame,
        list,
        ui::design::PairTable {
            headers: ["TOOL", "STATUS"],
            rows: &visible,
            selected,
        },
        &app.hits,
        Target::ToolsRow,
    );
}
