use super::{
    actions::{Data, Output, Pending, Task},
    shell::{Action, Form, Intent, Overlay, ProfilePicker, Shell},
};
use dort_core::{
    bootstrap::RecoveryAction,
    planner::{Request, UnmanageMode},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Files,
    Recovery,
    Checks,
    Git,
}
impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Files,
            Self::Files => Self::Recovery,
            Self::Recovery => Self::Checks,
            Self::Checks => Self::Git,
            Self::Git => Self::Overview,
        }
    }
    pub fn previous(self) -> Self {
        self.next().next().next().next()
    }
}
pub struct Preview {
    pub pending: Pending,
    pub text: String,
    pub scroll: u16,
    pub horizontal: u16,
    pub full: bool,
    pub selected: usize,
    pub effect_scroll: u16,
}
impl Preview {
    fn clamp_scroll(&mut self, width: u16, height: u16) {
        // Header, navigation, status, footer, approval row and preview borders.
        // Escaped report bytes are ASCII, so byte lengths equal terminal columns.
        let escaped = super::views::safe(&self.text);
        let lines = escaped.lines().count();
        let columns = escaped.lines().map(str::len).max().unwrap_or(0);
        self.scroll = self.scroll.min(
            lines
                .saturating_sub(usize::from(height.saturating_sub(7)))
                .min(u16::MAX as usize) as u16,
        );
        self.horizontal = self.horizontal.min(
            columns
                .saturating_sub(usize::from(width.saturating_sub(2)))
                .min(u16::MAX as usize) as u16,
        );
    }
}
pub struct Confirmation {
    pub prompt_scroll: u16,
    pub input: String,
    pub removal_ack: Option<String>,
    pub batch_acks: Vec<String>,
}
struct RowClick {
    at: std::time::Instant,
    key: usize,
    discovering: bool,
    size: (u16, u16),
    row: u16,
}
pub struct App {
    pub appearance: super::appearance::Appearance,
    pub tab: Tab,
    pub shell: Shell,
    pub help: Option<super::help::State>,
    pub navigation: super::navigation::State,
    pub repository: Option<PathBuf>,
    pub origin: &'static str,
    pub launch_directory: PathBuf,
    pub git: Result<PathBuf, String>,
    pub git_view: super::git::State,
    pub result: Option<String>,
    pub receipt: Option<super::task_surfaces::Receipt>,
    pub show_result: bool,
    pub activity: Option<super::task_surfaces::Activity>,
    pub reviewed_activity: Option<super::task_surfaces::Activity>,
    pub failure: Option<super::task_surfaces::Failure>,
    pub write_warning: bool,
    pub data: Option<Data>,
    pub content: super::discover_content::State,
    return_to_discover: bool,
    rediscovering_after_adoption: bool,
    pub discovering: bool,
    pub inventory: super::inventory::Inventory,
    pub batch_editor: Option<super::batch_ui::Editor>,
    pub discovery: Option<std::sync::Arc<crate::services::Discovery>>,
    pub candidate_selected: usize,
    pub requested_profiles: Vec<String>,
    pub check: Option<(Box<crate::services::CheckReport>, bool)>,
    pub diagnostics: Option<super::actions::Diagnostics>,
    pub showing_diagnostics: bool,
    pub history_search: super::inventory::Search,
    pub history_details: bool,
    pub retry: Option<super::actions::Retry>,
    pub selected: usize,
    pub hits: super::input::Hits,
    last_click: Option<RowClick>,
    pub details_scroll: u16,
    viewport: (u16, u16),
    pub preview: Option<Preview>,
    pub confirmation: Option<Confirmation>,
    pub busy: bool,
    pub writing: bool,
    pub exit_requested: bool,
    pub quit: bool,
    pub dry_run: bool,
    preview_only_locked: bool,
    pub status: String,
    pub error: Option<String>,
    pub failed_write: bool,
}
impl App {
    pub fn new(dry_run: bool) -> Self {
        Self {
            appearance: Default::default(),
            tab: Tab::Overview,
            shell: Shell::new(),
            help: None,
            navigation: Default::default(),
            repository: None,
            origin: "registered active",
            launch_directory: PathBuf::from("/"),
            git: Err("Git unavailable; install Git separately and restart Dort".into()),
            git_view: super::git::State::new(),
            result: None,
            receipt: None,
            show_result: false,
            activity: None,
            reviewed_activity: None,
            failure: None,
            write_warning: false,
            data: None,
            content: Default::default(),
            return_to_discover: false,
            rediscovering_after_adoption: false,
            discovering: false,
            inventory: Default::default(),
            batch_editor: None,
            discovery: None,
            candidate_selected: 0,
            requested_profiles: vec![],
            check: None,
            diagnostics: None,
            showing_diagnostics: false,
            history_search: Default::default(),
            history_details: false,
            retry: None,
            selected: 0,
            hits: Default::default(),
            last_click: None,
            details_scroll: 0,
            viewport: (48, 14),
            preview: None,
            confirmation: None,
            busy: false,
            writing: false,
            exit_requested: false,
            quit: false,
            dry_run,
            preview_only_locked: dry_run,
            status: "Loading read-only observations...".into(),
            error: None,
            failed_write: false,
        }
    }
    pub fn start(&mut self, writing: bool) {
        self.navigation.menu = None;
        self.last_click = None;
        self.inventory.pending_g = false;
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
    pub fn complete(&mut self, result: Result<Output, String>) -> Option<Task> {
        if let Ok(Output::DiscoverContent(report)) = result {
            self.content.accept(report);
            return None;
        }
        self.last_click = None;
        if let Ok(Output::DiscoveryTree(report)) = result {
            if super::discovery_tree::accept(self, report) {
                self.clamp_file_selection();
            }
            return None;
        }
        let writing = self.writing;
        let activity = self.activity.take();
        self.failed_write = writing && result.is_err();
        self.busy = false;
        self.writing = false;
        if self.exit_requested {
            self.quit = true;
        }
        if result.is_ok() {
            self.retry = None;
        }
        match result {
            Err(error) => {
                self.return_to_discover = false;
                self.rediscovering_after_adoption = false;
                self.failure = Some(super::task_surfaces::Failure {
                    cause: error.clone(),
                    activity,
                });
                self.write_warning |= writing;
                self.show_result = false;
                self.git_view.failed();
                self.details_scroll = 0;
                self.status = if writing {
                    format!(
                        "Error: {error}\nA failed write may leave partial effects. Inspect Recovery; nothing is automatically undone."
                    )
                } else {
                    format!("Read failed: {error}\nEsc returns to retained input and scope.")
                };
                self.preview = None;
                self.confirmation = None;
                if writing {
                    self.retry = None;
                }
                self.error = Some(self.status.clone());
            }
            Ok(Output::DiscoverContent(_) | Output::DiscoveryTree(_)) => unreachable!(),
            Ok(Output::History { rows, error }) => {
                let identity = self
                    .data
                    .as_ref()
                    .and_then(|d| d.history.get(self.selected))
                    .map(|r| (r.repository_id.clone(), r.transaction_id.clone()));
                let data = self.data.get_or_insert_with(Default::default);
                data.history = rows;
                data.history_error = error;
                let position = identity.as_ref().and_then(|(r, t)| {
                    data.history
                        .iter()
                        .position(|row| &row.repository_id == r && &row.transaction_id == t)
                });
                if data.history_error.is_some() {
                    self.status = "Recovery diagnosis unavailable; global write safety unknown. r retries read-only diagnosis.".into();
                } else if identity.is_some() && position.is_none() {
                    self.status =
                        "Selected transaction disappeared; showing nearest remaining row.".into();
                } else {
                    self.status = "All-repository diagnosis refreshed; live drift is assessed by each recovery preview.".into();
                }
                self.selected = position
                    .unwrap_or(self.selected)
                    .min(data.history.len().saturating_sub(1));
                self.clamp_history_selection();
            }
            Ok(Output::Diagnostics(report)) => {
                self.diagnostics = Some(report);
                self.showing_diagnostics = true;
                self.tab = Tab::Checks;
                self.shell.overlay = None;
                self.details_scroll = 0;
                self.status =
                    "Read-only Diagnostics loaded; unavailable observations include their cause."
                        .into();
            }
            Ok(Output::Git(report)) => {
                self.git_view.accept(
                    report,
                    self.tab == Tab::Git
                        && !self.quit
                        && self.shell.overlay.is_none()
                        && self.help.is_none()
                        && self.navigation.menu.is_none(),
                );
                self.status = "Git read completed. Summaries are metadata-only; patch viewing requires a new confirmation.".into();
            }
            Ok(Output::Data(data)) => {
                self.replace_data(data);
                // Adoption must refresh managed/history observations as well as
                // Discover, so immediately opening Recovery sees the new receipt.
                if self.rediscovering_after_adoption && !self.quit {
                    return Some(Task::Discover);
                }
                // Preserve execution errors/completion until the next explicit action.
                if self.status == "Reading..." {
                    self.status = "Read-only observations loaded.".into();
                } else if self.status.starts_with("Operation completed.") {
                    self.status = "Operation completed. Observations refreshed.".into();
                }
            }
            Ok(Output::Checked { report, standalone }) => {
                self.check = Some((report, standalone));
                self.showing_diagnostics = false;
                self.tab = Tab::Checks;
                self.shell.overlay = None;
                self.details_scroll = 0;
                self.status =
                    "Declarations validated; inspect Files for live state and verify installed tools separately.".into();
            }
            Ok(Output::Profiles {
                report,
                requested,
                standalone,
                apply,
            }) => {
                self.preview = None;
                self.confirmation = None;
                if apply {
                    if standalone {
                        self.shell.check_profiles = requested;
                        self.check = Some((report, true));
                        self.shell.overlay = Some(Overlay::Form(Form::Check));
                    } else {
                        self.requested_profiles = requested;
                        self.shell.overlay = None;
                        if self
                            .check
                            .as_ref()
                            .is_some_and(|(_, standalone)| !standalone)
                        {
                            self.check = None;
                        }
                        self.status = "Temporary profile additions changed; refreshing full active scope. Configuration unchanged.".into();
                        if !self.quit {
                            return Some(Task::Refresh);
                        }
                    }
                    self.status =
                        "Standalone profile additions validated; configuration unchanged.".into();
                } else {
                    self.shell.scroll = 0;
                    self.shell.overlay = Some(Overlay::Profiles(Box::new(ProfilePicker {
                        report,
                        requested,
                        standalone,
                        selected: 0,
                    })));
                }
            }
            Ok(Output::Discovery(discovery)) => {
                let old_path = self
                    .discovery
                    .as_ref()
                    .and_then(|d| d.report.candidates.get(self.candidate_selected))
                    .map(|c| &c.entry.path);
                let position = old_path.and_then(|path| {
                    discovery
                        .report
                        .candidates
                        .iter()
                        .position(|c| &c.entry.path == path)
                });
                if old_path.is_some() && position.is_none() {
                    self.status =
                        "Selected candidate disappeared; showing nearest remaining row.".into();
                } else {
                    self.status = "Discovery loaded; review each candidate before adoption.".into();
                }
                self.candidate_selected = position
                    .unwrap_or(self.candidate_selected)
                    .min(discovery.report.candidates.len().saturating_sub(1));
                self.discovery = Some(std::sync::Arc::new(discovery));
                self.clamp_file_selection();
                if std::mem::take(&mut self.rediscovering_after_adoption)
                    && let Some(receipt) = &self.receipt
                {
                    self.status = format!(
                        "{} completed; Discover refreshed. Receipt in details; Recovery retains transaction history.",
                        receipt.operation
                    );
                }
            }
            Ok(Output::Selected(repository, data)) => {
                self.origin = if repository.is_some() {
                    "session choice"
                } else {
                    "registered active"
                };
                self.repository = repository;
                self.reset_repository_scope();
                self.discovery = None;
                self.candidate_selected = 0;
                self.result = None;
                self.receipt = None;
                self.show_result = false;
                self.data = None;
                self.selected = 0;
                self.details_scroll = 0;
                self.preview = None;
                self.confirmation = None;
                self.shell.back();
                self.replace_data(data);
                self.status =
                    "Repository selected for this session only; configuration unchanged.".into();
            }
            Ok(Output::IgnoredPaths(paths)) => {
                self.shell.overlay = Some(Overlay::Ignored { paths, selected: 0 });
                self.shell.scroll = 0;
                self.status = "Select an exact ignored path; Enter previews Unignore, Esc returns. Ancestor rules still apply.".into();
            }
            Ok(Output::Preview(pending)) => match pending.report() {
                Ok(text) => {
                    self.reviewed_activity =
                        Some(super::task_surfaces::Activity::for_pending(&pending));
                    self.preview = Some(Preview {
                        pending,
                        text,
                        scroll: 0,
                        horizontal: 0,
                        full: false,
                        selected: 0,
                        effect_scroll: 0,
                    });
                    self.status = if self.preview.as_ref().is_some_and(|p| p.pending.adoption()) {
                        "Review all mappings; Adopt all approves once. Esc returns to the draft."
                    } else {
                        "Review effects; Enter opens confirmation, Esc dismisses."
                    }
                    .into();
                }
                Err(error) => self.status = super::actions::diagnostic(&error),
            },
            Ok(Output::Completed(completion)) => {
                if completion.operation == "Discovery preferences" {
                    self.content = Default::default();
                    self.discovery = None;
                    self.inventory.candidate_marks.clear();
                    self.inventory.candidate_mark_revision =
                        self.inventory.candidate_mark_revision.wrapping_add(1);
                    self.shell.back();
                    self.tab = Tab::Files;
                    self.discovering = true;
                    self.inventory.details = false;
                    self.status = "Discovery preferences saved. Refreshing discovery; undo in Actions > Manage ignored paths.".into();
                    if !self.quit {
                        return Some(Task::Discover);
                    }
                    return None;
                }
                self.receipt = Some(super::task_surfaces::Receipt {
                    operation: completion.operation,
                    transaction_id: completion.transaction_id.clone(),
                    activity: activity.or_else(|| self.reviewed_activity.clone()),
                });
                self.show_result = true;
                self.details_scroll = 0;
                if let Some(editor) = &mut self.batch_editor {
                    editor.open = false;
                }
                let adopted = std::mem::take(&mut self.return_to_discover);
                if !adopted {
                    self.discovery = None;
                }
                if let Some(root) = completion.repository {
                    self.repository = Some(root);
                    self.reset_repository_scope();
                    self.origin = "approved create/attach";
                    self.tab = if self.shell.return_to_adopt {
                        Tab::Files
                    } else {
                        Tab::Overview
                    };
                    self.data = None;
                    self.selected = 0;
                    self.details_scroll = 0;
                }
                self.shell.back();
                self.result = Some(format!(
                    "{} completed.\n{}",
                    completion.operation,
                    completion
                        .transaction_id
                        .map(|id| format!("Transaction ID: {id}"))
                        .unwrap_or_else(|| "Everything is up to date.".into())
                ));
                self.status = "Operation completed. Refreshing observations...".into();
                if adopted {
                    self.rediscovering_after_adoption = true;
                    self.tab = Tab::Files;
                    self.discovering = true;
                    self.show_result = false;
                    self.inventory.details = false;
                    self.inventory.candidate_marks.clear();
                    self.inventory.candidate_mark_revision =
                        self.inventory.candidate_mark_revision.wrapping_add(1);
                }
                if !self.quit {
                    return Some(Task::Refresh);
                }
            }
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
        self.mouse_at(event, width, height, std::time::Instant::now())
    }
    pub(super) fn mouse_at(
        &mut self,
        event: ratatui::crossterm::event::MouseEvent,
        width: u16,
        height: u16,
        now: std::time::Instant,
    ) -> Option<Task> {
        self.inventory.pending_g = false;
        if self.help.is_some() {
            use ratatui::crossterm::event::MouseEventKind;
            self.resize(width, height);
            let code = match event.kind {
                MouseEventKind::ScrollDown => Some(KeyCode::PageDown),
                MouseEventKind::ScrollUp => Some(KeyCode::PageUp),
                MouseEventKind::Down(ratatui::crossterm::event::MouseButton::Left)
                    if matches!(
                        self.hits.target(event.column, event.row),
                        Some(super::input::Target::Back)
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
        if self.navigation.menu.is_some() {
            let target = self.hits.target(event.column, event.row);
            self.hits.clear();
            if matches!(
                event.kind,
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
            ) {
                let code = if event.kind == MouseEventKind::ScrollDown {
                    KeyCode::Down
                } else {
                    KeyCode::Up
                };
                return self.key(KeyEvent::new(code, KeyModifiers::NONE), width, height);
            }
            if event.kind == MouseEventKind::Down(MouseButton::Left) {
                match target {
                    Some(Target::MenuGroup(group)) => {
                        self.open_menu(group, width);
                        if let Some(menu) = &mut self.navigation.menu {
                            menu.expanded = true;
                        }
                    }
                    Some(Target::MenuItem(index)) => {
                        let group = self.navigation.menu.as_ref()?.group;
                        if let Some(entry) = super::navigation::entries(group).get(index) {
                            let task = self.menu_item(entry.item);
                            super::discover_content::sync(self, width, height);
                            return task;
                        }
                    }
                    _ => {
                        return self.key(
                            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                            width,
                            height,
                        );
                    }
                }
                super::discover_content::sync(self, width, height);
            }
            return None;
        }
        let code = match event.kind {
            MouseEventKind::ScrollDown => Some(KeyCode::PageDown),
            MouseEventKind::ScrollUp => Some(KeyCode::PageUp),
            _ => None,
        };
        if let Some(code) = code {
            self.last_click = None;
            let wheel = self.hits.scroll(event.column, event.row)?;
            if matches!(wheel, Wheel::Choices | Wheel::PreviewRows) {
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
            if matches!(wheel, Wheel::Details) {
                self.hits.clear();
                if self.tab == Tab::Git {
                    self.git_view.scroll_metadata_details(code);
                } else {
                    super::shell::scroll_details(&mut self.details_scroll, code);
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
        if event.kind != MouseEventKind::Down(MouseButton::Left) || self.confirmation.is_some() {
            return None;
        }
        let target = self.hits.target(event.column, event.row)?;
        if self.tab == Tab::Files
            && matches!(
                target,
                Target::Row(_) | Target::TreeFold(_) | Target::TreeMark(_) | Target::Filter
            )
        {
            self.content.focused = false;
        }
        let previous = self.last_click.take();
        let row_click = matches!(target, Target::Row(_)) && self.tab == Tab::Files && !self.busy;
        // Row selection does not alter layout: retain render-owned hits for coalesced clicks.
        // Checkbox/chevron hits remain single-click controls, never double-click marks.
        if !row_click {
            self.hits.clear();
        }
        if row_click && let Target::Row(index) = target {
            if previous.is_some_and(|click| {
                click.key == index
                    && click.discovering == self.discovering
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
                discovering: self.discovering,
                size: (width, height),
                row: event.row,
            });
        }
        if self.busy {
            return None;
        }
        match target {
            Target::MenuOpen => {
                self.open_menu(super::navigation::group(self), width);
                None
            }
            Target::MenuGroup(group) => {
                self.open_menu(group, width);
                if let Some(menu) = &mut self.navigation.menu {
                    menu.expanded = true;
                }
                None
            }
            Target::MenuItem(_) => None,
            Target::Feedback(control) => self.feedback(control, width, height),
            Target::Continue => {
                if self.preview.is_some() {
                    self.key(
                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        width,
                        height,
                    )
                } else {
                    None
                }
            }
            Target::PreviewRow(index) => {
                if let Some(preview) = &mut self.preview
                    && preview.pending.review_len().is_some_and(|len| index < len)
                {
                    preview.selected = index;
                    preview.effect_scroll = 0;
                }
                None
            }
            Target::DiscoverFocus(content) => {
                self.content.focused = content && self.content.available();
                self.inventory.details = false;
                None
            }
            Target::BatchEdit => {
                self.batch_editor.as_mut().filter(|e| e.open)?.edit_item();
                None
            }
            Target::BatchControl(index, activate) => {
                let editor = self.batch_editor.as_mut().filter(|e| e.open)?;
                editor.focused = index;
                if !editor.item_form && index < editor.drafts.len() * 3 {
                    editor.item = index / 3;
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
                        let folded = if self.discovering {
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
            Target::Action(action) => self.activate(action),
            Target::Back => self.key(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                width,
                height,
            ),
            Target::Row(index) => {
                if self.tab == Tab::Files && super::tree::position(self, index).is_some() {
                    super::tree::select(self, index);
                    self.inventory.details = false;
                } else if self.tab == Tab::Recovery && self.visible_history_rows().contains(&index)
                {
                    self.selected = index;
                    self.history_details = false;
                } else if self.tab == Tab::Git {
                    self.git_view.select_metadata(index);
                }
                self.details_scroll = 0;
                None
            }
            Target::Filter => {
                if self.tab == Tab::Files {
                    self.inventory.search_mut(self.discovering).editing = true;
                }
                if self.tab == Tab::Recovery {
                    self.history_search.editing = true;
                }
                if self.tab == Tab::Git {
                    self.git_view.search.editing = true;
                }
                None
            }
            Target::FormControl(index, activate) => {
                match self.shell.overlay {
                    Some(Overlay::Form(Form::Adopt)) => self.shell.adoption.focused = index,
                    Some(Overlay::Form(Form::Check)) => self.shell.manifest_focused = index,
                    Some(Overlay::Form(Form::KeepId | Form::RemoveId)) => {
                        self.shell.unmanage_focused = index
                    }
                    Some(Overlay::Form(Form::Choose | Form::Init)) => {
                        self.shell.path_focused = index
                    }
                    _ => return None,
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
            Target::Picker(index, activate) => {
                match &mut self.shell.overlay {
                    Some(Overlay::Repositories { selected }) => *selected = index,
                    Some(Overlay::Profiles(picker)) => picker.selected = index,
                    Some(Overlay::Ignored { paths, selected }) if index < paths.len() => {
                        *selected = index
                    }
                    _ => return None,
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
            Target::Palette(action) => {
                if let Some(Overlay::Actions {
                    entries,
                    query,
                    selected,
                    searching,
                }) = &mut self.shell.overlay
                {
                    if let Some(index) = super::palette::matches(entries, query)
                        .iter()
                        .position(|entry| entry.action == action)
                    {
                        *selected = index;
                    }
                    *searching = false;
                }
                self.intent(Intent::Action(action))
            }
            Target::PaletteSearch => {
                if let Some(Overlay::Actions { searching, .. }) = &mut self.shell.overlay {
                    *searching = true;
                } else {
                    self.git_view.discard_patch();
                    self.shell.open_actions(super::palette::entries(self));
                }
                None
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
            return self.key(
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
                self.git_view.discard_patch();
                self.tab = Tab::Recovery;
                self.shell.overlay = None;
                Some(Task::History)
            }
            Control::Back => None,
        }
    }
    pub fn key(&mut self, key: KeyEvent, width: u16, height: u16) -> Option<Task> {
        self.hits.clear();
        self.last_click = None;
        if key.code != KeyCode::Char('g')
            || key.modifiers != KeyModifiers::NONE
            || self.tab != Tab::Files
            || self.text_entry()
            || self.preview.is_some()
            || self.shell.overlay.is_some()
            || self.busy
        {
            self.inventory.pending_g = false;
        }
        let return_content = self
            .help
            .as_ref()
            .map(|h| (h.content_focus, h.content_scroll))
            .or_else(|| {
                self.navigation
                    .menu
                    .map(|m| (m.content_focus, m.content_scroll))
            });
        let task = self.key_inner(key, width, height);
        super::discover_content::sync(self, width, height);
        if self.help.is_none()
            && self.navigation.menu.is_none()
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
    fn key_inner(&mut self, key: KeyEvent, width: u16, height: u16) -> Option<Task> {
        let interrupt =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if interrupt
            || (key.code == KeyCode::Char('q')
                && (self.help.is_some()
                    || self.navigation.menu.is_some()
                    || self.confirmation.is_none()
                        && (self.error.is_some()
                            || self.show_result
                            || self.busy
                            || self.preview.is_some()
                            || !self.text_entry())))
        {
            self.git_view.discard_patch();
            if self.busy {
                self.exit_requested = true;
                self.status = "Exit requested; waiting for the active worker. Approved writes run to completion.".into();
            } else {
                self.quit = true;
            }
            return None;
        }
        self.resize(width, height);
        if let Some(help) = &mut self.help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') => self.help = None,
                KeyCode::Home => help.scroll = 0,
                KeyCode::PageDown | KeyCode::Down | KeyCode::Char('j') => {
                    help.scroll = help.scroll.saturating_add(2)
                }
                KeyCode::PageUp | KeyCode::Up | KeyCode::Char('k') => {
                    help.scroll = help.scroll.saturating_sub(2)
                }
                _ => (),
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
            self.navigation.menu = None;
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
                    self.inventory.search_mut(self.discovering).editing = false;
                    self.inventory.details = false;
                    self.history_search.editing = false;
                    self.history_details = false;
                }
            }
            self.status = "Resize to at least 48x14 to review and approve.".into();
            return None;
        }
        if self.navigation.menu.is_some() {
            return self.menu_key(key.code, width);
        }
        if key.code == KeyCode::F(10) && self.menus_available() {
            self.open_menu(super::navigation::group(self), width);
            return None;
        }
        if self.busy {
            if key.code == KeyCode::Esc {
                self.shell.overlay = None;
            }
            let destination = match key.code {
                KeyCode::Left => Some(self.tab.previous()),
                KeyCode::Right => Some(self.tab.next()),
                _ => None,
            };
            if let Some(tab) = destination {
                self.tab = tab;
                self.selected = 0;
                self.git_view.discard_patch();
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
                KeyCode::Char('h') => {
                    self.error = None;
                    self.preview = None;
                    self.confirmation = None;
                    self.retry = None;
                    self.tab = Tab::Recovery;
                    self.shell.overlay = None;

                    self.details_scroll = 0;
                    return Some(Task::History);
                }
                KeyCode::Char('r') => {
                    self.error = None;
                    self.preview = None;
                    self.confirmation = None;
                    self.details_scroll = 0;
                    if let Some(intent) = self.retry.take() {
                        return Some(intent.task());
                    }
                    if self.tab == Tab::Git {
                        return self.git_task();
                    }
                    return Some(self.refresh_task());
                }
                KeyCode::Esc => {
                    self.error = None;
                    self.failure = None;
                    self.failed_write = false;
                    self.retry = None;
                    self.details_scroll = 0;
                }
                KeyCode::Home => self.details_scroll = 0,
                KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(2),
                KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(2),
                _ => (),
            }
            return None;
        }
        if self.show_result && self.preview.is_none() {
            use super::task_surfaces::Control;
            let control = match key.code {
                KeyCode::Char('r') => Some(Control::Retry),
                KeyCode::Char('h') => Some(Control::Recovery),
                KeyCode::Esc => Some(Control::Back),
                KeyCode::Home | KeyCode::PageDown | KeyCode::PageUp => {
                    super::shell::scroll_details(&mut self.details_scroll, key.code);
                    return None;
                }
                KeyCode::Tab => return None,
                _ => None,
            };
            if let Some(control) = control {
                return self.feedback(control, width, height);
            }
            self.show_result = false;
            self.details_scroll = 0;
        }
        if self.confirmation.is_some() {
            if self
                .preview
                .as_ref()
                .is_some_and(|p| matches!(p.pending, Pending::Batch(_)))
            {
                return self.batch_confirm_key(key);
            }
            return self.confirm_key(key);
        }
        if key.code == KeyCode::Enter
            && !self.dry_run
            && self
                .preview
                .as_ref()
                .is_some_and(|p| p.pending.adoption() && !p.pending.blocked())
        {
            let preview = self.preview.take().expect("review retained");
            return match preview.pending.approve(None) {
                Ok(approved) => {
                    self.return_to_discover = true;
                    Some(Task::Execute(approved))
                }
                Err(error) => {
                    self.status = super::actions::diagnostic(&error);
                    None
                }
            };
        }
        if let Some(preview) = &mut self.preview {
            let file_review = preview.pending.review_len().is_some();
            match key.code {
                KeyCode::Char('i') if file_review => {
                    preview.full = !preview.full;
                    preview.scroll = 0;
                    preview.horizontal = 0;
                    preview.effect_scroll = 0;
                }
                KeyCode::Down | KeyCode::Up if file_review && !preview.full => {
                    if let Some(len) = preview.pending.review_len() {
                        preview.selected = if key.code == KeyCode::Down {
                            (preview.selected + 1).min(len.saturating_sub(1))
                        } else {
                            preview.selected.saturating_sub(1)
                        };
                        preview.effect_scroll = 0;
                    }
                }
                KeyCode::PageDown | KeyCode::PageUp if file_review && !preview.full => {
                    preview.effect_scroll = if key.code == KeyCode::PageDown {
                        preview.effect_scroll.saturating_add(4)
                    } else {
                        preview.effect_scroll.saturating_sub(4)
                    };
                }
                KeyCode::Home => {
                    preview.scroll = 0;
                    preview.horizontal = 0;
                    preview.effect_scroll = 0;
                }
                KeyCode::Esc => {
                    self.preview = None;
                    self.status = "Returned to the draft.".into();
                }
                KeyCode::Down | KeyCode::PageDown => {
                    preview.scroll = preview
                        .scroll
                        .saturating_add(if key.code == KeyCode::Down { 1 } else { 8 })
                        .min(
                            preview
                                .text
                                .lines()
                                .count()
                                .saturating_sub(1)
                                .min(u16::MAX as usize) as u16,
                        )
                }
                KeyCode::Up | KeyCode::PageUp => {
                    preview.scroll =
                        preview
                            .scroll
                            .saturating_sub(if key.code == KeyCode::Up { 1 } else { 8 })
                }
                KeyCode::Right => preview.horizontal = preview.horizontal.saturating_add(8),
                KeyCode::Left => preview.horizontal = preview.horizontal.saturating_sub(8),
                KeyCode::Enter if !preview.pending.blocked() && !self.dry_run => {
                    self.confirmation = Some(Confirmation {
                        prompt_scroll: 0,
                        input: String::new(),
                        removal_ack: None,
                        batch_acks: vec![],
                    })
                }
                KeyCode::Enter => {
                    self.status = "Approval disabled: blocked preview or --dry-run.".into()
                }
                _ => (),
            }
            if let Some(preview) = &mut self.preview {
                preview.clamp_scroll(width, height);
            }
            return None;
        }
        if let Some(editor) = self.batch_editor.as_mut().filter(|e| e.open) {
            return editor.key(key);
        }
        if self.shell.overlay.is_some() {
            if key.code == KeyCode::Home {
                self.shell.scroll = 0;
            }
            if key.code == KeyCode::PageDown {
                self.shell.page(true);
            }
            if key.code == KeyCode::PageUp {
                self.shell.page(false);
            }
            let count = self.data.as_ref().map_or(0, |data| data.known_roots.len());
            if let Some(intent) = self.shell.key(key, count) {
                return self.intent(intent);
            }
            return None;
        }
        // Only Discover offers region focus; forms and disclosures retain local routing.
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) && !self.text_entry() {
            if self.tab == Tab::Files && self.discovering && self.content.available() {
                self.content.focused = !self.content.focused;
                self.inventory.details = false;
            }
            return None;
        }
        if self.tab == Tab::Files && self.discovering && self.content.focused && !self.text_entry()
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
                    KeyCode::Char('g') => {
                        if std::mem::take(&mut self.inventory.pending_g) {
                            self.content.scroll = 0;
                        } else {
                            self.inventory.pending_g = true;
                        }
                    }
                    KeyCode::Char('G') => self.content.scroll = self.content.end.get(),
                    _ => return self.workspace_key(key),
                }
                return None;
            }
        }
        self.workspace_key(key)
    }
    fn workspace_key(&mut self, key: KeyEvent) -> Option<Task> {
        if !self.text_entry()
            && !self.git_view.sensitive()
            && matches!(key.code, KeyCode::Left | KeyCode::Right)
        {
            let tab = if key.code == KeyCode::Left {
                self.tab.previous()
            } else {
                self.tab.next()
            };
            return self.activate(Action::Destination(tab));
        }
        let content_area = ratatui::layout::Rect::new(
            0,
            2,
            self.viewport.0,
            self.viewport
                .1
                .saturating_sub(4 + u16::from(!self.page_actions().is_empty())),
        );
        let split_details = if self.tab == Tab::Git {
            self.git_view.wide_metadata(content_area)
        } else {
            super::workbench::wide_details(content_area, 2)
        };
        if self.tab == Tab::Files
            && self.discovering
            && !self.text_entry()
            && !key.modifiers.contains(KeyModifiers::ALT)
            && matches!(
                key.code,
                KeyCode::PageDown | KeyCode::PageUp | KeyCode::Home
            )
        {
            if !self.inventory.details && !split_details {
                let backwards = key.code != KeyCode::PageDown;
                let count = if key.code == KeyCode::Home {
                    usize::MAX
                } else {
                    self.inventory.page_capacity.get().max(1)
                };
                self.move_files(backwards, count);
                return None;
            }
            if key.code == KeyCode::Home {
                self.content.scroll = 0;
            } else {
                self.content.scroll_by(key.code == KeyCode::PageUp, 2);
            }
            return None;
        }
        if !self.text_entry()
            && matches!(
                key.code,
                KeyCode::PageDown | KeyCode::PageUp | KeyCode::Home
            )
            && !split_details
        {
            let backwards = key.code != KeyCode::PageDown;
            let count = if key.code == KeyCode::Home {
                usize::MAX
            } else {
                2
            };
            if self.tab == Tab::Files && !self.inventory.details {
                self.move_files(backwards, count);
                return None;
            }
            if self.tab == Tab::Recovery
                && !self.history_details
                && self
                    .data
                    .as_ref()
                    .is_some_and(|d| d.history_error.is_none())
            {
                self.page_history(backwards, count);
                return None;
            }
            if self.tab == Tab::Git
                && !self.git_view.browsing_details()
                && !self.git_view.navigation_input()
                && !self.git_view.sensitive()
            {
                self.git_view.page_metadata(backwards, count);
                return None;
            }
        }
        if self.tab == Tab::Git {
            if self.git_view.editing_root
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
                KeyCode::Char('w') => Some(Action::GitPatch),
                KeyCode::Char('r') => Some(Action::GitRefresh),
                _ => None,
            };
            if let Some(action) = action {
                return self.activate(action);
            }
        }
        // Any outer-shell navigation/action drops a disclosed patch reference.
        if self.git_view.sensitive() {
            self.git_view.discard_patch();
        }
        if self.tab == Tab::Recovery && self.history_search.editing {
            self.history_search.key(key);
            self.clamp_history_selection();
            self.details_scroll = 0;
            return None;
        }
        if self.tab == Tab::Files && self.inventory.search(self.discovering).editing {
            self.inventory.search_mut(self.discovering).key(key);
            self.clamp_file_selection();
            self.details_scroll = 0;
            return None;
        }
        if self.tab == Tab::Files
            && (!self.inventory.details || self.discovering)
            && !self.content.focused
        {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let page = self.inventory.page_capacity.get().max(1);
                let (backwards, count) = match key.code {
                    KeyCode::Char('f') => (false, page),
                    KeyCode::Char('b') => (true, page),
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
                KeyCode::Char('g') => {
                    if std::mem::take(&mut self.inventory.pending_g) {
                        self.move_files(true, usize::MAX);
                    } else {
                        self.inventory.pending_g = true;
                    }
                    return None;
                }
                KeyCode::Char('G') => {
                    self.move_files(false, usize::MAX);
                    return None;
                }
                _ => (),
            }
        }
        let opens_overlay = matches!(key.code, KeyCode::Char('?' | ';' | '/' | 'p' | 'n'));
        if !matches!(
            key.code,
            KeyCode::PageDown | KeyCode::PageUp | KeyCode::Tab | KeyCode::BackTab
        ) && !opens_overlay
        {
            self.details_scroll = 0;
        }
        match key.code {
            KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(2),
            KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(2),
            KeyCode::Home => self.details_scroll = 0,
            KeyCode::Char(' ') if self.tab == Tab::Files => {
                return self.activate(Action::ToggleMark);
            }
            KeyCode::Char('h') if self.tab == Tab::Files => super::tree::fold(self, true),
            KeyCode::Char('l') if self.tab == Tab::Files => super::tree::fold(self, false),
            KeyCode::Char('E') if self.tab == Tab::Files => {
                return self.activate(Action::BatchAdopt);
            }
            KeyCode::Char('A') if self.tab == Tab::Files => {
                return self.activate(Action::ApplyMarked);
            }
            KeyCode::Char('u')
                if self.tab == Tab::Files
                    && !self.discovering
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                return self.activate(Action::Keep);
            }
            KeyCode::Char('K') if self.tab == Tab::Files => {
                return self.activate(Action::KeepMarked);
            }
            KeyCode::Char('D') if self.tab == Tab::Files => {
                return self.activate(Action::RemoveMarked);
            }
            KeyCode::Char('I') if self.tab == Tab::Files && self.discovering => {
                return self.activate(Action::IgnoreSelected);
            }
            KeyCode::Char('U') if self.tab == Tab::Files && self.discovering => {
                return self.activate(Action::ManageIgnored);
            }
            KeyCode::Char('e') if self.tab == Tab::Files && self.discovering => {
                return self.activate(Action::AdoptSelected);
            }
            KeyCode::Esc => {
                self.inventory.details = false;
                self.history_details = false;

                self.result = None;
            }
            KeyCode::Char(';') => self.shell.open_actions(super::palette::entries(self)),
            KeyCode::Char('/') if self.tab == Tab::Files => {
                self.inventory.search_mut(self.discovering).editing = true
            }
            KeyCode::Char('/') if self.tab == Tab::Recovery => self.history_search.editing = true,
            KeyCode::Char('/') => self.shell.open_actions(super::palette::entries(self)),
            KeyCode::Delete if self.tab == Tab::Recovery => {
                self.history_search.query.clear();
                self.clamp_history_selection();
            }
            KeyCode::Enter if self.tab == Tab::Recovery => {
                self.history_details = true;
                self.details_scroll = 0;
            }
            KeyCode::Down if self.tab == Tab::Recovery => self.move_history_selection(false),
            KeyCode::Up if self.tab == Tab::Recovery => self.move_history_selection(true),
            KeyCode::Char('j') if self.tab == Tab::Checks => {
                return self.activate(Action::Diagnostics);
            }
            KeyCode::Char('m') if self.tab == Tab::Checks => {
                self.showing_diagnostics = false;
                self.details_scroll = 0;
            }
            KeyCode::Delete if self.tab == Tab::Files => return self.activate(Action::ClearFilter),
            KeyCode::Char('p') => return self.activate(Action::Choose),
            KeyCode::Char('n') => return self.activate(Action::Init),
            KeyCode::Char('c') => return self.activate(Action::CheckCurrent),
            KeyCode::Char('v') => return self.activate(Action::CheckStandalone),
            KeyCode::Char('t') => return self.activate(Action::Profiles),
            KeyCode::Char('?') => return self.activate(Action::Help),
            KeyCode::Char('r') => return self.activate(Action::Refresh),
            KeyCode::Char('f') => return self.activate(Action::Discover),
            KeyCode::Char('o') => return self.activate(Action::Adopt),
            KeyCode::Enter if self.tab == Tab::Files && self.discovering => {
                self.inventory.details = true;
                self.details_scroll = 0;
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
            KeyCode::Char('a') if self.tab == Tab::Files || self.tab == Tab::Overview => {
                return self.activate(Action::Apply);
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
            self.inventory.pending_g = false;
            self.last_click = None;
            self.hits.clear();
        }
        self.viewport = (width, height);
        if width < 48 || height < 14 {
            self.navigation.menu = None;
            self.confirmation = None;
            self.git_view.resize();
        } else if let Some(preview) = &mut self.preview {
            preview.clamp_scroll(width, height);
        }
        super::discover_content::sync(self, width, height);
    }
    fn replace_data(&mut self, data: Data) {
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
            let ids: std::collections::BTreeSet<_> =
                data.files.iter().map(|(id, _)| id.as_str()).collect();
            self.inventory
                .managed_marks
                .retain(|id| ids.contains(id.as_str()));
        }
        let git_default = data.repository.clone().or(self.repository.clone());
        if self.git_view.follows_files && self.git_view.root != git_default {
            self.git_view.scope_changed();
            self.git_view.root = git_default;
        }
        self.data = Some(data);
        self.selected = position
            .unwrap_or(self.selected)
            .min(self.row_count().saturating_sub(1));
        self.clamp_file_selection();
        self.clamp_history_selection();
    }
    fn intent(&mut self, intent: Intent) -> Option<Task> {
        match intent {
            Intent::Unignore(path) => Some(Task::Ignore(path, false)),
            Intent::Action(action) => {
                if matches!(self.shell.overlay, Some(Overlay::Actions { .. }))
                    && let Some(reason) = self.action_reason(action)
                {
                    self.status = reason.into();
                    return None;
                }
                self.activate(action)
            }
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
                    self.status =
                        "Enter an exact mapping ID; the preview uses this exact ID.".into();
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
                if draft.target.is_empty() || draft.id.is_empty() || draft.source.is_empty() {
                    self.status =
                        "Enter TARGET, ID and repository-relative SOURCE; draft retained.".into();
                    return None;
                }
                // The core owns all path/kind/sensitivity/mapping policy.
                Some(Task::Files(crate::services::adopt_request(
                    std::path::Path::new(&draft.target),
                    &draft.id,
                    std::path::Path::new(&draft.source),
                )))
            }
            Intent::Submit(form) => {
                let draft = self.shell.draft(form);
                if draft.is_empty() {
                    self.status = "Enter an exact path; input is retained in memory only.".into();
                    return None;
                }
                let root = crate::services::resolve_root(
                    std::path::Path::new(draft),
                    &self.launch_directory,
                );
                match form {
                    Form::Adopt | Form::Check | Form::KeepId | Form::RemoveId => unreachable!(),
                    Form::Choose => Some(Task::Select(Some(root))),
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
        self.help = Some(super::help::State {
            topic: super::help::topic(self),
            scroll: 0,
            content_focus: self.content.focused,
            content_scroll: self.content.scroll,
        });
        super::discover_content::sync(self, self.viewport.0, self.viewport.1);
    }
    pub(super) fn activate(&mut self, action: Action) -> Option<Task> {
        if action == Action::Help {
            self.open_help();
            return None;
        }
        if let Some(reason) = self.action_reason(action) {
            self.status = reason.into();
            self.shell.overlay = None;
            return None;
        }
        self.failed_write = false;
        self.show_result = false;
        self.shell.scroll = 0;
        if matches!(self.shell.overlay, Some(Overlay::Form(Form::Adopt)))
            && matches!(action, Action::Choose | Action::Init)
        {
            self.shell.return_to_adopt = true;
        }
        self.shell.overlay = None;
        if self.git_view.sensitive() {
            self.git_view.discard_patch();
        }
        match action {
            Action::ToggleMark => super::tree::toggle(self),
            Action::ClearMarks => {
                if self.discovering {
                    self.inventory.candidate_marks.clear();
                    self.inventory.candidate_mark_revision = self.inventory.candidate_mark_revision.wrapping_add(1);
                } else {
                    self.inventory.managed_marks.clear();
                }
            }
            Action::ExpandAll | Action::CollapseAll => {
                let folds = super::visibility::Folds::all(action == Action::CollapseAll);
                let folded = if self.discovering {
                    &mut self.inventory.candidate_folded
                } else {
                    &mut self.inventory.managed_folded
                };
                *folded = std::sync::Arc::new(folds);
                self.clamp_file_selection();
            }
            Action::BatchAdopt => {
                let Some(discovery) = &self.discovery else { return None; };
                if let Some(editor) = self.batch_editor.as_mut() {
                    editor.sync(&self.inventory.candidate_marks, &discovery.home);
                } else {
                    self.batch_editor = Some(super::batch_ui::Editor::new(
                        &self.inventory.candidate_marks, &discovery.home,
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
            Action::GitPatch => {
                self.tab = Tab::Git;

                self.git_view.confirm_patch();
                self.status = "Patch viewing needs a loaded Unstaged or Staged summary. Type literal view patch in its warning to disclose once.".into();
            }
            Action::Destination(tab) => { self.go_tab(tab); if tab == Tab::Files { self.discovering = false; } }
            Action::Choose => self.shell.overlay = Some(Overlay::Repositories { selected: 0 }),
            Action::Init => { self.shell.path_focused = 0; self.shell.overlay = Some(Overlay::Form(Form::Init)); }
            Action::ManageIgnored => return Some(Task::IgnoredPaths),
            Action::IgnoreSelected => {
                if let Some(node) = super::tree::selected(self) {
                    return Some(Task::Ignore(node.path, true));
                }
            }
            Action::Discover => {
                self.go_tab(Tab::Files);
                self.discovering = true;
                return Some(Task::Discover);
            }
            Action::Adopt => {
                self.tab = Tab::Files;
                self.shell.overlay = Some(Overlay::Form(Form::Adopt));
            }
            Action::AdoptSelected => {
                if self.tab == Tab::Files && self.discovering {self.adopt_candidate(); }
                else { self.status = "Open Files > Discover and select an eligible regular file.".into(); }
            }
            Action::KeepId | Action::RemoveId => {
                self.tab = Tab::Files;
                self.discovering = false;
                self.shell.unmanage_focused = 0;
                self.shell.overlay = Some(Overlay::Form(if matches!(action, Action::KeepId) { Form::KeepId } else { Form::RemoveId }));
            }
            Action::ClearFilter if self.tab == Tab::Recovery => {
                self.history_search.query.clear();
                self.clamp_history_selection();
                self.status = "Recovery display filter cleared; global blockers remain visible.".into();
            }
            Action::ClearFilter if self.tab == Tab::Git => self.git_view.clear_filter(),
            Action::ClearFilter => {
                self.inventory.search_mut(self.discovering).query.clear();
                self.clamp_file_selection();
                self.status = "File display filter cleared; Apply uses the full active scope.".into();
            }
            Action::Inspect if self.tab == Tab::Git => self.git_view.inspect_metadata(),
            Action::Inspect if self.tab == Tab::Recovery => {

                self.history_details = true;
                self.details_scroll = 0;
            }
            Action::Inspect if self.tab == Tab::Files => {

                self.inventory.details = true;
                self.details_scroll = 0;
            }
            Action::Profiles => return self.check_task(false, true),
            Action::CheckCurrent => return self.check_task(false, false),
            Action::CheckStandalone => { self.tab = Tab::Checks; self.shell.overlay = Some(Overlay::Form(Form::Check)); }
            Action::StandaloneProfiles => {
                self.shell.overlay = Some(Overlay::Form(Form::Check));
                return self.check_task(true, true);
            }
            Action::Refresh if self.tab == Tab::Git => return self.git_task(),
            Action::Refresh => return Some(self.refresh_task()),
            Action::PreviewOnly => {
                if self.preview_only_locked { self.status = "--dry-run locks this session to preview-only.".into(); }
                else { self.dry_run = !self.dry_run; self.status = "Preview-only mode changed; approval always requires a fresh preview.".into(); }
            }
            Action::Diagnostics => { self.go_tab(Tab::Checks); self.showing_diagnostics = true; return Some(Task::Diagnostics); }
            Action::About => self.shell.overlay = Some(Overlay::About),
            Action::Help => unreachable!("Help preserves the underlying surface"),
            Action::Quit => self.quit = true,
            Action::Apply if self.data.as_ref().and_then(|d| d.repository.as_ref()).is_some() => return Some(Task::Files(Request::Apply)),
            Action::Keep | Action::Remove if self.tab == Tab::Files && !self.discovering => {
                if !self.visible_file_rows().contains(&self.selected) {
                    self.status = "No displayed mapping selected. Use Enter exact mapping ID in Actions.".into();
                    return None;
                }
                if let Some((id, _)) = self.data.as_ref().and_then(|data| data.files.get(self.selected)) {
                    return Some(Task::Files(Request::Unmanage {
                        id: id.clone(), mode: if matches!(action, Action::Keep) { UnmanageMode::Keep } else { UnmanageMode::Remove },
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
                        self.history_details = true;
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
                self.status = "Choose a transaction in Recovery first.".into();
            }
            _ => self.status = "Action unavailable: choose a repository, file in Files, or transaction in Recovery as labeled.".into(),
        }
        self.clamp_file_selection();
        self.clamp_history_selection();
        None
    }
    pub fn text_entry(&self) -> bool {
        self.batch_editor.as_ref().is_some_and(|e| e.open)
            || self.shell.text_entry()
            || (self.tab == Tab::Git && self.git_view.text_entry())
            || (self.tab == Tab::Recovery && self.history_search.editing)
            || (self.shell.overlay.is_none()
                && self.tab == Tab::Files
                && self.inventory.search(self.discovering).editing)
    }
    pub fn file_selection(&self) -> usize {
        if let Some(group) = self.inventory.group {
            return group;
        }
        if self.discovering {
            self.candidate_selected
        } else {
            self.selected
        }
    }
    pub fn file_rows_empty(&self) -> bool {
        if self.discovering {
            super::discovery_tree::snapshot(self).matching.is_empty()
        } else {
            self.visible_file_rows().is_empty()
        }
    }
    pub fn visible_file_rows(&self) -> Vec<usize> {
        let search = self.inventory.search(self.discovering);
        if self.discovering {
            super::discovery_tree::snapshot(self).matching.to_vec()
        } else {
            self.data
                .as_ref()
                .map(|d| {
                    d.files
                        .iter()
                        .enumerate()
                        .filter(|(_, row)| search.matches(&row.0) || search.matches(&row.1))
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
        if let Some(&index) = visible.get(next) {
            self.selected = index;
        }
    }
    fn reset_repository_scope(&mut self) {
        self.git_view.discard_patch();
        if self.git_view.follows_files {
            self.git_view.scope_changed();
            self.git_view.root = None;
        }
        self.inventory.group = None;
        self.inventory.managed_marks.clear();
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
        self.confirmation = None;
    }
    fn check_task(&mut self, standalone: bool, profiles: bool) -> Option<Task> {
        let manifest = if standalone {
            if self.shell.manifest_draft.is_empty() {
                self.shell.overlay = Some(Overlay::Form(Form::Check));
                self.status = "Enter a standalone manifest path; draft retained.".into();
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
        if self.tab == Tab::Files && self.discovering {
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
                self.status = "Not eligible for adoption: select an owned, single-link regular file. Directories cannot be adopted; use Enter for details or o for a manual draft.".into();
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
                    || !self.discovering
                    || super::tree::selected(self).is_none() =>
            {
                Some(
                    "Select a displayed discovery file or folder after the list finishes updating.",
                )
            }
            Action::ToggleMark | Action::ClearMarks | Action::ExpandAll | Action::CollapseAll
                if self.tab != Tab::Files =>
            {
                Some("Tree controls are available in Files.")
            }
            Action::BatchAdopt
                if self.tab != Tab::Files
                    || !self.discovering
                    || self.inventory.candidate_marks.is_empty() =>
            {
                Some(
                    "Mark discovered regular files with Space before opening a combined adoption draft.",
                )
            }
            Action::BatchAdopt
                if self.inventory.candidate_marks.len() > dort_core::batch::MAX_FILES =>
            {
                Some("A batch supports at most 32 marked files; clear some marks first.")
            }
            Action::ApplyMarked | Action::KeepMarked | Action::RemoveMarked
                if self.tab != Tab::Files
                    || self.discovering
                    || self.inventory.managed_marks.is_empty() =>
            {
                Some(
                    "Mark managed mappings with Space; uppercase A/K/D review the marked batch; Apply reviews the full active scope.",
                )
            }
            Action::Keep | Action::Remove | Action::AdoptSelected
                if self.inventory.group.is_some() =>
            {
                Some(
                    "Not eligible for adoption or single-file actions: select a regular file; Space marks matching descendants, including folded rows.",
                )
            }
            Action::Apply
                if self
                    .data
                    .as_ref()
                    .and_then(|d| d.repository.as_ref())
                    .is_none() =>
            {
                Some("Choose a repository before requesting a full-scope apply preview.")
            }
            Action::Keep | Action::Remove
                if self.tab != Tab::Files
                    || self.discovering
                    || !self.visible_file_rows().contains(&self.selected) =>
            {
                Some("Select a displayed managed mapping in Files, or use the exact-ID draft.")
            }
            Action::Inspect if self.tab == Tab::Files && self.file_rows_empty() => {
                Some("No displayed file selected; refresh, discover or clear the display filter.")
            }
            Action::Inspect if self.tab == Tab::Recovery && self.history_selection().is_none() => {
                Some(
                    "No displayed transaction selected; refresh or clear the Recovery display filter.",
                )
            }
            Action::Inspect if self.tab == Tab::Git && !self.git_view.has_selection() => {
                Some("No displayed Git entry selected; load metadata or clear the display filter.")
            }
            Action::Inspect if !matches!(self.tab, Tab::Files | Tab::Recovery | Tab::Git) => {
                Some("Select a file in Files or a transaction in Recovery to inspect.")
            }
            Action::ClearFilter if !matches!(self.tab, Tab::Files | Tab::Recovery | Tab::Git) => {
                Some("Display filters are available in Files, Recovery and Git.")
            }
            Action::PreviewOnly if self.preview_only_locked => {
                Some("--dry-run locks this session to preview-only; approval remains disabled.")
            }
            Action::GitPatch
                if !matches!(self.git_view.metadata, Some(super::git::Metadata::Diff(_))) =>
            {
                Some(
                    "Load an Unstaged or Staged Git summary before opening the separate content warning.",
                )
            }
            Action::AdoptSelected if self.tab != Tab::Files || !self.discovering => {
                Some("Select an eligible discovered regular file first.")
            }
            Action::AdoptSelected
                if !super::discovery_tree::contains(self, self.candidate_selected) =>
            {
                Some("Select a displayed candidate after the list finishes updating.")
            }
            Action::AdoptSelected
                if self
                    .discovery
                    .as_ref()
                    .and_then(|d| d.report.candidates.get(self.candidate_selected))
                    .is_none_or(|c| !c.supported_for_adoption) =>
            {
                Some(
                    "Not eligible for adoption: directories, links and special files are metadata only; choose an owned single-link regular file.",
                )
            }
            Action::Rollback
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::rollback(r)) =>
            {
                Some("Rollback unavailable; select an eligible schema-1/2/4 transaction.")
            }
            Action::Resume
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::resume(r)) =>
            {
                Some("Resume unavailable; select an intact Running bootstrap.")
            }
            Action::Abandon
                if self.tab != Tab::Recovery
                    || self
                        .history_selection()
                        .is_none_or(|r| !super::recovery::abandon(r)) =>
            {
                Some("Abandon unavailable; select an intact nonterminal bootstrap.")
            }
            _ => None,
        }
    }
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
    fn batch_confirm_key(&mut self, key: KeyEvent) -> Option<Task> {
        if key.code != KeyCode::Enter {
            return self.confirm_key(key);
        }
        let ids = self.batch_removal_ids();
        let confirmation = self.confirmation.as_mut()?;
        if let Some(id) = ids.get(confirmation.batch_acks.len()) {
            if &confirmation.input == id {
                confirmation.batch_acks.push(id.clone());
                confirmation.input.clear();
                confirmation.prompt_scroll = 0;
            } else {
                self.status = "Exact batch file ID mismatch; re-enter the requested ID.".into();
            }
            return None;
        }
        if confirmation.input != "yes" {
            self.status = "Acknowledge every deletion ID, then type literal yes.".into();
            return None;
        }
        let acks = confirmation.batch_acks.clone();
        let Pending::Batch(plan) = self.preview.take()?.pending else {
            return None;
        };
        self.confirmation = None;
        let approval = if plan.inputs.action == dort_core::batch::Action::Remove {
            dort_core::batch::Approval::from_removal_plan(
                *plan,
                dort_core::batch::RemovalAcknowledgements::confirm(acks),
            )
        } else {
            dort_core::batch::Approval::from_plan(*plan)
        };
        match approval {
            Ok(a) => Some(Task::Execute(super::actions::Approved::Batch(Box::new(a)))),
            Err(error) => {
                self.status = format!("Approval refused: {}", super::actions::diagnostic(&error));
                None
            }
        }
    }
    fn confirm_key(&mut self, key: KeyEvent) -> Option<Task> {
        let confirmation = self.confirmation.as_mut()?;
        match key.code {
            KeyCode::Esc => self.confirmation = None,
            KeyCode::PageDown => {
                confirmation.prompt_scroll = confirmation.prompt_scroll.saturating_add(4)
            }
            KeyCode::PageUp => {
                confirmation.prompt_scroll = confirmation.prompt_scroll.saturating_sub(4)
            }
            KeyCode::Home => confirmation.prompt_scroll = 0,
            KeyCode::Backspace => {
                confirmation.input.pop();
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && key
                        .modifiers
                        .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        .is_empty()
                    && confirmation.input.len() + c.len_utf8() <= 256 =>
            {
                confirmation.input.push(c)
            }
            KeyCode::Enter => {
                let preview = self.preview.as_ref()?;
                if let Some(id) = preview.pending.removal_id()
                    && confirmation.removal_ack.is_none()
                {
                    if confirmation.input == id {
                        confirmation.removal_ack = Some(confirmation.input.clone());
                        confirmation.prompt_scroll = 0;
                        confirmation.input.clear();
                    } else {
                        self.status = "Exact file ID mismatch; re-enter the requested ID.".into();
                    }
                } else if confirmation.input == "yes" {
                    let ack = confirmation.removal_ack.clone();
                    let pending = self.preview.take()?.pending;
                    self.confirmation = None;
                    match pending.approve(ack.as_deref()) {
                        Ok(approval) => return Some(Task::Execute(approval)),
                        Err(error) => {
                            self.status =
                                format!("Approval refused: {}", super::actions::diagnostic(&error))
                        }
                    }
                } else {
                    self.status = "Type literal yes to approve, or Esc to cancel.".into();
                }
            }
            _ => (),
        }
        None
    }
}
