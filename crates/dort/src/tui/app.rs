use super::workspace::{Region, Scope};
use super::{
    actions::{Data, Output, Pending, Task},
    components::{
        approval::Approval,
        reader::{ReviewScroll, ReviewStage},
    },
    input::Target,
    inventory::Page as FilesPage,
    shell::{Action, Form, Intent, Overlay, ProfilePicker, Shell},
};
use dort_core::{
    bootstrap::RecoveryAction,
    planner::{Request, UnmanageMode},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

mod output;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Files,
    Recovery,
    Checks,
    Git,
    Tools,
}
pub struct Preview {
    pub pending: Pending,
    pub text: String,
    pub reader: ReviewScroll,
}
struct RowClick {
    at: std::time::Instant,
    key: usize,
    page: FilesPage,
    size: (u16, u16),
    row: u16,
}
pub(super) enum ResultNext {
    Repair(PathBuf),
    Attach,
    RecoverRun,
}

pub struct App {
    pub appearance: super::appearance::Appearance,
    pub overview_details: bool,
    pub profiles: super::profiles::Page,
    pub more: Option<super::more::More>,
    pub choice: Option<super::choice::Choice>,
    pub motion: super::components::motion::Indicator,
    pub reduced_motion: bool,
    pub tab: Tab,
    pub shell: Shell,
    pub help: Option<super::help::State>,
    pub navigation: super::navigation::State,
    pub repository: Option<PathBuf>,
    pub origin: &'static str,
    pub launch_directory: PathBuf,
    pub git: Result<PathBuf, String>,
    pub git_view: super::git::State,
    pub tools_view: super::tools::State,
    pub form_focus: Option<&'static str>,
    pub result: Option<String>,
    pub receipt: Option<super::task_surfaces::Receipt>,
    pub show_result: bool,
    pub(super) result_next: Option<ResultNext>,
    pub(super) manifest_tool_results: Vec<(String, &'static str)>,
    pub activity: Option<super::task_surfaces::Activity>,
    pub reviewed_activity: Option<super::task_surfaces::Activity>,
    pub failure: Option<super::task_surfaces::Failure>,
    pub write_warning: bool,
    pub data: Option<Data>,
    pub content: super::discover_content::State,
    return_to_discover: bool,
    rediscovering_after_adoption: bool,
    pub files_page: FilesPage,
    pub inventory: super::inventory::Inventory,
    pub batch_editor: Option<super::batch_ui::Editor>,
    pub discovery: Option<std::sync::Arc<crate::services::Discovery>>,
    pub candidate_selected: usize,
    pub requested_profiles: Vec<String>,
    pub check: Option<(Box<crate::services::CheckReport>, bool)>,
    pub diagnostics: Option<super::actions::Diagnostics>,
    pub showing_diagnostics: bool,
    pub history_search: super::inventory::Search,
    pub history_region: Region,
    pub history_viewport: super::navigation::ListViewport,
    pub retry: Option<super::actions::Retry>,
    pub selected: usize,
    pub hits: super::input::Hits,
    last_click: Option<RowClick>,
    pub details_scroll: u16,
    viewport: (u16, u16),
    pub preview: Option<Preview>,
    #[cfg(test)]
    pub confirmation: Option<()>,
    pub busy: bool,
    pub writing: bool,
    pub exit_requested: bool,
    pub quit: bool,
    pub dry_run: bool,
    preview_only_locked: bool,
    pub status: super::components::StatusText,
    pub interaction_notice: Option<&'static str>,
    pub error: Option<String>,
    pub failed_write: bool,
}
impl App {
    pub(super) fn registered_repository(&self) -> bool {
        self.data
            .as_ref()
            .and_then(|d| d.overview.as_ref())
            .and_then(|o| o.registered)
            == Some(true)
    }
    pub(super) fn checks_primary(&self) -> Action {
        if self.showing_diagnostics {
            Action::Diagnostics
        } else if self.check.is_some() {
            Action::Refresh
        } else if self
            .data
            .as_ref()
            .and_then(|d| d.repository.as_ref())
            .or(self.repository.as_ref())
            .is_some()
        {
            Action::CheckCurrent
        } else {
            Action::CheckStandalone
        }
    }
    pub fn new(dry_run: bool) -> Self {
        Self {
            appearance: Default::default(),
            overview_details: false,
            profiles: Default::default(),
            more: None,
            choice: None,
            motion: Default::default(),
            reduced_motion: false,
            tab: Tab::Overview,
            shell: Shell::new(),
            help: None,
            navigation: Default::default(),
            interaction_notice: None,
            repository: None,
            origin: "registered active",
            launch_directory: PathBuf::from("/"),
            git: Err("Git unavailable; install Git separately and restart Dort".into()),
            git_view: super::git::State::new(),
            tools_view: Default::default(),
            form_focus: None,
            result: None,
            receipt: None,
            show_result: false,
            result_next: None,
            manifest_tool_results: vec![],
            activity: None,
            reviewed_activity: None,
            failure: None,
            write_warning: false,
            data: None,
            content: Default::default(),
            return_to_discover: false,
            rediscovering_after_adoption: false,
            files_page: FilesPage::Managed,
            inventory: Default::default(),
            batch_editor: None,
            discovery: None,
            candidate_selected: 0,
            requested_profiles: vec![],
            check: None,
            diagnostics: None,
            showing_diagnostics: false,
            history_search: Default::default(),
            history_region: Region::Queue,
            history_viewport: Default::default(),
            retry: None,
            selected: 0,
            hits: Default::default(),
            last_click: None,
            details_scroll: 0,
            viewport: (48, 14),
            preview: None,
            #[cfg(test)]
            confirmation: None,
            busy: false,
            writing: false,
            exit_requested: false,
            quit: false,
            dry_run,
            preview_only_locked: dry_run,
            status: "Loading read-only data...".into(),
            error: None,
            failed_write: false,
        }
    }
    pub fn start(&mut self, writing: bool) {
        self.hits.invalidate_review();
        self.motion.reset();
        self.last_click = None;
        self.hits.clear();
        self.details_scroll = 0;
        self.failed_write = false;
        self.failure = None;
        self.busy = true;
        self.writing = writing;
        self.status = if writing {
            "Executing approved operation; quit waits for completion."
        } else {
            "Reading..."
        }
        .into();
        super::discover_content::sync(self, self.viewport.0, self.viewport.1);
    }
    pub(super) fn complete_task(&mut self, result: dort_core::Result<Output>) -> Option<Task> {
        if !self.writing
            && let Err(error) = &result
            && let Some(text) = super::actions::input_problem(error)
            && self.show_input_problem(text)
        {
            return self.complete(Ok(Output::InputRejected));
        }
        self.complete(super::actions::present(result))
    }

    fn show_input_problem(&mut self, text: String) -> bool {
        if let Some(Overlay::Form(form)) = self.shell.overlay {
            self.shell.form_details = false;
            if let Some(draft) = &mut self.shell.configuration {
                draft.feedback = None;
            }
            self.shell.form_feedback = Some((form, text));
        } else if let Some(Overlay::Profiles(picker)) = &mut self.shell.overlay {
            picker.problem = Some(text);
        } else if let Some(editor) = &mut self.batch_editor
            && editor.open
        {
            editor.problem = Some(text);
        } else if self.tab == Tab::Tools && self.tools_view.declaration_open {
            self.tools_view.declaration.failure = Some(text);
        } else if self.tab == Tab::Tools && self.tools_view.manifest_open {
            self.tools_view.declared.clear_reviews();
            self.tools_view.manifest_error = Some(text);
        } else {
            return false;
        }
        true
    }

    pub fn complete(&mut self, result: Result<Output, String>) -> Option<Task> {
        let (result, recovery) = match result {
            Ok(Output::RecoveryRequired(blocker)) => (Err(blocker.to_string()), Some(blocker)),
            other => (other, None),
        };
        let blocked = recovery.is_some();
        if let Ok(Output::DiscoverContent(report)) = result {
            self.content.accept(report);
            return None;
        }
        if let Ok(Output::GitPreview(report)) = result {
            self.sync_git_preview();
            self.git_view.accept_preview(report);
            return None;
        }
        self.last_click = None;
        if let Ok(Output::DiscoveryTree(report)) = result {
            if super::discovery_tree::accept(self, report) {
                self.clamp_file_selection();
            }
            return None;
        }
        self.hits.invalidate_review();
        let writing = self.writing;
        let activity = self.activity.take();
        self.failed_write = (writing || blocked) && result.is_err();
        self.busy = false;
        self.writing = false;
        if self.exit_requested {
            self.quit = true;
        }
        if result.is_ok() {
            self.retry = None;
        }
        if !matches!(&result, Ok(Output::Data(_) | Output::History { .. })) {
            self.result_next = None;
        }
        let task = match result {
            Ok(Output::ToolsInstalledPreview(preview)) => {
                self.accept_tools_installed_preview(preview)
            }
            Ok(Output::ToolsInstalled(current)) => self.accept_tools_installed(current),
            Ok(Output::ToolsCheckPreview(preview)) => self.accept_tools_check_preview(preview),
            Ok(Output::ToolsChecked(checked)) => self.accept_tools_checked(checked),
            Ok(Output::ToolsMissingPreview(preview)) => self.accept_tools_missing_preview(preview),
            Ok(Output::ToolsMissingInstalled(installed)) => {
                self.accept_tools_missing_installed(installed)
            }
            Err(error) => {
                if self.tools_view.install_open
                    && matches!(
                        self.tools_view.installation.mode,
                        super::tools_install::Mode::LoadingClose
                    )
                {
                    self.tools_view.installation.preparation_failed(&error);
                }
                self.return_to_discover = false;
                self.rediscovering_after_adoption = false;
                self.failure = Some(super::task_surfaces::Failure {
                    cause: error.clone(),
                    activity,
                    recovery,
                });
                self.write_warning |= writing && !blocked;
                self.show_result = false;
                self.git_view.failed();
                self.details_scroll = 0;
                self.status = if blocked {
                    error.clone().into()
                } else if writing {
                    format!(
                        "Error: {error}\nA failed write may leave partial effects. Inspect Recovery; nothing is automatically undone."
                    ).into()
                } else {
                    format!("Read failed: {error}\nEsc returns to retained input and scope.").into()
                };
                self.preview = None;
                if writing {
                    self.retry = None;
                }
                self.error = Some(self.status.to_string());
                None
            }
            Ok(
                Output::DiscoverContent(_)
                | Output::DiscoveryTree(_)
                | Output::GitPreview(_)
                | Output::RecoveryRequired(_),
            ) => unreachable!(),
            Ok(Output::Generated(preview)) => {
                self.shell.overlay = Some(Overlay::Generated(Box::new(
                    super::generated::Report::new(&preview),
                )));
                self.status = "".into();
                None
            }
            Ok(Output::GeneratedDeployed(result)) => self.accept_generated_deployment(result),
            Ok(Output::GeneratedUpdated(outcome)) => self.accept_generated_update(outcome),
            Ok(Output::History {
                rows,
                error,
                generated_recovery,
            }) => self.accept_history(rows, error, generated_recovery),
            Ok(Output::Diagnostics(report)) => self.accept_diagnostics(report),
            Ok(Output::ToolAttempts(result)) => self.accept_tool_attempts(result),
            Ok(Output::ToolClosePreview(preview)) => self.accept_tool_close_preview(preview),
            Ok(Output::ToolClosed(result)) => self.accept_tool_closed(result),
            Ok(Output::ToolsSettingsDetected(editor)) => {
                self.accept_tools_settings_detected(editor)
            }
            Ok(Output::ToolsSettingsSaved { preview_only }) => {
                self.accept_tools_settings_saved(preview_only)
            }
            Ok(Output::ToolSettingsRejected(text)) => {
                self.tools_view.settings.error = Some(text);
                self.status = "".into();
                None
            }
            Ok(Output::Git(report)) => self.accept_git(report),
            Ok(Output::Data(data)) => self.accept_data(data),
            Ok(Output::Checked { report, standalone }) => {
                self.manifest_tool_results.clear();
                self.accept_checked(report, standalone)
            }
            Ok(Output::ManifestChecked {
                report,
                standalone,
                tools,
            }) => {
                self.manifest_tool_results = tools;
                self.accept_checked(report, standalone)
            }
            Ok(Output::Profiles {
                report,
                requested,
                standalone,
                apply,
            }) => self.accept_profiles(report, requested, standalone, apply),
            Ok(Output::Discovery(discovery)) => self.accept_discovery(discovery),
            Ok(Output::Selected(repository, data)) => self.accept_selected(repository, data),
            Ok(Output::IgnoredPaths(paths)) => self.accept_ignored_paths(paths),
            Ok(Output::Preview(Pending::ToolDeclaration(_)))
                if self.tab != Tab::Tools || !self.tools_view.declaration_open || self.quit =>
            {
                None
            }
            Ok(Output::Preview(pending)) => self.accept_preview(pending),
            Ok(Output::MappingUnchanged) => {
                if matches!(self.shell.overlay, Some(Overlay::Form(Form::Generated))) {
                    self.shell.form_feedback = Some((
                        Form::Generated,
                        "No changes. Press Esc to return, or edit the template or its inputs."
                            .into(),
                    ));
                } else if let Some(draft) = &mut self.shell.configuration {
                    draft.feedback = Some(("No changes to this mapping.".into(), false));
                }
                self.status = "".into();
                None
            }
            Ok(Output::InputRejected) => {
                self.status = "".into();
                None
            }
            Ok(Output::EnvironmentRecord(run)) => {
                self.receipt = None;
                self.result_next = run
                    .as_ref()
                    .filter(|run| !run.abandoned() && !run.complete())
                    .map(|_| ResultNext::RecoverRun);
                self.result = Some(run.map_or_else(
                    || "No environment run for this repository.".into(),
                    |run| run.human(),
                ));
                self.show_result = true;
                self.details_scroll = 0;
                self.status = "Latest environment run for the selected repository.".into();
                None
            }
            Ok(Output::EnvironmentRun(run)) => {
                self.receipt = None;
                if !run.abandoned() && !run.complete() {
                    self.result_next = Some(ResultNext::RecoverRun);
                }
                self.result = Some(run.human());
                self.show_result = true;
                self.details_scroll = 0;
                self.status = if run.abandoned() {
                    "Environment run abandoned. Completed effects were kept.".into()
                } else if run.complete() {
                    "Environment completed.".into()
                } else {
                    "Environment stopped. Review the run record before resuming.".into()
                };
                self.tools_view.declared.invalidate();
                None
            }
            Ok(Output::RepairDraft(draft)) => {
                self.shell.repair = Some(super::configuration::RepairEditor::new(*draft));
                self.shell.overlay = Some(Overlay::Form(Form::Repair));
                self.form_focus = Some("manifest repair");
                self.status =
                    "Edit the draft, then review the diff. Nothing has been saved.".into();
                None
            }
            Ok(Output::CloneNeedsRepair { repository, reason }) => {
                self.shell.back();
                self.receipt = None;
                self.result = Some(format!(
                    "Clone kept at {repository:?}.\n\n{reason}\n\nRepair dort.toml before attaching the repository."
                ));
                self.result_next = Some(ResultNext::Repair(repository));
                self.show_result = true;
                self.details_scroll = 0;
                self.status = "Cloned. The manifest needs repair.".into();
                None
            }
            Ok(Output::ManifestRepaired(backup)) => {
                if let Some(draft) = self.shell.repair.take() {
                    self.shell.init_draft = draft.repository().display().to_string();
                }
                self.shell.back();
                self.receipt = None;
                self.result = Some(format!(
                    "Saved repaired manifest. The folder is still unregistered.\n\nPrivate backup: {backup:?}\n\nReview Create/attach for this exact folder to register it."
                ));
                self.show_result = true;
                self.details_scroll = 0;
                self.status = "Manifest repaired. Review attaching the repository next.".into();
                self.result_next = Some(ResultNext::Attach);
                None
            }
            Ok(Output::Completed(completion)) => self.accept_completed(completion, activity),
        };
        if task.is_some() {
            return task;
        }
        self.hits.clear();
        None
    }
    pub fn mouse(
        &mut self,
        event: ratatui::crossterm::event::MouseEvent,
        width: u16,
        height: u16,
    ) -> Option<Task> {
        let task = self.mouse_at(event, width, height, std::time::Instant::now());
        self.sync_git_preview();
        task
    }
    pub(super) fn mouse_at(
        &mut self,
        event: ratatui::crossterm::event::MouseEvent,
        width: u16,
        height: u16,
        now: std::time::Instant,
    ) -> Option<Task> {
        if self.help.is_some() {
            use ratatui::crossterm::event::MouseEventKind;
            self.resize(width, height);
            let code = match event.kind {
                MouseEventKind::ScrollDown => Some(KeyCode::Down),
                MouseEventKind::ScrollUp => Some(KeyCode::Up),
                MouseEventKind::Down(ratatui::crossterm::event::MouseButton::Left)
                    if self.hits.size() == (width, height)
                        && matches!(
                            self.hits.target(event.column, event.row),
                            Some(super::input::Target::Back | super::input::Target::DismissDialog)
                        ) =>
                {
                    Some(KeyCode::Esc)
                }
                _ => None,
            };
            return code
                .and_then(|code| self.key(KeyEvent::new(code, KeyModifiers::NONE), width, height));
        }
        use super::input::{Target, Wheel};
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        if self.hits.size() != (width, height) {
            self.resize(width, height);
            return None;
        }
        if width < 48 || height < 14 {
            self.resize(width, height);
            return None;
        }
        self.viewport = (width, height);
        let code = match event.kind {
            MouseEventKind::ScrollDown => Some(KeyCode::PageDown),
            MouseEventKind::ScrollUp => Some(KeyCode::PageUp),
            _ => None,
        };
        if let Some(code) = code {
            self.last_click = None;
            let wheel = self.hits.scroll(event.column, event.row)?;
            if let Some(workspace) = self.hits.workspace((width, height))
                && workspace.is_split()
            {
                match workspace.region_at(event.column, event.row) {
                    Some(region) if !self.text_entry() => {
                        self.focus_workspace(workspace.scope, region)
                    }
                    None if matches!(wheel, Wheel::Details | Wheel::Tools) => return None,
                    _ => (),
                }
            }
            if matches!(wheel, Wheel::Choices) {
                let code = if code == KeyCode::PageDown {
                    KeyCode::Down
                } else {
                    KeyCode::Up
                };
                return self.key(KeyEvent::new(code, KeyModifiers::NONE), width, height);
            }
            if matches!(wheel, Wheel::Content) {
                self.content.scroll_by(code == KeyCode::PageUp, 2);
                return None;
            }
            if matches!(wheel, Wheel::GitPatch) {
                self.git_view.scroll_patch(code);
                return None;
            }
            if matches!(wheel, Wheel::Details) {
                self.hits.clear();
                if self.tab == Tab::Git {
                    self.git_view.scroll_metadata_details(code);
                } else {
                    super::components::reader::ScrollKeys::DETAILS
                        .apply(&mut self.details_scroll, code);
                }
                return None;
            }
            if matches!(wheel, Wheel::Rows) && !self.busy {
                self.hits.clear();
                let backwards = code == KeyCode::PageUp;
                match self.tab {
                    Tab::Files => self.move_file_selection(backwards),
                    Tab::Recovery => self.move_history_selection(backwards),
                    Tab::Git => self.git_view.move_metadata(backwards),
                    _ => (),
                }
                self.details_scroll = 0;
                return None;
            }
            return self.key(KeyEvent::new(code, KeyModifiers::NONE), width, height);
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        if matches!(
            self.hits.target(event.column, event.row),
            Some(Target::NavigationBlocked)
        ) {
            let reason = self
                .navigation_block()
                .unwrap_or("Close or cancel the dialog with Esc before navigating.");
            self.interaction_notice = Some(reason);
            self.status = reason.into();
            return None;
        }
        let target = self.hits.target(event.column, event.row)?;
        if let Some(workspace) = self.hits.workspace((width, height))
            && workspace.is_split()
            && !self.text_entry()
            && let Some(region) = workspace.region_at(event.column, event.row)
        {
            self.focus_workspace(workspace.scope, region);
        }
        if matches!(target, Target::More) {
            let (_, hidden) = super::workbench::strip_actions(self, width);
            let mut actions = super::workbench::actions(self);
            self.hits.clear();
            if !hidden.is_empty() {
                actions.extend(hidden);
                self.open_more(actions);
            }
            return None;
        }
        if self.tab == Tab::Files
            && matches!(
                target,
                Target::Row(_) | Target::TreeFold(_) | Target::TreeMark(_) | Target::Filter
            )
        {
            self.content.focused = false;
        }
        let previous = self.last_click.take();
        let row_click = matches!(target, Target::Row(_))
            && self.tab == Tab::Files
            && self.files_page != FilesPage::Generated
            && !self.busy;
        // Row selection does not alter layout: retain render-owned hits for coalesced clicks.
        // Checkbox/chevron hits remain single-click controls, never double-click marks.
        if row_click {
            self.hits.retain_file_rows();
        } else {
            self.hits.clear();
        }
        if row_click && let Target::Row(index) = target {
            if previous.is_some_and(|click| {
                click.key == index
                    && click.page == self.files_page
                    && click.size == (width, height)
                    && click.row == event.row
                    && now.saturating_duration_since(click.at)
                        <= std::time::Duration::from_millis(400)
            }) && super::tree::position(self, index).is_some()
            {
                super::tree::select(self, index);
                super::tree::toggle(self);
                self.hits.clear();
                return None;
            }
            self.last_click = Some(RowClick {
                at: now,
                key: index,
                page: self.files_page,
                size: (width, height),
                row: event.row,
            });
        }
        self.activate_target(target, width, height)
    }
    pub(super) fn activate_target(
        &mut self,
        target: Target,
        width: u16,
        height: u16,
    ) -> Option<Task> {
        if self.busy
            && !matches!(
                target,
                Target::NavigationBlocked | Target::Action(Action::Help)
            )
        {
            return None;
        }
        if matches!(
            target,
            Target::FormControl(_, false)
                | Target::ToolDeclarationField(_)
                | Target::ToolsControl(_, false)
                | Target::BatchControl(_, false)
                | Target::ToolInstallControl(_, false)
        ) {
            self.form_focus = self.form_page();
        }
        match target {
            Target::Key(key) => self.action_key(key, width, height),
            Target::ChoiceRow(index) => {
                if let Some(choice) = &mut self.choice {
                    choice.click_row(index);
                }
                None
            }
            Target::More => {
                let (_, hidden) = super::workbench::strip_actions(self, width);
                if !hidden.is_empty() {
                    let mut actions = super::workbench::actions(self);
                    actions.extend(hidden);
                    self.open_more(actions);
                }
                None
            }
            Target::MoreRow(index) => {
                if let Some(menu) = self.more.as_mut() {
                    menu.selected = index.min(menu.items.len().saturating_sub(1));
                }
                None
            }
            Target::Destination(tab) => self.go_place(super::navigation::Place::Destination(tab)),
            Target::NavigationBlocked => {
                let reason = self
                    .navigation_block()
                    .unwrap_or("Close or cancel the dialog with Esc before navigating.");
                self.interaction_notice = Some(reason);
                self.status = reason.into();
                None
            }
            Target::LocalDestination(group, index) => {
                if let Some(reason) = self.navigation_block() {
                    self.interaction_notice = Some(reason);
                    self.status = reason.into();
                    return None;
                }
                let (_, place) = *super::navigation::local_catalog(group).get(index)?;
                self.go_place(place)
            }
            Target::Feedback(control) => self.feedback(control, width, height),
            Target::Continue => self.action_key(Approval::event(), width, height),
            Target::DiscoverFocus(content) => {
                self.content.focused = content && self.content.available();
                self.inventory.details = false;
                None
            }
            Target::BatchDetails => self
                .batch_editor
                .as_mut()
                .filter(|e| e.open)?
                .key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            Target::BatchEdit => {
                self.batch_editor.as_mut().filter(|e| e.open)?.edit_item();
                None
            }
            Target::BatchControl(index, activate) => {
                let editor = self.batch_editor.as_mut().filter(|e| e.open)?;
                if !activate {
                    let fields = super::shell::AdoptionDraft::FIELDS;
                    if editor.item_form {
                        editor.focused = editor.item * fields + index % fields;
                    } else if index < editor.drafts.len() * fields {
                        editor.item = index / fields;
                        editor.focused = editor.drafts.len() * fields;
                    }
                }
                if activate {
                    editor.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                } else {
                    None
                }
            }
            Target::TreeFold(index) | Target::TreeMark(index) => {
                super::tree::select(self, index);
                if matches!(target, Target::TreeFold(_)) {
                    let close = super::tree::selected(self).is_some_and(|n| {
                        let folded = if self.discovering() {
                            &self.inventory.candidate_folded
                        } else {
                            &self.inventory.managed_folded
                        };
                        !folded.contains(&n.path)
                    });
                    super::tree::fold(self, close);
                } else {
                    super::tree::toggle(self);
                }
                self.details_scroll = 0;
                None
            }
            Target::Workspace(scope, region) => {
                if !self.text_entry() {
                    self.focus_workspace(scope, region);
                }
                None
            }
            Target::Action(action) => self.activate(action),
            Target::DismissDialog => {
                self.interaction_notice = None;
                if self.choice.take().is_some() || self.more.take().is_some() {
                    return None;
                }
                self.shell.picker_details = false;
                self.key(
                    KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                    width,
                    height,
                )
            }
            Target::Back => self.key(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                width,
                height,
            ),
            Target::Row(index) => {
                if self.tab == Tab::Overview && self.profiles.open {
                    self.profiles.selected = index;
                } else if self.tab == Tab::Files && super::tree::position(self, index).is_some() {
                    super::tree::select(self, index);
                    self.inventory.details = false;
                } else if self.tab == Tab::Recovery && self.visible_history_rows().contains(&index)
                {
                    self.selected = index;
                    self.history_region = Region::Queue;
                } else if self.tab == Tab::Git {
                    self.git_view.select_metadata(index);
                }
                self.details_scroll = 0;
                None
            }
            Target::Filter => {
                if self.tab == Tab::Files {
                    self.inventory.details = false;
                    self.inventory.search_mut(self.files_page).editing = true;
                }
                if self.tab == Tab::Recovery {
                    self.history_region = Region::Queue;
                    self.history_search.editing = true;
                }
                if self.tab == Tab::Git {
                    self.git_view.edit_filter();
                }
                None
            }
            Target::ToolDeclarationField(index) => {
                if self.form_page() != Some("declaration")
                    || !self.tools_view.declaration.controls().contains(&index)
                {
                    return None;
                }
                self.tools_view.declaration.focus = index;
                self.open_control_choice(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                None
            }
            Target::ToolDeclarationReview => {
                if self.tab != Tab::Tools || !self.tools_view.declaration_open {
                    return None;
                }
                self.tools_view.declaration.prepare()
            }
            Target::ToolAttemptRow(index) => {
                if self.tab == Tab::Tools && self.tools_view.install_open {
                    self.tools_view.installation.selected = index;
                }
                None
            }
            Target::ToolInstallProvider => {
                if self.tab != Tab::Tools
                    || !self.tools_view.install_open
                    || self.tools_view.installation.mode != super::tools_install::Mode::CloseInputs
                {
                    return None;
                }
                self.key(
                    KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE),
                    width,
                    height,
                )
            }
            Target::ToolInstallControl(index, activate) => {
                if self.tab != Tab::Tools || !self.tools_view.install_open {
                    return None;
                }
                if !activate {
                    let count = match self.tools_view.installation.mode {
                        super::tools_install::Mode::CloseInputs => {
                            self.tools_view.installation.close_field_count()
                        }
                        _ => 1,
                    };
                    self.tools_view.installation.focus = index.min(count - 1);
                }
                if activate {
                    self.key(
                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        width,
                        height,
                    )
                } else {
                    None
                }
            }
            Target::ToolsRow(index) => {
                if self.tools_view.installed_open {
                    self.tools_view.installed.selected = index;
                } else if self.tools_view.landing() {
                    self.tools_view.declared_selected = index;
                }
                None
            }
            Target::ToolsControl(index, activate) => {
                if self.tab != Tab::Tools
                    || !(self.tools_view.settings_open || self.tools_view.manifest_open)
                {
                    return None;
                }
                if !activate && self.tools_view.settings_open {
                    self.tools_view.settings.focus =
                        index.min(self.tools_view.settings.count() - 1);
                }
                if activate {
                    self.key(
                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        width,
                        height,
                    )
                } else {
                    None
                }
            }
            Target::FormAction(command) => self
                .shell
                .form_action(command)
                .and_then(|intent| self.intent(intent)),
            Target::FormControl(index, activate) => {
                self.form_page()?;
                match self.shell.overlay {
                    Some(Overlay::Form(Form::Adopt)) => {
                        self.shell.adoption.focused =
                            index.min(super::shell::AdoptionDraft::FIELDS - 1)
                    }
                    Some(Overlay::Form(Form::Check)) => self.shell.manifest_focused = 0,
                    Some(Overlay::Form(Form::KeepId | Form::RemoveId)) => {
                        self.shell.unmanage_focused = 0
                    }
                    Some(Overlay::Form(Form::Clone)) => self.shell.path_focused = index.min(1),
                    Some(Overlay::Form(Form::Repair)) => {
                        self.shell.repair.as_mut()?.focus_at(index)
                    }
                    Some(Overlay::Form(Form::Generated)) => {
                        let draft = self.shell.generated.as_mut()?;
                        draft.focus_at(index);
                        if draft.toggle_host() {
                            self.shell.form_feedback = None;
                        }
                    }
                    Some(Overlay::Form(Form::Configure)) => {
                        let draft = self.shell.configuration.as_mut()?;
                        let toggles = draft.is_default_row(index);
                        draft.focus_at(index);
                        if toggles {
                            draft.toggle_default();
                        }
                    }
                    Some(Overlay::Form(Form::Choose | Form::Init)) => self.shell.path_focused = 0,
                    _ => return None,
                }
                let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                if self.open_control_choice(enter) {
                    None
                } else if activate {
                    self.key(enter, width, height)
                } else {
                    None
                }
            }
            Target::PickerDetails => {
                if !matches!(
                    self.shell.overlay,
                    Some(Overlay::Profiles(_) | Overlay::Repositories { .. })
                ) {
                    return None;
                }
                self.key(
                    KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE),
                    width,
                    height,
                )
            }
            Target::Picker(index, activate) => {
                let mut key = KeyCode::Enter;
                let mut modifiers = KeyModifiers::NONE;
                match &mut self.shell.overlay {
                    Some(Overlay::Repositories { selected }) => {
                        let count = self.data.as_ref().map_or(0, |d| d.known_roots.len()) + 2;
                        if index >= count {
                            key = KeyCode::Char('n');
                            modifiers = KeyModifiers::CONTROL;
                        } else {
                            *selected = index;
                        }
                    }
                    Some(Overlay::Profiles(picker)) => {
                        let count = picker.report.choices.known.len();
                        if index < count {
                            picker.selected = index;
                            key = KeyCode::Char(' ');
                        }
                    }
                    Some(Overlay::Ignored { paths, selected }) if index < paths.len() => {
                        *selected = index
                    }
                    _ => return None,
                }
                self.shell.scroll = 0;
                if activate {
                    self.key(KeyEvent::new(key, modifiers), width, height)
                } else {
                    None
                }
            }
        }
    }
    fn feedback(
        &mut self,
        control: super::task_surfaces::Control,
        width: u16,
        height: u16,
    ) -> Option<Task> {
        use super::task_surfaces::Control;
        if self.error.is_some() {
            return self.action_key(
                KeyEvent::new(
                    match control {
                        Control::Retry => KeyCode::Char('r'),
                        Control::Recovery => KeyCode::Char('h'),
                        Control::Back => KeyCode::Esc,
                    },
                    KeyModifiers::NONE,
                ),
                width,
                height,
            );
        }
        self.show_result = false;
        self.details_scroll = 0;
        match control {
            Control::Retry => {
                self.show_result = true;
                Some(Task::Refresh)
            }
            Control::Recovery => {
                self.git_view.discard_preview();
                self.tab = Tab::Recovery;
                self.shell.overlay = None;
                Some(Task::History)
            }
            Control::Back => {
                self.status = "".into();
                None
            }
        }
    }
    fn workspace_region(&self, scope: Scope) -> Option<Region> {
        if let Some(overlay) = &self.shell.overlay {
            return match (scope, overlay) {
                (Scope::Ignored, Overlay::Ignored { .. })
                | (Scope::Repositories, Overlay::Repositories { .. })
                | (Scope::Profiles, Overlay::Profiles(_)) => {
                    Some(Region::from_details(self.shell.picker_details))
                }
                _ => None,
            };
        }
        match scope {
            Scope::Recovery if self.tab == Tab::Recovery => Some(self.history_region),
            Scope::Files(page) if self.tab == Tab::Files && self.files_page == page => {
                Some(if self.inventory.details || self.content.focused {
                    Region::Details
                } else {
                    Region::Queue
                })
            }
            Scope::Git if self.tab == Tab::Git && !self.git_view.editing_root => {
                Some(self.git_view.region)
            }
            scope if self.tab == Tab::Tools && self.tools_view.workspace_visible() => match scope {
                Scope::DeclaredTools if self.tools_view.landing() => {
                    Some(Region::from_details(self.tools_view.declared_details))
                }
                Scope::InstalledTools if self.tools_view.installed_open => {
                    Some(Region::from_details(self.tools_view.installed.details))
                }
                Scope::InstallAttempts if self.tools_view.install_open => {
                    Some(Region::from_details(self.tools_view.installation.details))
                }
                _ => None,
            },
            _ => None,
        }
    }
    fn focus_workspace(&mut self, scope: Scope, region: Region) {
        if self.workspace_region(scope).is_some() {
            match scope {
                Scope::Recovery => self.history_region = region,
                Scope::Ignored | Scope::Repositories | Scope::Profiles => {
                    self.shell.picker_details = region == Region::Details
                }
                Scope::Files(page) => {
                    self.inventory.details = region == Region::Details;
                    self.content.focused = page == FilesPage::Discover
                        && self.content.available()
                        && self.inventory.details;
                }
                Scope::Git => self.git_view.region = region,
                Scope::DeclaredTools => {
                    self.tools_view.declared_details = region == Region::Details
                }
                Scope::InstalledTools => {
                    self.tools_view.installed.details = region == Region::Details
                }
                Scope::InstallAttempts => {
                    self.tools_view.installation.details = region == Region::Details
                }
            }
        }
    }
    fn paging_list(&self) -> bool {
        if self.help.is_some()
            || self.more.is_some()
            || self.choice.is_some()
            || self.busy
            || self.error.is_some()
            || self.show_result
            || self.preview.is_some()
            || self.text_entry()
            || self.batch_editor.as_ref().is_some_and(|e| e.open)
        {
            return false;
        }
        if let Some(overlay) = &self.shell.overlay {
            return matches!(overlay, Overlay::Ignored { .. }) && !self.shell.picker_details;
        }
        match self.tab {
            Tab::Files => !self.inventory.details && !self.content.focused,
            Tab::Recovery => self.history_region == Region::Queue,
            Tab::Git => self.git_view.browsing_entries(),
            Tab::Tools => self.tools_view.collection_visible(),
            _ => false,
        }
    }
    pub fn key(&mut self, mut key: KeyEvent, width: u16, height: u16) -> Option<Task> {
        if key.modifiers == KeyModifiers::CONTROL
            && let KeyCode::Char(letter @ ('f' | 'b')) = key.code
            && self.paging_list()
        {
            key = KeyEvent::new(
                if letter == 'f' {
                    KeyCode::PageDown
                } else {
                    KeyCode::PageUp
                },
                KeyModifiers::NONE,
            );
        }
        if !self.key_text_entry()
            && self.tab == Tab::Git
            && key.code == KeyCode::Char('P')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            key.code = KeyCode::Char('g');
            key.modifiers = KeyModifiers::NONE;
            return self.dispatch_key(key, width, height, false);
        }
        // Lowercase section letters are global outside text; uppercase retains
        // the former contextual workflow action. Never alter typed field text.
        if !self.key_text_entry()
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && let KeyCode::Char(letter @ ('F' | 'T' | 'R' | 'C')) = key.code
        {
            key.code = KeyCode::Char(letter.to_ascii_lowercase());
            key.modifiers = KeyModifiers::NONE;
            return self.dispatch_key(key, width, height, false);
        }
        self.dispatch_key(key, width, height, true)
    }
    /// A rendered typed control invokes its contextual action, not a section alias.
    /// All normal approval, text ownership and worker guards still run.
    pub(super) fn action_key(&mut self, key: KeyEvent, width: u16, height: u16) -> Option<Task> {
        self.dispatch_key(key, width, height, false)
    }
    fn dispatch_key(
        &mut self,
        key: KeyEvent,
        width: u16,
        height: u16,
        section_shortcuts: bool,
    ) -> Option<Task> {
        let more_actions = (key.code == KeyCode::F(2)).then(|| {
            let mut actions = super::workbench::actions(self);
            actions.extend(super::workbench::strip_actions(self, width).1);
            actions
        });
        self.hits.clear();
        self.last_click = None;
        let return_content = self
            .help
            .as_ref()
            .map(|h| (h.content_focus, h.content_scroll));
        let form_before = self.form_page();
        let transient_before = self.help.is_some() || self.more.is_some() || self.choice.is_some();
        let task = self.key_inner(key, width, height, more_actions, section_shortcuts);
        self.sync_git_preview();
        let transient_after = self.help.is_some() || self.more.is_some() || self.choice.is_some();
        if transient_before != transient_after {
            self.hits.invalidate_review();
        }
        if !transient_before
            && !transient_after
            && form_before != self.form_page()
            && self.form_focus != self.form_page()
        {
            self.form_focus = None;
        }
        super::discover_content::sync(self, width, height);
        if self.help.is_none()
            && self.content.available()
            && let Some((focus, scroll)) = return_content
        {
            self.content.focused = focus;
            self.content.scroll = scroll;
        }
        if self.text_entry() {
            self.content.focused = false;
        }
        task
    }
    fn key_inner(
        &mut self,
        key: KeyEvent,
        width: u16,
        height: u16,
        more_actions: Option<Vec<super::components::ActionButton>>,
        section_shortcuts: bool,
    ) -> Option<Task> {
        let interrupt =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if interrupt
            || (key.code == KeyCode::Char('q')
                && self.more.is_none()
                && self.choice.is_none()
                && (self.help.is_some()
                    || (self.error.is_some()
                        || self.show_result
                        || self.busy
                        || self.preview.is_some()
                        || !self.text_entry())))
        {
            self.git_view.discard_preview();
            if self.busy {
                self.exit_requested = true;
                self.status = "Exit requested; waiting for the active worker. Approved writes run to completion.".into();
            } else {
                self.quit = true;
            }
            return None;
        }
        self.resize(width, height);
        if width >= 48
            && height >= 14
            && key.modifiers == KeyModifiers::CONTROL
            && self.can_switch_scope()
        {
            let action = match key.code {
                KeyCode::Char('p') => Some(Action::Choose),
                KeyCode::Char('t') if self.registered_repository() => Some(Action::ProfilesPage),
                _ => None,
            };
            if let Some(action) = action {
                return self.go_place(super::navigation::Place::Action(action));
            }
        }
        if section_shortcuts
            && key.modifiers == KeyModifiers::NONE
            && !self.key_text_entry()
            && let KeyCode::Char(letter) = key.code
            && !(letter == 'c' && self.environment_can_check())
            && let Some(place) = super::navigation::section_shortcut(letter)
        {
            if width < 48 || height < 14 {
                return None;
            }
            if let Some(reason) = self.navigation_block() {
                self.interaction_notice = Some(reason);
                self.status = reason.into();
                return None;
            }
            return self.go_place(place);
        }
        if key.modifiers == KeyModifiers::NONE
            && let KeyCode::Char(digit @ '1'..='9') = key.code
            && width >= 48
            && height >= 14
            && self.dialog_title().is_none()
            && !self.text_entry()
            && super::navigation::local_available(self)
            && let Some((_, place)) =
                super::navigation::local_catalog(super::navigation::group(self))
                    .get((digit as u8 - b'1') as usize)
        {
            return self.go_place(*place);
        }
        if self.dialog_title().is_some() && matches!(key.code, KeyCode::Left | KeyCode::Right) {
            let reason = self
                .navigation_block()
                .unwrap_or("Close this dialog with Esc before navigating.");
            self.interaction_notice = Some(reason);
            self.status = reason.into();
            return None;
        }
        self.interaction_notice = None;
        if let Some(help) = &self.help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') => self.help = None,
                code => help.scroll(code),
            }
            return None;
        }
        if key.code == KeyCode::F(1)
            || (key.code == KeyCode::Char('?') && !super::help::text_input(self))
        {
            self.open_help();
            return None;
        }
        if width < 48 || height < 14 {
            if key.code == KeyCode::Esc {
                if self.error.is_some() {
                    self.error = None;
                    self.failure = None;
                    self.failed_write = false;
                    self.retry = None;
                } else if self.show_result {
                    self.show_result = false;
                    self.details_scroll = 0;
                } else if self.preview.is_some() {
                    self.preview = None;
                } else if self.shell.overlay.is_some() {
                    self.shell.back();
                } else {
                    self.inventory.search_mut(self.files_page).editing = false;
                    self.inventory.details = false;
                    self.history_search.editing = false;
                    self.history_region = Region::Queue;
                }
            }
            self.status = "Resize to at least 48x14 to review and approve.".into();
            return None;
        }
        if self.choice.is_some() {
            return self.choice_key(key.code);
        }
        if self.more.is_some() {
            return self.more_key(key.code, width, height);
        }
        if key.code == KeyCode::F(2) {
            if self.dialog_title().is_none() {
                self.open_more(more_actions.unwrap_or_default());
            }
            return None;
        }
        if key.code == KeyCode::F(3) && self.preview.is_some() && !self.busy {
            return self.key(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
                width,
                height,
            );
        }
        if self.tab == Tab::Overview
            && self.batch_editor.as_ref().is_none_or(|e| !e.open)
            && self.shell.overlay.is_none()
            && !self.busy
            && self.error.is_none()
            && !self.show_result
            && self.preview.is_none()
        {
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                if self.profiles.open {
                    return self.profiles_key(key.code);
                }
                if let Some(action) = super::context::control_action(
                    key.code,
                    super::observations::overview_controls(self),
                ) {
                    return self.activate(action);
                }
            }
            match key.code {
                KeyCode::F(3) => {
                    self.overview_details = !self.overview_details;
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Esc if self.overview_details => {
                    self.overview_details = false;
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Down | KeyCode::PageDown if self.overview_details => {
                    self.details_scroll = self.details_scroll.saturating_add(2);
                    return None;
                }
                KeyCode::Up | KeyCode::PageUp if self.overview_details => {
                    self.details_scroll = self.details_scroll.saturating_sub(2);
                    return None;
                }
                KeyCode::Home if self.overview_details => {
                    self.details_scroll = 0;
                    return None;
                }
                _ => (),
            }
        }
        if self.busy {
            if matches!(
                key.code,
                KeyCode::Esc
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Char('p' | 'n' | 'f' | ';' | '/')
            ) {
                let reason = "Navigation paused while owned work completes; q/Ctrl-C waits joined, not cancellation.";
                self.interaction_notice = Some(reason);
                self.status = reason.into();
            }
            match key.code {
                KeyCode::Home => self.details_scroll = 0,
                KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(2),
                KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(2),
                _ => (),
            }
            return None;
        }
        if self.error.is_some() {
            match key.code {
                KeyCode::Enter => {
                    return self.feedback(
                        if self.failed_write {
                            super::task_surfaces::Control::Recovery
                        } else {
                            super::task_surfaces::Control::Retry
                        },
                        width,
                        height,
                    );
                }
                KeyCode::Char('h') => {
                    self.error = None;
                    self.preview = None;
                    self.retry = None;
                    self.tab = Tab::Recovery;
                    self.shell.overlay = None;

                    self.details_scroll = 0;
                    return Some(Task::History);
                }
                KeyCode::Char('r') => {
                    self.error = None;
                    self.preview = None;
                    self.details_scroll = 0;
                    if let Some(intent) = self.retry.take() {
                        return Some(intent.task());
                    }
                    if self.tab == Tab::Git {
                        return self.git_task();
                    }
                    if self.tab == Tab::Tools {
                        if self.tools_view.install_open {
                            return self.tools_view.installation.retry_preview();
                        }
                        if self.tools_view.declaration_open {
                            return self.tools_view.declaration.prepare();
                        }
                        if self.tools_view.manifest_open {
                            return Some(self.tools_view.manifest_check());
                        }
                        if self.tools_view.settings_open {
                            return self.tools_view.settings.prepare(self.dry_run);
                        }
                        return self.activate(Action::ToolsCheck);
                    }
                    return Some(self.refresh_task());
                }
                KeyCode::Esc => {
                    self.error = None;
                    self.failure = None;
                    self.failed_write = false;
                    self.retry = None;
                    self.status = "".into();
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Home => self.details_scroll = 0,
                KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(2),
                KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(2),
                _ => (),
            }
            return None;
        }
        if self.show_result && self.preview.is_none() {
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && let Some(action) = self.result_action(key.code)
            {
                return self.activate(action);
            }
            use super::task_surfaces::Control;
            let control = match key.code {
                KeyCode::Char('r') => Some(Control::Retry),
                KeyCode::Char('h') => Some(Control::Recovery),
                KeyCode::Enter | KeyCode::Esc => Some(Control::Back),
                KeyCode::Home | KeyCode::PageDown | KeyCode::PageUp => {
                    super::components::reader::ScrollKeys::DETAILS
                        .apply(&mut self.details_scroll, key.code);
                    return None;
                }
                KeyCode::Tab => return None,
                code if code == Approval::event().code => return None,
                _ => None,
            };
            if let Some(control) = control {
                return self.feedback(control, width, height);
            }
            self.show_result = false;
            self.details_scroll = 0;
        }
        if self.preview_approval().accepts(key) {
            return self.approve_preview();
        }
        if key.code == KeyCode::Char('w')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && self.file_review_needs_environment()
        {
            return self.activate(Action::EnvironmentPlan);
        }
        if matches!(key.code, KeyCode::Char('c' | 'C'))
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && self.environment_can_check()
        {
            return self.activate(Action::EnvironmentCheck);
        }
        if let Some(preview) = &mut self.preview {
            match key.code {
                KeyCode::Esc => {
                    let back = match &preview.pending {
                        Pending::FullCheck(check) => check.cancel(),
                        _ => None,
                    };
                    self.preview = None;
                    self.hits.invalidate_review();
                    self.status = "".into();
                    return back;
                }
                code => preview.reader.key(&self.hits, ReviewStage::Changes, code),
            }
            return None;
        }
        if let Some(owner) = self.form_page() {
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) && !self.form_has_focus() {
                self.focus_first_field();
                self.form_focus = Some(owner);
                return None;
            }
            if key.code == KeyCode::Esc && self.form_has_focus() && owner != "generated file" {
                self.form_focus = None;
                return None;
            }
            if !self.form_has_focus()
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && matches!(
                    key.code,
                    KeyCode::Char(_)
                        | KeyCode::Backspace
                        | KeyCode::Delete
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Home
                        | KeyCode::End
                )
            {
                return None;
            }
        }
        if let Some(editor) = self.batch_editor.as_mut().filter(|e| e.open) {
            return editor.key(key);
        }
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && !self.text_entry()
            && let Some(workspace) = self.hits.workspace((width, height))
            && workspace.is_split()
            && let Some(active) = self.workspace_region(workspace.scope)
        {
            self.focus_workspace(workspace.scope, active.other());
            return None;
        }
        if self.open_control_choice(key) {
            return None;
        }
        if let Some(Overlay::Generated(report)) = &self.shell.overlay {
            if matches!(key.code, KeyCode::Char('r' | 'R'))
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                if report.can_review_deployment() {
                    return Some(Task::ReviewGeneratedDeployment(report.id.clone()));
                }
                if let Some(action) = report.review_action() {
                    let id = report.id.clone();
                    self.shell.back();
                    return Some(Task::ReviewGeneratedUpdate(id, action));
                }
            }
            if key.code == KeyCode::Esc {
                self.shell.back();
            } else {
                report.scroll.key(
                    &self.hits,
                    super::components::reader::ReviewStage::Generated,
                    key.code,
                );
            }
            return None;
        }
        if self.shell.overlay.is_some() {
            let report_scroll = !self.shell.form_details
                && !self.shell.picker_details
                && !matches!(
                    self.shell.overlay,
                    Some(Overlay::Profiles(_) | Overlay::Repositories { .. })
                );
            if report_scroll {
                match key.code {
                    KeyCode::Home => self.shell.scroll = 0,
                    KeyCode::PageDown => self.shell.page(true),
                    KeyCode::PageUp => self.shell.page(false),
                    _ => (),
                }
            }
            let count = self.data.as_ref().map_or(0, |data| data.known_roots.len());
            if let Some(intent) = self.shell.key(key, count) {
                return self.intent(intent);
            }
            return None;
        }
        if self.tab == Tab::Tools
            && self.tools_view.landing()
            && let Some(task) = super::declared_tools::key(self, key)
        {
            return task;
        }
        if self.tab == Tab::Tools
            && let Some(task) = self.tools_view.key(key, self.dry_run, &self.hits)
        {
            return task;
        }
        // Tab traverses fields only; explicit readers retain their owned viewport.
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) && !self.text_entry() {
            return None;
        }
        if self.tab == Tab::Files
            && self.discovering()
            && self.content.focused
            && !self.text_entry()
        {
            if key.code == KeyCode::Esc {
                self.content.focused = false;
                self.inventory.details = false;
                return None;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let page = self.content.page.get().max(1);
                let movement = match key.code {
                    KeyCode::Char('f') => Some((false, page)),
                    KeyCode::Char('b') => Some((true, page)),
                    KeyCode::Char('d') => Some((false, (page / 2).max(1))),
                    KeyCode::Char('u') => Some((true, (page / 2).max(1))),
                    _ => None,
                };
                if let Some((backwards, rows)) = movement {
                    self.content.scroll_by(backwards, rows);
                    return None;
                }
            } else if !key.modifiers.contains(KeyModifiers::ALT) {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => self.content.scroll_by(false, 1),
                    KeyCode::Char('k') | KeyCode::Up => self.content.scroll_by(true, 1),
                    KeyCode::Home => self.content.scroll = 0,
                    KeyCode::End => self.content.scroll = self.content.end.get(),
                    KeyCode::PageDown => self.content.scroll_by(false, 2),
                    KeyCode::PageUp => self.content.scroll_by(true, 2),
                    _ => return self.workspace_key(key),
                }
                return None;
            }
        }
        self.workspace_key(key)
    }
    fn workspace_key(&mut self, key: KeyEvent) -> Option<Task> {
        if self.tab == Tab::Recovery
            && self.history_region == Region::Details
            && !self.history_search.editing
        {
            if super::components::reader::ScrollKeys::DETAILS
                .apply(&mut self.details_scroll, key.code)
            {
                return None;
            }
            match key.code {
                KeyCode::Esc => {
                    self.history_region = Region::Queue;
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Enter => return None,
                _ => (),
            }
        }
        if key.modifiers == KeyModifiers::NONE && !self.text_entry() && self.template_selected() {
            match key.code {
                KeyCode::Enter => return self.activate(Action::PreviewGenerated),
                KeyCode::F(3) => return self.activate(Action::Inspect),
                _ => (),
            }
        }
        if self.tab == Tab::Files
            && self.inventory.details
            && !self.content.focused
            && !self.text_entry()
        {
            match key.code {
                KeyCode::Down | KeyCode::PageDown | KeyCode::Char('j') => {
                    self.details_scroll = self.details_scroll.saturating_add(2);
                    return None;
                }
                KeyCode::Up | KeyCode::PageUp | KeyCode::Char('k') => {
                    self.details_scroll = self.details_scroll.saturating_sub(2);
                    return None;
                }
                KeyCode::Home => {
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Esc => {
                    self.inventory.details = false;
                    self.details_scroll = 0;
                    return None;
                }
                KeyCode::Enter => return None,
                _ => (),
            }
        }
        if !self.text_entry() && !key.modifiers.contains(KeyModifiers::ALT) {
            let movement = |page| match key.code {
                KeyCode::PageDown => Some((false, page)),
                KeyCode::PageUp => Some((true, page)),
                KeyCode::Home => Some((true, usize::MAX)),
                KeyCode::End => Some((false, usize::MAX)),
                _ => None,
            };
            if self.tab == Tab::Files
                && !self.inventory.details
                && !self.content.focused
                && let Some((backwards, count)) =
                    movement(self.inventory.viewport(self.files_page).page())
            {
                self.move_files(backwards, count);
                return None;
            }
            if self.tab == Tab::Recovery
                && self.history_region == Region::Queue
                && self
                    .data
                    .as_ref()
                    .is_some_and(|d| d.history_error.is_none())
                && let Some((backwards, count)) = movement(self.history_viewport.page())
            {
                self.page_history(backwards, count);
                return None;
            }
            if self.tab == Tab::Git
                && self.git_view.browsing_entries()
                && let Some((backwards, count)) = movement(self.git_view.viewport.page())
            {
                self.git_view.page_metadata(backwards, count);
                return None;
            }
        }
        if self.tab == Tab::Git {
            if self.git_view.editing_root
                && !self.git_view.root_metadata()
                && key.code == KeyCode::Enter
                && let Err(error) = &self.git
            {
                self.failure = None;
                self.failed_write = false;
                self.error = Some(format!(
                    "{error}\nInstall Git separately and restart Dort. Draft retained."
                ));
                return None;
            }
            if let Some(task) = self.git_view.key(key, &self.launch_directory, &self.git) {
                return task;
            }
            let action = match key.code {
                KeyCode::Char('g') => Some(Action::GitRoot),
                KeyCode::Char('z') => Some(Action::GitStatus),
                KeyCode::Char('u') => Some(Action::GitUnstaged),
                KeyCode::Char('s') => Some(Action::GitStaged),
                KeyCode::Char('r') => Some(Action::GitRefresh),
                _ => None,
            };
            if let Some(action) = action {
                return self.activate(action);
            }
        }
        if self.tab == Tab::Recovery && self.history_search.editing {
            self.history_search.key(key);
            self.clamp_history_selection();
            self.details_scroll = 0;
            return None;
        }
        if self.tab == Tab::Files && self.inventory.search(self.files_page).editing {
            self.inventory.search_mut(self.files_page).key(key);
            self.clamp_file_selection();
            self.details_scroll = 0;
            return None;
        }
        if self.tab == Tab::Files
            && (!self.inventory.details || self.discovering())
            && !self.content.focused
        {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let page = self.inventory.viewport(self.files_page).page();
                let (backwards, count) = match key.code {
                    KeyCode::Char('d') => (false, (page / 2).max(1)),
                    KeyCode::Char('u') => (true, (page / 2).max(1)),
                    _ => (false, 0),
                };
                if count > 0 {
                    self.move_files(backwards, count);
                    return None;
                }
            }
            match key.code {
                _ if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {}
                KeyCode::Char('j') => {
                    self.move_files(false, 1);
                    return None;
                }
                KeyCode::Char('k') => {
                    self.move_files(true, 1);
                    return None;
                }
                KeyCode::Char('G') if !self.discovering() => {
                    self.move_files(false, usize::MAX);
                    return None;
                }
                _ => (),
            }
        }
        if matches!(key.code, KeyCode::Char(_))
            && key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let opens_overlay = matches!(key.code, KeyCode::Char('?' | '/' | 'p' | 'n'));
        if !matches!(
            key.code,
            KeyCode::PageDown | KeyCode::PageUp | KeyCode::Tab | KeyCode::BackTab
        ) && !opens_overlay
            && !matches!(self.tab, Tab::Files | Tab::Recovery)
        {
            self.details_scroll = 0;
        }
        if self.tab == Tab::Files {
            let action = super::context::control_action(key.code, super::files::controls(self))
                .or_else(|| super::context::file_action(key.code, self));
            if let Some(action) = action {
                return self.activate(action);
            }
        }
        match key.code {
            KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(2),
            KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(2),
            KeyCode::Home => self.details_scroll = 0,
            KeyCode::Char('h') | KeyCode::Left if self.tab == Tab::Files => {
                super::tree::fold(self, true)
            }
            KeyCode::Char('l') if self.tab == Tab::Files => super::tree::fold(self, false),
            KeyCode::Right if self.tab == Tab::Files => super::tree::expand(self),
            KeyCode::Esc => {
                self.inventory.details = false;
                self.history_region = Region::Queue;

                self.result = None;
            }
            KeyCode::Char('/') if self.tab == Tab::Files => {
                self.inventory.details = false;
                self.inventory.search_mut(self.files_page).editing = true
            }
            KeyCode::Char('/') if self.tab == Tab::Recovery => {
                self.history_region = Region::Queue;
                self.history_search.editing = true;
            }
            KeyCode::Delete if self.tab == Tab::Recovery => {
                self.history_search.query.clear();
                self.clamp_history_selection();
            }
            KeyCode::Enter if self.tab == Tab::Recovery => {
                if self.history_selection().is_some() {
                    self.history_region = Region::Details;
                }
                self.details_scroll = 0;
            }
            KeyCode::Down if self.tab == Tab::Recovery => self.move_history_selection(false),
            KeyCode::Up if self.tab == Tab::Recovery => self.move_history_selection(true),
            KeyCode::Char('j') if self.tab == Tab::Checks => {
                return self.activate(Action::Diagnostics);
            }
            KeyCode::Char('e') if self.tab == Tab::Checks && !self.showing_diagnostics => {
                return self.activate(Action::Configure(super::configuration::Kind::Repair));
            }
            KeyCode::Char('e')
                if self.tab == Tab::Recovery && self.history_region == Region::Queue =>
            {
                return self.activate(Action::EnvironmentRecord);
            }
            KeyCode::Char('m') if self.tab == Tab::Checks => {
                self.showing_diagnostics = false;
                self.details_scroll = 0;
            }
            KeyCode::Delete if self.tab == Tab::Files => return self.activate(Action::ClearFilter),
            KeyCode::Char('p') => return self.activate(Action::Choose),
            KeyCode::Char('c') => return self.activate(Action::CheckCurrent),
            KeyCode::Char('v')
                if self.tab != Tab::Tools
                    && !(self.tab == Tab::Files && self.files_page == FilesPage::Generated) =>
            {
                return self.activate(Action::CheckStandalone);
            }
            KeyCode::Char('t') => return self.activate(Action::Profiles),
            KeyCode::Char('?') => return self.activate(Action::Help),
            KeyCode::Char('r') => return self.activate(Action::Refresh),
            KeyCode::Char('f') => return self.activate(Action::Discover),
            KeyCode::Char('m') if self.tab != Tab::Files => return self.activate(Action::Adopt),
            KeyCode::Enter if self.tab == Tab::Checks => {
                return self.activate(self.checks_primary());
            }
            KeyCode::Enter if self.tab == Tab::Files => return self.activate(Action::Inspect),
            KeyCode::Down if self.tab == Tab::Files => self.move_file_selection(false),
            KeyCode::Up if self.tab == Tab::Files => self.move_file_selection(true),
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.row_count().saturating_sub(1))
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('a') if self.tab == Tab::Overview => {
                return self.activate(Action::Apply);
            }
            KeyCode::Char('u')
                if self.tab == Tab::Recovery
                    && self
                        .data
                        .as_ref()
                        .and_then(|d| d.generated_recovery.as_ref())
                        .is_some_and(Result::is_ok) =>
            {
                return Some(Task::ReviewGeneratedRecovery);
            }
            KeyCode::Char('b') if self.tab == Tab::Recovery => {
                return self.activate(Action::Rollback);
            }
            KeyCode::Char('s') if self.tab == Tab::Recovery => {
                return self.activate(Action::Resume);
            }
            KeyCode::Char('x') if self.tab == Tab::Recovery => {
                return self.activate(Action::Abandon);
            }
            _ => (),
        }
        self.clamp_file_selection();
        self.clamp_history_selection();
        None
    }
    pub fn resize(&mut self, width: u16, height: u16) {
        if self.viewport != (width, height) {
            self.last_click = None;
            self.hits.clear();
        }
        if self.hits.size() != (width, height) {
            self.hits.invalidate_review();
            self.hits.invalidate_workspace();
        }
        self.viewport = (width, height);
        if width < 48 || height < 14 {
            self.more = None;
            self.choice = None;
            self.git_view.discard_preview();
            self.tools_view.shrink();
        }
        super::discover_content::sync(self, width, height);
    }
    fn replace_data(&mut self, data: Data) {
        let saved_template = self.inventory.saved_template.take().and_then(|id| {
            data.files
                .iter()
                .position(|row| row.0 == id)
                .filter(|&i| data.is_template(i))
        });
        let previous_template = self.data.as_ref().and_then(|old| {
            let index = self.inventory.generated_selected;
            let row = old.files.get(index).filter(|_| old.is_template(index))?;
            data.files.iter().position(|next| next.0 == row.0)
        });
        self.inventory.generated_selected = saved_template
            .or(previous_template)
            .unwrap_or(self.inventory.generated_selected);
        let old_id = self.data.as_ref().and_then(|old| match self.tab {
            Tab::Files => old.files.get(self.selected).map(|row| row.0.clone()),
            Tab::Recovery => old
                .history
                .get(self.selected)
                .map(|row| format!("{}:{}", row.repository_id, row.transaction_id)),
            _ => None,
        });
        let had_selection = old_id.is_some();
        let position = old_id.and_then(|id| match self.tab {
            Tab::Files => data.files.iter().position(|row| row.0 == id),
            Tab::Recovery => data
                .history
                .iter()
                .position(|row| format!("{}:{}", row.repository_id, row.transaction_id) == id),
            _ => None,
        });
        if had_selection && position.is_none() {
            self.status = "Selected item disappeared; showing the nearest remaining row.".into();
        }
        // Only a successful full inventory establishes that a marked ID is gone.
        // Display filters and unavailable observations must preserve valid marks.
        if data.repository.is_some() && data.files_error.is_none() {
            let ids: std::collections::BTreeSet<_> = data
                .files
                .iter()
                .enumerate()
                .filter(|(i, _)| !data.is_template(*i))
                .map(|(_, (id, _))| id.as_str())
                .collect();
            self.inventory
                .managed_marks
                .retain(|id| ids.contains(id.as_str()));
        }
        let git_default = data.repository.clone().or(self.repository.clone());
        if self.git_view.follows_files && self.git_view.root != git_default {
            self.git_view.scope_changed();
            self.git_view.root = git_default;
        }
        self.profiles.selected = self
            .selected_profile()
            .and_then(|old| {
                data.profile_rows
                    .iter()
                    .position(|row| row.name == old.name)
            })
            .unwrap_or(self.profiles.selected)
            .min(data.profile_rows.len().saturating_sub(1));
        self.data = Some(data);
        if self.tab == Tab::Files
            && self.files_page == FilesPage::Generated
            && let Some(index) = saved_template
            && !self.visible_file_rows().contains(&index)
        {
            self.inventory
                .search_mut(FilesPage::Generated)
                .query
                .clear();
        }
        self.selected = position
            .unwrap_or(self.selected)
            .min(self.row_count().saturating_sub(1));
        self.clamp_file_selection();
        self.clamp_history_selection();
    }
    fn intent(&mut self, intent: Intent) -> Option<Task> {
        if matches!(&intent, Intent::Submit(_)) {
            self.shell.form_feedback = None;
        }
        match intent {
            Intent::Unignore(path) => Some(Task::Ignore(path, false)),
            Intent::Action(action) => self.activate(action),
            Intent::Active => Some(Task::Select(None)),
            Intent::ApplyProfiles {
                manifest,
                requested,
            } => Some(Task::Profiles {
                manifest,
                requested,
                apply: true,
            }),
            Intent::Submit(Form::Check) => self.check_task(true, false),
            Intent::Submit(form @ (Form::KeepId | Form::RemoveId)) => {
                if self.shell.unmanage_id.is_empty() {
                    self.shell.form_feedback =
                        Some((form, "Enter the exact mapping ID to review.".into()));
                    self.status = "Exact mapping ID required.".into();
                    return None;
                }
                Some(Task::Files(Request::Unmanage {
                    id: self.shell.unmanage_id.clone(),
                    mode: if form == Form::KeepId {
                        UnmanageMode::Keep
                    } else {
                        UnmanageMode::Remove
                    },
                }))
            }
            Intent::Known(index) => self
                .data
                .as_ref()?
                .known_roots
                .get(index)
                .map(|root| Task::Select(Some(root.clone()))),
            Intent::Submit(Form::Adopt) => {
                let draft = &self.shell.adoption;
                if let Some(i) = [&draft.target, &draft.id, &draft.source]
                    .iter()
                    .position(|s| s.is_empty())
                {
                    self.shell.adoption.focused = i;
                    self.shell.form_feedback = Some((
                        Form::Adopt,
                        "Complete TARGET, ID and repository-relative SOURCE.".into(),
                    ));
                    self.status = "Adoption fields are incomplete; draft retained.".into();
                    return None;
                }
                // The core owns all path/kind/sensitivity/mapping policy.
                Some(Task::Files(Request::Adopt {
                    mapping: draft.mapping(),
                }))
            }
            Intent::Submit(Form::Repair) => Some(Task::Configuration(Box::new(
                self.shell.repair.as_ref()?.change(),
            ))),
            Intent::Submit(Form::Generated) => {
                use super::configuration::Request;
                self.shell.form_feedback = None;
                match self.shell.generated.as_mut()?.submit() {
                    Ok(Some(change)) => {
                        Some(Task::Configuration(Box::new(Request::Manifest(change))))
                    }
                    Ok(None) => None,
                    Err(error) => {
                        self.show_input_problem(super::actions::diagnostic(&error));
                        None
                    }
                }
            }
            Intent::Submit(Form::Configure) => match self
                .shell
                .configuration
                .as_ref()?
                .change(self.git.as_ref().ok())
            {
                Ok(change) => Some(Task::Configuration(Box::new(change))),
                Err(error) => {
                    self.show_input_problem(super::actions::diagnostic(&error));
                    None
                }
            },
            Intent::Submit(form) => {
                let draft = self.shell.draft(form);
                if draft.is_empty() {
                    self.shell.form_feedback = Some((form, "Enter an exact path.".into()));
                    self.status = "Exact path required.".into();
                    return None;
                }
                let root = crate::services::resolve_root(
                    std::path::Path::new(draft),
                    &self.launch_directory,
                );
                match form {
                    Form::Configure
                    | Form::Generated
                    | Form::Repair
                    | Form::Adopt
                    | Form::Check
                    | Form::KeepId
                    | Form::RemoveId => {
                        unreachable!()
                    }
                    Form::Choose => Some(Task::Select(Some(root))),
                    Form::Clone => match &self.git {
                        Ok(git) if !self.shell.clone_fields[0].is_empty() => Some(Task::Clone {
                            source: self.shell.clone_fields[0].clone(),
                            root,
                            git: git.clone(),
                        }),
                        Ok(_) => {
                            self.shell.form_feedback = Some((form, "Enter a Git source.".into()));
                            None
                        }
                        Err(error) => {
                            self.error = Some(error.to_string());
                            None
                        }
                    },
                    Form::Init => match &self.git {
                        Ok(git) => Some(Task::Init {
                            root,
                            git: git.clone(),
                        }),
                        Err(error) => {
                            self.details_scroll = 0;
                            self.failure = None;
                            self.failed_write = false;
                            self.error = Some(format!(
                                "{error}\nInstall Git separately, then restart Dort to resolve its executable. Draft retained for this session."
                            ));
                            None
                        }
                    },
                }
            }
        }
    }
    fn open_help(&mut self) {
        self.help = Some(super::help::State::open(self));
        super::discover_content::sync(self, self.viewport.0, self.viewport.1);
    }
    pub(super) fn result_action(&self, code: KeyCode) -> Option<Action> {
        match (&self.result_next, code) {
            (Some(ResultNext::Repair(_)), KeyCode::Char('m')) => Some(Action::RepairClone),
            (Some(ResultNext::Attach), KeyCode::Char('a')) => Some(Action::AttachRepaired),
            (Some(ResultNext::RecoverRun), KeyCode::Char('s')) => Some(Action::EnvironmentResume),
            (Some(ResultNext::RecoverRun), KeyCode::Char('x')) => Some(Action::EnvironmentAbandon),
            _ => None,
        }
    }

    pub(super) fn activate(&mut self, action: Action) -> Option<Task> {
        if action == Action::Help {
            self.open_help();
            return None;
        }
        if let Some(reason) = self.action_reason(action) {
            self.interaction_notice = Some(reason);
            self.status = reason.into();
            return None;
        }
        self.status = "".into();
        self.interaction_notice = None;
        self.form_focus = None;
        self.failed_write = false;
        self.show_result = false;
        self.shell.scroll = 0;
        self.shell.form_details = false;
        self.shell.picker_details = false;
        if matches!(self.shell.overlay, Some(Overlay::Form(Form::Adopt)))
            && matches!(action, Action::Choose | Action::Init)
        {
            self.shell.return_to_adopt = true;
        }
        self.shell.overlay = None;
        match action {
            Action::PreviewGenerated => {
                return Some(Task::PreviewGenerated(self.selected_mapping()?.id.clone()));
            }
            Action::NewGenerated | Action::EditGenerated => {
                let before = if action == Action::EditGenerated {
                    Some(self.selected_mapping()?.clone())
                } else {
                    None
                };
                self.shell.generated = Some(super::generated::Draft::new(before));
                self.shell.form_feedback = None;
                self.shell.overlay = Some(Overlay::Form(Form::Generated));
                self.form_focus = self.form_page();
            }
            Action::ProfilesPage => {
                self.go_tab(Tab::Overview);
                self.profiles.open = true;
            }
            Action::SaveProfileChoice => self.open_profile_choice(self.selected_profile()?.clone()),
            Action::Configure(kind) if kind.edits_profile() => {
                use super::configuration::{Draft, Kind, Request};
                use dort_core::manifest_edit::{Change, Profile};
                let name = self.selected_profile()?.name.clone();
                if kind == Kind::DeleteProfile {
                    return Some(Task::Configuration(Box::new(Request::Manifest(
                        Change::Profile {
                            old: Some(name),
                            new: None,
                        },
                    ))));
                }
                let profiles = self.data.as_ref()?.profile_declarations.as_ref()?;
                let profile = Profile::from_declaration(profiles, name);
                self.shell.configuration = Some(Draft::for_profile(kind, profile));
                self.shell.overlay = Some(Overlay::Form(Form::Configure));
                self.form_focus = self.form_page();
            }
            Action::Configure(kind) => {
                use super::configuration::{Draft, Kind};
                let mapping = if kind == Kind::EditFile {
                    Some(self.selected_mapping()?.clone())
                } else {
                    None
                };
                let root = self
                    .data
                    .as_ref()
                    .and_then(|data| data.repository.as_deref())
                    .or(self.repository.as_deref());
                self.shell.configuration = Some(Draft::new(kind, mapping, root));
                self.shell.overlay = Some(Overlay::Form(Form::Configure));
            }
            Action::RepairClone => {
                if let Some(ResultNext::Repair(root)) = self.result_next.take() {
                    let git = self.git.as_ref().ok()?.clone();
                    return Some(Task::Configuration(Box::new(
                        super::configuration::Request::OpenRepair(root, git),
                    )));
                }
            }
            Action::AttachRepaired => {
                if !matches!(self.result_next.take(), Some(ResultNext::Attach)) {
                    return None;
                }
                let git = self.git.as_ref().ok()?.clone();
                return Some(Task::Init {
                    root: PathBuf::from(&self.shell.init_draft),
                    git,
                });
            }
            Action::EnvironmentRecord => return Some(Task::EnvironmentRecord),
            Action::EnvironmentAbandon => return Some(Task::EnvironmentAbandon),
            Action::EnvironmentPlan | Action::EnvironmentResume => {
                self.preview = None;
                self.hits.invalidate_review();
                return Some(Task::Environment {
                    resume: action == Action::EnvironmentResume,
                });
            }
            Action::ToggleMark => super::tree::toggle(self),
            Action::ClearMarks => {
                if self.discovering() {
                    self.inventory.candidate_marks.clear();
                    self.inventory.candidate_mark_revision =
                        self.inventory.candidate_mark_revision.wrapping_add(1);
                } else {
                    self.inventory.managed_marks.clear();
                }
            }
            Action::ExpandAll | Action::CollapseAll => {
                let folds = super::visibility::Folds::all(action == Action::CollapseAll);
                let folded = if self.discovering() {
                    &mut self.inventory.candidate_folded
                } else {
                    &mut self.inventory.managed_folded
                };
                *folded = std::sync::Arc::new(folds);
                self.clamp_file_selection();
            }
            Action::BatchAdopt => {
                let Some(discovery) = &self.discovery else {
                    return None;
                };
                if let Some(editor) = self.batch_editor.as_mut() {
                    editor.sync(&self.inventory.candidate_marks, &discovery.home);
                } else {
                    self.batch_editor = Some(super::batch_ui::Editor::new(
                        &self.inventory.candidate_marks,
                        &discovery.home,
                    ));
                }
            }
            Action::ApplyMarked | Action::KeepMarked | Action::RemoveMarked => {
                let ids = self.inventory.managed_marks.iter().cloned().collect();
                return Some(Task::Batch(match action {
                    Action::ApplyMarked => dort_core::batch::Request::Apply(ids),
                    Action::KeepMarked => dort_core::batch::Request::Keep(ids),
                    _ => dort_core::batch::Request::Remove(ids),
                }));
            }
            Action::GitRoot => {
                self.go_tab(Tab::Git);

                self.git_view.editing_root = true;
            }
            Action::GitDefault => {
                self.git_view.scope_changed();
                self.git_view.root = None;
                self.git_view.follows_files = true;
                return self.git_task();
            }
            Action::GitStatus | Action::GitUnstaged | Action::GitStaged => {
                self.go_tab(Tab::Git);
                self.git_view.choose(match action {
                    Action::GitStatus => super::git::Comparison::Status,
                    Action::GitUnstaged => super::git::Comparison::Unstaged,
                    _ => super::git::Comparison::Staged,
                });
                return self.git_task();
            }
            Action::GitRefresh => return self.git_task(),
            Action::Destination(tab) => {
                if tab == Tab::Files {
                    self.go_files_page(FilesPage::Managed);
                } else {
                    self.go_tab(tab);
                }
                if tab == Tab::Tools {
                    self.tools_view.leave_section();
                }
            }
            Action::ToolDeclaration => {
                self.go_tab(Tab::Tools);
                self.tools_view.declare();
            }
            Action::ToolsConfigure => {
                let data = self.data.as_ref()?;
                let manifest = data.repository.as_ref()?.join("dort.toml");
                let tool = data
                    .tools
                    .as_ref()?
                    .as_ref()
                    .ok()?
                    .get(self.tools_view.declared_selected)?
                    .clone();
                self.go_tab(Tab::Tools);
                return self.tools_view.configure(manifest, tool);
            }
            Action::ToolsInstalled => {
                self.go_tab(Tab::Tools);
                return self.tools_view.open_installed();
            }
            Action::ToolsStandaloneCheck => {
                self.go_tab(Tab::Tools);
                self.tools_view.open_manifest();
            }
            Action::EnvironmentCheck => {
                let resume = self.preview.take().is_some_and(|review| matches!(review.pending, Pending::Environment(plan) if plan.is_resume()));
                self.hits.invalidate_review();
                return Some(Task::EnvironmentCheck {
                    detection: self.tools_view.inspection_inputs(),
                    resume,
                });
            }
            Action::ToolsCheck => {
                self.go_tab(Tab::Tools);
                let details = self.tools_view.declared_details;
                self.tools_view.leave_section();
                self.tools_view.declared_details = details;
                self.tools_view.declared.check_waiting = true;
                return Some(Task::ToolsCheckPrepare(self.tools_view.inspection_inputs()));
            }
            Action::ToolsInstallMissing => {
                self.go_tab(Tab::Tools);
                let checked = self.tools_view.declared.checked.clone()?;
                let details = self.tools_view.declared_details;
                self.tools_view.leave_section();
                self.tools_view.declared_details = details;
                self.tools_view.declared.install_waiting = true;
                return Some(Task::ToolsInstallMissingPrepare(checked));
            }
            Action::ToolAttempts => {
                self.go_tab(Tab::Tools);
                return Some(self.tools_view.open_attempts());
            }
            Action::Choose => self.shell.overlay = Some(Overlay::Repositories { selected: 0 }),
            Action::Clone => {
                self.shell.path_focused = 0;
                self.shell.overlay = Some(Overlay::Form(Form::Clone));
            }
            Action::Init => {
                self.shell.path_focused = 0;
                self.shell.overlay = Some(Overlay::Form(Form::Init));
            }
            Action::ManageIgnored => return Some(Task::IgnoredPaths),
            Action::IgnoreSelected => {
                if let Some(node) = super::tree::selected(self) {
                    return Some(Task::Ignore(node.path, true));
                }
            }
            Action::Discover => {
                self.go_files_page(FilesPage::Discover);
                return Some(Task::Discover);
            }
            Action::Adopt => {
                self.tab = Tab::Files;
                self.shell.overlay = Some(Overlay::Form(Form::Adopt));
            }
            Action::AdoptSelected => {
                if self.tab == Tab::Files && self.discovering() {
                    self.adopt_candidate();
                } else {
                    self.status =
                        "Open Files > Discover and select an eligible regular file.".into();
                }
            }
            Action::KeepId | Action::RemoveId => {
                self.go_files_page(FilesPage::Managed);
                self.shell.unmanage_focused = 0;
                self.shell.overlay = Some(Overlay::Form(if matches!(action, Action::KeepId) {
                    Form::KeepId
                } else {
                    Form::RemoveId
                }));
            }
            Action::ClearFilter if self.tab == Tab::Recovery => {
                self.history_search.query.clear();
                self.clamp_history_selection();
                self.status =
                    "Recovery display filter cleared; global blockers remain visible.".into();
            }
            Action::ClearFilter if self.tab == Tab::Git => self.git_view.clear_filter(),
            Action::ClearFilter => {
                self.inventory.search_mut(self.files_page).query.clear();
                self.clamp_file_selection();
                self.status = if self.files_page == FilesPage::Managed {
                    "File display filter cleared; Apply uses the full active scope."
                } else {
                    "File display filter cleared."
                }
                .into();
            }
            Action::Inspect if self.tab == Tab::Git => self.git_view.inspect_metadata(),
            Action::Inspect if self.tab == Tab::Recovery => {
                self.history_region = Region::Details;
                self.details_scroll = 0;
            }
            Action::Inspect if self.tab == Tab::Files => {
                self.inventory.details = true;
                self.details_scroll = 0;
            }
            Action::Profiles => return self.check_task(false, true),
            Action::CheckCurrent => return self.check_task(false, false),
            Action::CheckStandalone => {
                self.tab = Tab::Checks;
                self.shell.overlay = Some(Overlay::Form(Form::Check));
            }
            Action::StandaloneProfiles => {
                self.shell.overlay = Some(Overlay::Form(Form::Check));
                return self.check_task(true, true);
            }
            Action::Refresh if self.tab == Tab::Git => return self.git_task(),
            Action::Refresh => return Some(self.refresh_task()),
            Action::PreviewOnly => {
                if self.preview_only_locked {
                    self.status = "--dry-run locks this session to preview-only.".into();
                } else {
                    self.dry_run = !self.dry_run;
                    self.status =
                        "Preview-only mode changed; approval always requires a fresh preview."
                            .into();
                }
            }
            Action::Diagnostics => {
                self.go_tab(Tab::Checks);
                self.showing_diagnostics = true;
                return Some(Task::Diagnostics);
            }
            Action::About => self.shell.overlay = Some(Overlay::About),
            Action::Help => unreachable!("Help preserves the underlying surface"),
            Action::Apply
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.repository.as_ref())
                    .is_some() =>
            {
                return Some(Task::Files(Request::Apply));
            }
            Action::Keep | Action::Remove
                if self.tab == Tab::Files && self.files_page == FilesPage::Managed =>
            {
                if !self.visible_file_rows().contains(&self.selected) {
                    self.status =
                        "No displayed mapping selected. Use Enter exact mapping ID in Actions."
                            .into();
                    return None;
                }
                if let Some((id, _)) = self
                    .data
                    .as_ref()
                    .and_then(|data| data.files.get(self.selected))
                {
                    return Some(Task::Files(Request::Unmanage {
                        id: id.clone(),
                        mode: if matches!(action, Action::Keep) {
                            UnmanageMode::Keep
                        } else {
                            UnmanageMode::Remove
                        },
                    }));
                }
                self.status = "Choose a managed mapping in Files first.".into();
            }
            Action::Rollback | Action::Resume | Action::Abandon if self.tab == Tab::Recovery => {
                if let Some(row) = self.history_selection() {
                    let eligible = match action {
                        Action::Rollback => super::recovery::rollback(row),
                        Action::Resume => super::recovery::resume(row),
                        _ => super::recovery::abandon(row),
                    };
                    if !eligible {
                        self.history_region = Region::Details;
                        self.status = "Action unavailable for this diagnosis. Inspect details and request an eligible recovery preview. Refresh if history changed.".into();
                        return None;
                    }
                    let (r, t) = (row.repository_id.clone(), row.transaction_id.clone());
                    return Some(match action {
                        Action::Rollback => Task::Rollback(r, t),
                        Action::Resume => Task::Bootstrap(r, t, RecoveryAction::Resume),
                        _ => Task::Bootstrap(r, t, RecoveryAction::Abandon),
                    });
                }
                self.status = "Choose an operation in Recovery first.".into();
            }
            _ => {
                self.status =
                    "Action unavailable. Choose a repository, file or operation as labeled.".into()
            }
        }
        self.clamp_file_selection();
        self.clamp_history_selection();
        None
    }
    pub(super) fn key_text_entry(&self) -> bool {
        self.help.is_none()
            && self.more.is_none()
            && self.choice.is_none()
            && !self.busy
            && self.error.is_none()
            && !self.show_result
            && self.preview.is_none()
            && self.text_entry()
    }
    /// Stable identity of the foreground form, independent of its selected field.
    pub(super) fn paste_generated(&mut self, text: &str, width: u16, height: u16) {
        self.resize(width, height);
        if width >= 48
            && height >= 14
            && self.form_page() == Some("generated file")
            && self.form_has_focus()
            && let Some(draft) = &mut self.shell.generated
        {
            self.shell.form_feedback = draft
                .paste(text)
                .err()
                .map(|error| (Form::Generated, error.to_string()));
        }
    }

    pub(super) fn form_page(&self) -> Option<&'static str> {
        if self.help.is_some()
            || self.more.is_some()
            || self.choice.is_some()
            || self.busy
            || self.error.is_some()
            || self.show_result
            || self.preview.is_some()
        {
            return None;
        }
        if let Some(Overlay::Form(form)) = self.shell.overlay {
            if self.shell.form_details {
                return None;
            }
            return Some(match form {
                Form::Adopt => "adopt",
                Form::Check => "check",
                Form::Choose => "choose",
                Form::Init => "init",
                Form::Clone => "clone",
                Form::Configure => "configuration",
                Form::Repair => "manifest repair",
                Form::Generated => "generated file",
                Form::KeepId => "keep",
                Form::RemoveId => "remove",
            });
        }
        if self
            .batch_editor
            .as_ref()
            .is_some_and(|e| e.open && e.item_form && !e.details)
        {
            return Some("batch item");
        }
        if self.tab == Tab::Git && self.git_view.editing_root && self.git_view.text_entry() {
            return Some("git scope");
        }
        if self.tab != Tab::Tools {
            return None;
        }
        let tools = &self.tools_view;
        if tools.install_open
            && tools.installation.text_entry()
            && matches!(
                tools.installation.mode,
                super::tools_install::Mode::CloseInputs
            )
        {
            return Some("installation");
        }
        if tools.declaration_open
            && tools.declaration.change != Some(dort_core::tools::declaration::Change::Remove)
        {
            return Some("declaration");
        }
        if tools.settings_open {
            return Some("tool settings");
        }
        if tools.manifest_open {
            return Some("tool check manifest");
        }
        None
    }
    pub(super) fn form_has_focus(&self) -> bool {
        self.form_page()
            .is_none_or(|owner| owner == "generated file" || self.form_focus == Some(owner))
    }
    fn focus_first_field(&mut self) {
        self.shell.scroll = 0;
        match self.form_page() {
            Some("choose" | "init" | "clone") => self.shell.path_focused = 0,
            Some("check") => self.shell.manifest_focused = 0,
            Some("keep" | "remove") => self.shell.unmanage_focused = 0,
            Some("adopt") => self.shell.adoption.focused = 0,
            Some("batch item") => {
                if let Some(editor) = &mut self.batch_editor {
                    editor.focused = editor.item * super::shell::AdoptionDraft::FIELDS;
                }
            }
            Some("declaration") => {
                self.tools_view.declaration.focus = self
                    .tools_view
                    .declaration
                    .controls()
                    .first()
                    .copied()
                    .unwrap_or(1)
            }
            Some("installation") => self.tools_view.installation.focus = 0,
            Some("tool settings") => self.tools_view.settings.focus = 0,
            _ => (),
        }
    }
    pub fn text_entry(&self) -> bool {
        self.form_has_focus()
            && (self
                .batch_editor
                .as_ref()
                .is_some_and(|e| e.open && e.item_form && !e.details)
                || self.shell.text_entry()
                || (self.tab == Tab::Git && self.git_view.text_entry())
                || (self.tab == Tab::Tools && self.tools_view.text_entry())
                || (self.tab == Tab::Recovery && self.history_search.editing)
                || (self.shell.overlay.is_none()
                    && self.tab == Tab::Files
                    && self.inventory.search(self.files_page).editing))
    }
    pub fn discovering(&self) -> bool {
        self.files_page == FilesPage::Discover
    }
    pub(super) fn go_files_page(&mut self, page: FilesPage) {
        let changed = self.tab != Tab::Files || self.files_page != page;
        self.go_tab(Tab::Files);
        if changed {
            self.files_page = page;
            self.inventory.group = None;
            self.inventory.details = false;
            self.inventory.mark_refusal = None;
            self.content = Default::default();
            self.details_scroll = 0;
        }
        self.clamp_file_selection();
    }
    pub fn file_selection(&self) -> usize {
        self.inventory.group.unwrap_or(match self.files_page {
            FilesPage::Managed => self.selected,
            FilesPage::Discover => self.candidate_selected,
            FilesPage::Generated => self.inventory.generated_selected,
        })
    }
    pub fn file_rows_empty(&self) -> bool {
        if self.discovering() {
            super::discovery_tree::snapshot(self).matching.is_empty()
        } else {
            self.visible_file_rows().is_empty()
        }
    }
    pub fn visible_file_rows(&self) -> Vec<usize> {
        let search = self.inventory.search(self.files_page);
        if self.discovering() {
            super::discovery_tree::snapshot(self).matching.to_vec()
        } else {
            self.data
                .as_ref()
                .map(|d| {
                    d.files
                        .iter()
                        .enumerate()
                        .filter(|(i, row)| {
                            let generated = self.files_page == FilesPage::Generated;
                            d.is_template(*i) == generated
                                && (search.matches(&row.0)
                                    || if generated {
                                        d.managed.get(*i).is_some_and(|file| {
                                            search.matches(&file.target.to_string_lossy())
                                                || file.source.as_ref().is_some_and(|path| {
                                                    search.matches(&path.to_string_lossy())
                                                })
                                        })
                                    } else {
                                        search.matches(&row.1)
                                    })
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default()
        }
    }
    fn clamp_file_selection(&mut self) {
        if self.tab != Tab::Files {
            return;
        }
        let visible = super::tree::visible(self);
        let old = self.file_selection();
        let next = if super::tree::position(self, old).is_some() {
            Some(old)
        } else {
            visible.first().map(|n| n.key)
        };
        self.inventory.group = None;
        if let Some(key) = next {
            super::tree::select(self, key);
        }
    }
    fn move_file_selection(&mut self, backwards: bool) {
        self.move_files(backwards, 1);
    }
    fn move_files(&mut self, backwards: bool, count: usize) {
        self.details_scroll = 0;
        let visible = super::tree::visible(self);
        let position = super::tree::position(self, self.file_selection()).unwrap_or(0);
        let next = if backwards {
            position.saturating_sub(count)
        } else {
            position
                .saturating_add(count)
                .min(visible.len().saturating_sub(1))
        };
        if let Some(node) = visible.get(next) {
            super::tree::select(self, node.key);
        }
    }
    pub fn visible_history_rows(&self) -> Vec<usize> {
        self.data
            .as_ref()
            .filter(|d| d.history_error.is_none())
            .map(|d| {
                d.history
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| super::recovery::matches(&self.history_search, row))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn history_selection(&self) -> Option<&dort_core::journal::Diagnosis> {
        if !self.visible_history_rows().contains(&self.selected) {
            return None;
        }
        self.data.as_ref()?.history.get(self.selected)
    }
    fn clamp_history_selection(&mut self) {
        if self.tab != Tab::Recovery {
            return;
        }
        let visible = self.visible_history_rows();
        if !visible.contains(&self.selected)
            && let Some(index) = visible.iter().min_by_key(|&&i| i.abs_diff(self.selected))
        {
            self.selected = *index;
        }
    }
    fn move_history_selection(&mut self, backwards: bool) {
        self.page_history(backwards, 1);
    }
    fn page_history(&mut self, backwards: bool, count: usize) {
        let visible = self.visible_history_rows();
        let position = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if backwards {
            position.saturating_sub(count)
        } else {
            position
                .saturating_add(count)
                .min(visible.len().saturating_sub(1))
        };
        if let Some(&index) = visible.get(next)
            && self.selected != index
        {
            self.selected = index;
            self.details_scroll = 0;
        }
    }
    pub(super) fn can_switch_scope(&self) -> bool {
        self.data
            .as_ref()
            .is_some_and(|data| data.overview.is_some())
            && self.dialog_title().is_none()
            && !self.busy
            && self.error.is_none()
            && self.preview.is_none()
            && !self.show_result
            && self.shell.overlay.is_none()
            && !self.text_entry()
            && self.batch_editor.as_ref().is_none_or(|editor| !editor.open)
            && (self.tab != Tab::Tools
                || self.tools_view.workspace_visible() && !self.tools_view.installed_open)
            && (self.tab != Tab::Recovery || self.history_region == Region::Queue)
            && (self.tab != Tab::Checks
                || self
                    .check
                    .as_ref()
                    .is_none_or(|(_, standalone)| !standalone))
            && (self.tab != Tab::Git || self.git_view.follows_files && !self.git_view.editing_root)
    }

    pub(super) fn environment_review_open(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|review| matches!(review.pending, Pending::Environment(_)))
    }

    pub(super) fn environment_can_check(&self) -> bool {
        self.preview.as_ref().is_some_and(
            |review| matches!(&review.pending, Pending::Environment(plan) if plan.has_tools()),
        )
    }

    fn reset_repository_scope(&mut self) {
        self.manifest_tool_results.clear();
        self.profiles = Default::default();
        self.tools_view.declared.invalidate();
        self.git_view.discard_preview();
        if self.git_view.follows_files {
            self.git_view.scope_changed();
            self.git_view.root = None;
        }
        self.inventory.group = None;
        self.inventory.managed_marks.clear();
        self.inventory.generated_selected = 0;
        self.inventory.saved_template = None;
        // Adoption paths/drafts are retained; the new root is freshly core-validated.
        if let Some(editor) = &mut self.batch_editor {
            editor.open = false;
        }
        self.requested_profiles.clear();
        if self
            .check
            .as_ref()
            .is_some_and(|(_, standalone)| !standalone)
        {
            self.check = None;
        }
        self.preview = None;
    }
    fn check_task(&mut self, standalone: bool, profiles: bool) -> Option<Task> {
        let manifest = if standalone {
            if self.shell.manifest_draft.is_empty() {
                self.shell.overlay = Some(Overlay::Form(Form::Check));
                self.shell.form_feedback =
                    Some((Form::Check, "Enter the standalone manifest path.".into()));
                self.status = "Manifest path required; draft retained.".into();
                return None;
            }
            Some(crate::services::resolve_root(
                std::path::Path::new(&self.shell.manifest_draft),
                &self.launch_directory,
            ))
        } else {
            None
        };
        let requested = if standalone {
            self.shell.check_profiles.clone()
        } else {
            self.requested_profiles.clone()
        };
        Some(if profiles {
            Task::Profiles {
                manifest,
                requested,
                apply: false,
            }
        } else {
            Task::Check {
                manifest,
                requested,
            }
        })
    }
    pub(super) fn sync_git_preview(&mut self) {
        self.git_view.sync_preview(
            self.tab == Tab::Git
                && self.git.is_ok()
                && !self.busy
                && !self.quit
                && !self.exit_requested
                && self.error.is_none()
                && self.shell.overlay.is_none()
                && !self.show_result
                && self.preview.is_none()
                && self.viewport.0 >= 48
                && self.viewport.1 >= 14,
        );
    }
    pub(super) fn next_git_preview(&mut self) -> Option<Task> {
        if self.help.is_some() || self.choice.is_some() || self.more.is_some() {
            return None;
        }
        self.git_view.next_preview(self.git.as_ref().ok()?.clone())
    }
    fn git_task(&mut self) -> Option<Task> {
        self.go_tab(Tab::Git);
        let root = if self.git_view.follows_files {
            self.data
                .as_ref()
                .and_then(|d| d.repository.clone())
                .or(self.repository.clone())
        } else {
            self.git_view.root.clone()
        };
        let Some(root) = root else {
            self.git_view.editing_root = true;
            self.status = "Choose an independent Git root for this session.".into();
            return None;
        };
        match &self.git {
            Ok(executable) => Some(self.git_view.request(root, executable.clone())),
            Err(error) => {
                self.failure = None;
                self.failed_write = false;
                self.error = Some(format!(
                    "{error}\nInstall Git separately and restart Dort. Git root draft retained."
                ));
                None
            }
        }
    }
    fn refresh_task(&self) -> Task {
        if self.tab == Tab::Files && self.discovering() {
            Task::Discover
        } else if self.tab == Tab::Recovery {
            Task::History
        } else if self.tab == Tab::Checks && self.showing_diagnostics {
            Task::Diagnostics
        } else if self.tab == Tab::Checks {
            if let Some((report, standalone)) = &self.check {
                Task::Check {
                    manifest: standalone.then(|| report.manifest.clone()),
                    requested: if *standalone {
                        self.shell.check_profiles.clone()
                    } else {
                        self.requested_profiles.clone()
                    },
                }
            } else {
                Task::Refresh
            }
        } else {
            Task::Refresh
        }
    }
    fn adopt_candidate(&mut self) {
        if !super::discovery_tree::contains(self, self.candidate_selected) {
            return;
        }
        if let Some(candidate) = self
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(self.candidate_selected))
        {
            if !candidate.supported_for_adoption {
                self.status = "Not eligible for adoption. Only owned regular files with one link can be added.".into();
                return;
            }
            let home = &self.discovery.as_ref().expect("selected discovery").home;
            self.shell.adoption.suggest(&candidate.entry.path, home);
            self.shell.scroll = 0;
            self.shell.overlay = Some(Overlay::Form(Form::Adopt));
        }
    }
    pub fn action_reason(&self, action: Action) -> Option<&'static str> {
        match action {
            Action::ToolsInstallMissing if self.tools_view.declared.stale => {
                Some("Check installed again before preparing another install review.")
            }
            Action::ToolsInstallMissing
                if self
                    .tools_view
                    .declared
                    .checked
                    .as_ref()
                    .is_some_and(|c| c.missing() == 0) =>
            {
                Some(
                    "No missing packages were observed. See tool details for unknown results or version differences.",
                )
            }
            Action::ToolsInstallMissing if self.tools_view.declared.checked.is_none() => {
                Some("Check installed first to find missing tools.")
            }
            Action::ToolsCheck
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.tools.as_ref())
                    .is_some_and(|t| t.as_ref().is_ok_and(Vec::is_empty)) =>
            {
                Some("Add a tool declaration before checking installed tools.")
            }
            Action::ToolsCheck
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.repository.as_ref())
                    .or(self.repository.as_ref())
                    .is_none() =>
            {
                Some("Choose a repository to check its declared tools.")
            }
            Action::Configure(
                super::configuration::Kind::CreateProfile
                | super::configuration::Kind::EditProfileDefaults
                | super::configuration::Kind::RenameProfile
                | super::configuration::Kind::DeleteProfile
                | super::configuration::Kind::File
                | super::configuration::Kind::EditFile,
            )
            | Action::NewGenerated
            | Action::EditGenerated
            | Action::EnvironmentPlan
            | Action::EnvironmentCheck
            | Action::EnvironmentResume
            | Action::EnvironmentRecord
            | Action::EnvironmentAbandon
            | Action::ProfilesPage
                if !self.registered_repository() =>
            {
                Some("Choose a registered repository.")
            }
            Action::Configure(kind)
                if (kind.edits_profile()
                    || kind == super::configuration::Kind::CreateProfile
                    || kind == super::configuration::Kind::File)
                    && self
                        .data
                        .as_ref()
                        .is_none_or(|data| data.profile_declarations.is_none()) =>
            {
                Some("Choose a repository with a readable manifest.")
            }
            Action::Configure(kind) if kind.edits_profile() => self.profile_action_reason(),
            Action::SaveProfileChoice => self.profile_action_reason(),
            Action::NewGenerated | Action::EditGenerated | Action::PreviewGenerated
                if self.tab != Tab::Files || self.files_page != FilesPage::Generated =>
            {
                Some("Open Files / Generated.")
            }
            Action::NewGenerated
                if self
                    .data
                    .as_ref()
                    .is_none_or(|data| data.profile_declarations.is_none()) =>
            {
                Some("Choose a repository with a readable manifest.")
            }
            Action::EditGenerated
                if !self.selected_mapping().is_some_and(|file| {
                    file.mode == dort_core::model::FileMode::Template && file.template.is_some()
                }) =>
            {
                Some("Select a generated file with a template in dort.toml.")
            }
            Action::PreviewGenerated if !self.template_selected() => {
                Some("Select a generated file to preview.")
            }
            Action::Configure(super::configuration::Kind::EditFile)
                if self.files_page != FilesPage::Managed || self.selected_mapping().is_none() =>
            {
                Some("Select a displayed file mapping first.")
            }
            Action::CheckCurrent | Action::Profiles
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.repository.as_ref())
                    .or(self.repository.as_ref())
                    .is_none() =>
            {
                Some(
                    "Choose a repository first; standalone manifest checks have an independent path and profiles.",
                )
            }
            Action::IgnoreSelected
                if self.tab != Tab::Files
                    || !self.discovering()
                    || super::tree::selected(self).is_none() =>
            {
                Some(
                    "Select a displayed discovery file or folder after the list finishes updating.",
                )
            }
            Action::ToggleMark | Action::ClearMarks | Action::ExpandAll | Action::CollapseAll
                if self.tab != Tab::Files || self.files_page == FilesPage::Generated =>
            {
                Some("Marks and folder controls are available in Managed and Discover.")
            }
            Action::BatchAdopt
                if self.tab != Tab::Files
                    || !self.discovering()
                    || self.inventory.candidate_marks.is_empty() =>
            {
                Some("Mark regular files with Space before starting a batch.")
            }
            Action::BatchAdopt
                if self.inventory.candidate_marks.len() > dort_core::batch::MAX_FILES =>
            {
                Some("A batch can hold 32 files. Clear some marks.")
            }
            Action::ApplyMarked | Action::KeepMarked | Action::RemoveMarked
                if self.tab != Tab::Files
                    || self.files_page != FilesPage::Managed
                    || self.inventory.managed_marks.is_empty() =>
            {
                Some(
                    "Mark managed files with Space. A/U/D review the batch; Apply covers every active file.",
                )
            }
            Action::Keep | Action::Remove | Action::AdoptSelected
                if self.inventory.group.is_some() =>
            {
                Some(
                    "Not eligible for adoption. Select a regular file. Folders can only be marked.",
                )
            }
            Action::Apply
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.repository.as_ref())
                    .is_none() =>
            {
                Some("Choose a repository first.")
            }
            Action::Keep | Action::Remove
                if self.tab != Tab::Files
                    || self.files_page != FilesPage::Managed
                    || !self.visible_file_rows().contains(&self.selected) =>
            {
                Some("Select a managed file, or use the exact-ID form.")
            }
            Action::Inspect if self.tab == Tab::Files && self.file_rows_empty() => {
                Some("Select a file first, or clear the filter.")
            }
            Action::Inspect if self.tab == Tab::Recovery && self.history_selection().is_none() => {
                Some("Select an operation record, or clear the filter.")
            }
            Action::Inspect if self.tab == Tab::Git && !self.git_view.has_selection() => {
                Some("Select a Git entry, or clear the filter.")
            }
            Action::Inspect if !matches!(self.tab, Tab::Files | Tab::Recovery | Tab::Git) => {
                Some("Select a file or operation record to inspect.")
            }
            Action::ClearFilter if !matches!(self.tab, Tab::Files | Tab::Recovery | Tab::Git) => {
                Some("Filters are available in Files, Recovery and Git.")
            }
            Action::PreviewOnly if self.preview_only_locked => {
                Some("--dry-run keeps this session preview-only. Approval is off.")
            }
            Action::AdoptSelected if self.tab != Tab::Files || !self.discovering() => {
                Some("Select an eligible regular file first.")
            }
            Action::AdoptSelected
                if !super::discovery_tree::contains(self, self.candidate_selected) =>
            {
                Some("Pick a file from the current list.")
            }
            Action::AdoptSelected => {
                let Some(candidate) = self
                    .discovery
                    .as_ref()
                    .and_then(|d| d.report.candidates.get(self.candidate_selected))
                else {
                    return Some("Pick a file from the current list.");
                };
                match self.managed_destination(&candidate.entry.path) {
                    Some(row) if row.mode == dort_core::model::FileMode::Template => {
                        Some("Already declared. Open Files / Generated for this destination.")
                    }
                    Some(_) => {
                        Some("Already managed. Change the existing entry in Managed instead.")
                    }
                    None if !candidate.supported_for_adoption => Some(
                        "Not eligible for adoption. Only owned regular files with one link can be added.",
                    ),
                    None => None,
                }
            }

            Action::Rollback
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::rollback(r)) =>
            {
                Some("Rollback is unavailable. Select a completed file or declaration operation.")
            }
            Action::Resume
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::resume(r)) =>
            {
                Some("Resume is unavailable. Select an intact repository setup.")
            }
            Action::Abandon
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::abandon(r)) =>
            {
                Some("Abandon is unavailable. Select an unfinished repository setup.")
            }
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn page_actions(&self) -> Vec<Action> {
        super::context::page_actions(self)
    }
    fn row_count(&self) -> usize {
        self.data.as_ref().map_or(0, |data| match self.tab {
            Tab::Files => data.files.len(),
            Tab::Recovery => data.history.len(),
            _ => 0,
        })
    }
    pub(super) fn file_review_needs_environment(&self) -> bool {
        self.tab == Tab::Files
            && self.registered_repository()
            && matches!(self.preview.as_ref().map(|p| &p.pending), Some(Pending::Files(plan))
                if plan.has_blockers() && plan.inputs.files.iter().any(|file| !file.mapping.requires.is_empty()))
    }
    pub fn batch_removal_ids(&self) -> Vec<String> {
        match self.preview.as_ref().map(|p| &p.pending) {
            Some(Pending::Batch(plan))
                if plan.inputs.action == dort_core::batch::Action::Remove =>
            {
                plan.inputs
                    .items
                    .iter()
                    .flat_map(|s| s.files.iter().map(|f| f.mapping.id.clone()))
                    .collect()
            }
            _ => vec![],
        }
    }
    pub(super) fn preview_approval(&self) -> Approval {
        Approval::review(
            &self.hits,
            ReviewStage::Changes,
            self.dry_run,
            self.preview.as_ref().is_none_or(|p| p.pending.blocked()),
        )
    }
    fn approve_preview(&mut self) -> Option<Task> {
        let removal_ids = self.batch_removal_ids();
        let pending = self.preview.take()?.pending;
        let adopting = pending.adoption();
        let approval = match pending {
            Pending::Batch(plan) if plan.inputs.action == dort_core::batch::Action::Remove => {
                dort_core::batch::Approval::from_removal_plan(
                    *plan,
                    dort_core::batch::RemovalAcknowledgements::confirm(removal_ids),
                )
                .map(|approval| super::actions::Approved::Batch(Box::new(approval)))
            }
            pending => {
                let id = pending.removal_id().map(str::to_owned);
                pending.approve(id.as_deref())
            }
        };
        match approval {
            Ok(approval) => {
                self.return_to_discover = adopting;
                Some(Task::Execute(approval))
            }
            Err(error) => {
                self.error = Some(format!(
                    "Approval refused: {}",
                    super::actions::diagnostic(&error)
                ));
                self.retry = None;
                self.failure = None;
                self.failed_write = false;
                self.status = "Approval refused; a fresh review is required.".into();
                None
            }
        }
    }
}
