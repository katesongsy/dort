use super::{
    actions::{Data, Output, Pending, Task},
    shell::{Action, Focus, Form, Intent, Overlay, ProfilePicker, Shell},
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
}
pub struct Confirmation {
    pub input: String,
    pub removal_ack: Option<String>,
}
pub struct App {
    pub tab: Tab,
    pub shell: Shell,
    pub repository: Option<PathBuf>,
    pub origin: &'static str,
    pub launch_directory: PathBuf,
    pub git: Result<PathBuf, String>,
    pub git_view: super::git::State,
    pub result: Option<String>,
    pub data: Option<Data>,
    pub discovering: bool,
    pub inventory: super::inventory::Inventory,
    pub discovery: Option<crate::services::Discovery>,
    pub candidate_selected: usize,
    pub requested_profiles: Vec<String>,
    pub check: Option<(Box<crate::services::CheckReport>, bool)>,
    pub diagnostics: Option<super::actions::Diagnostics>,
    pub showing_diagnostics: bool,
    pub history_search: super::inventory::Search,
    pub history_details: bool,
    pub retry: Option<super::actions::Retry>,
    pub selected: usize,
    pub details_scroll: u16,
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
}
impl App {
    pub fn new(dry_run: bool) -> Self {
        Self {
            tab: Tab::Overview,
            shell: Shell::new(),
            repository: None,
            origin: "registered active",
            launch_directory: PathBuf::from("/"),
            git: Err("Git unavailable; install Git separately and restart Dort".into()),
            git_view: super::git::State::new(),
            result: None,
            data: None,
            discovering: false,
            inventory: Default::default(),
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
            details_scroll: 0,
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
        }
    }
    pub fn start(&mut self, writing: bool) {
        self.busy = true;
        self.writing = writing;
        self.status = if writing {
            "Executing approved operation; quit waits for completion."
        } else {
            "Reading..."
        }
        .into();
    }
    pub fn complete(&mut self, result: Result<Output, String>) -> Option<Task> {
        let writing = self.writing;
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
                self.git_view.failed();
                self.details_scroll = 0;
                self.status = if writing {
                    format!(
                        "Error: {error}\nA failed write may leave partial effects. Inspect Recovery; nothing is automatically undone."
                    )
                } else {
                    format!(
                        "Read failed: {error}\nNo operation was approved by this read. Esc returns to retained input/scope."
                    )
                };
                self.preview = None;
                self.confirmation = None;
                if writing {
                    self.retry = None;
                }
                self.error = Some(self.status.clone());
            }
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
                    self.status = "All-repository diagnosis refreshed; journal integrity is not a live-drift check.".into();
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
                self.status = "Read-only Diagnostics loaded; unavailable observations are labeled, never repaired.".into();
            }
            Ok(Output::Git(report)) => {
                self.git_view.accept(
                    report,
                    self.tab == Tab::Git && !self.quit && self.shell.overlay.is_none(),
                );
                self.status = "Git read completed. Summaries are metadata-only; patch viewing requires a new confirmation.".into();
            }
            Ok(Output::Data(data)) => {
                self.replace_data(data);
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
                    "Structural check passed; live files and installed tools not inspected.".into();
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
                    self.status =
                        "Metadata discovery loaded; candidates are not a safety guarantee.".into();
                }
                self.candidate_selected = position
                    .unwrap_or(self.candidate_selected)
                    .min(discovery.report.candidates.len().saturating_sub(1));
                self.discovery = Some(discovery);
                self.clamp_file_selection();
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
            Ok(Output::Preview(pending)) => match pending.report() {
                Ok(text) => {
                    self.preview = Some(Preview {
                        pending,
                        text,
                        scroll: 0,
                        horizontal: 0,
                    });
                    self.status = "Review effects; Enter opens confirmation, Esc dismisses.".into();
                }
                Err(error) => self.status = super::actions::diagnostic(&error),
            },
            Ok(Output::Completed(completion)) => {
                self.discovery = None;
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
                        .unwrap_or_else(|| "No changes needed; no transaction created.".into())
                ));
                self.status = "Operation completed. Refreshing observations...".into();
                if !self.quit {
                    return Some(Task::Refresh);
                }
            }
        }
        None
    }
    pub fn key(&mut self, key: KeyEvent, width: u16, height: u16) -> Option<Task> {
        let interrupt =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if interrupt
            || (key.code == KeyCode::Char('q')
                && self.confirmation.is_none()
                && (self.preview.is_some() || !self.text_entry()))
        {
            self.git_view.discard_patch();
            if self.busy {
                self.exit_requested = true;
                self.status = "Exit requested; waiting for the active worker. Approved writes are not cancelled.".into();
            } else {
                self.quit = true;
            }
            return None;
        }
        self.resize(width, height);
        if width < 48 || height < 14 {
            if key.code == KeyCode::Esc {
                if self.error.is_some() {
                    self.error = None;
                    self.retry = None;
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
            self.status =
                "Resize to at least 48x14; no approval is accepted in a small layout.".into();
            return None;
        }
        if self.busy {
            if key.code == KeyCode::Char('?') {
                self.git_view.discard_patch();
                self.shell.scroll = 0;
                self.shell.overlay = Some(Overlay::Help);
            }
            if key.code == KeyCode::Esc {
                self.shell.overlay = None;
            }
            if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                self.tab = if key.code == KeyCode::Left {
                    self.tab.previous()
                } else {
                    self.tab.next()
                };
                self.selected = 0;
                self.git_view.discard_patch();
            }
            let scroll = if self.shell.overlay.is_some() {
                &mut self.shell.scroll
            } else {
                &mut self.details_scroll
            };
            if key.code == KeyCode::PageDown {
                *scroll = scroll.saturating_add(4);
            }
            if key.code == KeyCode::PageUp {
                *scroll = scroll.saturating_sub(4);
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
                    self.shell.focus = Focus::Content;
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
                    self.retry = None;
                    self.details_scroll = 0;
                }
                KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(4),
                KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(4),
                _ => (),
            }
            return None;
        }
        if self.confirmation.is_some() {
            return self.confirm_key(key);
        }
        if let Some(preview) = &mut self.preview {
            match key.code {
                KeyCode::Esc => {
                    self.preview = None;
                    self.status = "Preview dismissed; no writes.".into();
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
                        input: String::new(),
                        removal_ack: None,
                    })
                }
                KeyCode::Enter => {
                    self.status = "Approval disabled: blocked preview or --dry-run.".into()
                }
                _ => (),
            }
            return None;
        }
        if self.shell.overlay.is_some() {
            if key.code == KeyCode::PageDown {
                self.shell.scroll = self.shell.scroll.saturating_add(4);
            }
            if key.code == KeyCode::PageUp {
                self.shell.scroll = self.shell.scroll.saturating_sub(4);
            }
            let count = self.data.as_ref().map_or(0, |data| data.known_roots.len());
            if let Some(intent) = self.shell.key(key, count) {
                return self.intent(intent);
            }
            return None;
        }
        if self.tab == Tab::Git
            && (self.shell.focus == Focus::Content
                || self.git_view.text_entry()
                || self.git_view.sensitive())
        {
            if self.git_view.editing_root
                && key.code == KeyCode::Enter
                && let Err(error) = &self.git
            {
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
        self.git_view.discard_patch();
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
        let opens_overlay = matches!(key.code, KeyCode::Char('?' | ' ' | '/' | 'p' | 'n'))
            || (key.code == KeyCode::Enter && self.shell.focus == Focus::Actions);
        if !matches!(key.code, KeyCode::PageDown | KeyCode::PageUp) && !opens_overlay {
            self.details_scroll = 0;
        }
        match key.code {
            KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(4),
            KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(4),
            KeyCode::Tab | KeyCode::BackTab => {
                self.shell.focus = self.shell.focus.next(
                    key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT),
                );
            }
            KeyCode::Right | KeyCode::Left => {
                self.tab = if key.code == KeyCode::Left {
                    self.tab.previous()
                } else {
                    self.tab.next()
                };
                self.selected = 0;
            }
            KeyCode::Enter if self.shell.focus == Focus::Actions => self.shell.open_actions(false),
            KeyCode::Enter if self.shell.focus == Focus::Destinations => {
                self.tab = self.tab.next();
                self.selected = 0;
            }
            KeyCode::Esc => {
                self.inventory.details = false;
                self.history_details = false;
                self.shell.focus = Focus::Content;
                self.result = None;
            }
            KeyCode::Char(' ') => self.shell.open_actions(false),
            KeyCode::Char('/') if self.tab == Tab::Files && self.shell.focus == Focus::Content => {
                self.inventory.search_mut(self.discovering).editing = true
            }
            KeyCode::Char('/')
                if self.tab == Tab::Recovery && self.shell.focus == Focus::Content =>
            {
                self.history_search.editing = true
            }
            KeyCode::Char('/') => self.shell.open_actions(true),
            KeyCode::Delete if self.tab == Tab::Recovery => {
                self.history_search.query.clear();
                self.clamp_history_selection();
            }
            KeyCode::Char('i') | KeyCode::Enter if self.tab == Tab::Recovery => {
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
            KeyCode::Char('i') if self.tab == Tab::Files => return self.activate(Action::Inspect),
            KeyCode::Char('p') => return self.activate(Action::Choose),
            KeyCode::Char('n') => return self.activate(Action::Init),
            KeyCode::Char('1') => {
                self.tab = Tab::Overview;
                self.selected = 0;
            }
            KeyCode::Char('2') => {
                self.discovering = false;
                self.tab = Tab::Files;
                self.selected = 0;
            }
            KeyCode::Char('3') => {
                self.tab = Tab::Recovery;
                self.selected = 0;
            }
            KeyCode::Char('4') => {
                self.tab = Tab::Checks;
                self.details_scroll = 0;
            }
            KeyCode::Char('5') => return self.activate(Action::Destination(Tab::Git)),
            KeyCode::Char('c') => return self.activate(Action::CheckCurrent),
            KeyCode::Char('v') => return self.activate(Action::CheckStandalone),
            KeyCode::Char('t') => return self.activate(Action::Profiles),
            KeyCode::Char('?') => return self.activate(Action::Help),
            KeyCode::Char('r') => return self.activate(Action::Refresh),
            KeyCode::Char('f') => return self.activate(Action::Discover),
            KeyCode::Char('o') => return self.activate(Action::Adopt),
            KeyCode::Enter if self.tab == Tab::Files && self.discovering => self.adopt_candidate(),
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
            KeyCode::Char('k') if self.tab == Tab::Files => return self.activate(Action::Keep),
            KeyCode::Char('d') if self.tab == Tab::Files => return self.activate(Action::Remove),
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
        if width < 48 || height < 14 {
            self.confirmation = None;
            self.git_view.resize();
        }
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
                    self.status =
                        "Enter an exact mapping ID; filters never limit this input.".into();
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
    fn activate(&mut self, action: Action) -> Option<Task> {
        self.shell.scroll = 0;
        if matches!(self.shell.overlay, Some(Overlay::Form(Form::Adopt)))
            && matches!(action, Action::Choose | Action::Init)
        {
            self.shell.return_to_adopt = true;
        }
        self.shell.overlay = None;
        self.git_view.discard_patch();
        match action {
            Action::GitRoot => {
                self.tab = Tab::Git;
                self.shell.focus = Focus::Content;
                self.git_view.editing_root = true;
            }
            Action::GitDefault => {
                self.git_view.scope_changed();
                self.git_view.root = None;
                self.git_view.follows_files = true;
                return self.git_task();
            }
            Action::GitStatus | Action::GitUnstaged | Action::GitStaged => {
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
                self.shell.focus = Focus::Content;
                self.git_view.confirm_patch();
                self.status = "Patch viewing needs a loaded Unstaged or Staged summary. Type literal view patch in its warning to disclose once.".into();
            }
            Action::Destination(tab) => { self.tab = tab; self.selected = 0; self.shell.focus = Focus::Content; if tab == Tab::Files { self.discovering = false; } }
            Action::Choose => self.shell.overlay = Some(Overlay::Repositories { selected: 0 }),
            Action::Init => self.shell.overlay = Some(Overlay::Form(Form::Init)),
            Action::Discover => {
                self.tab = Tab::Files;
                self.discovering = true;
                self.shell.focus = Focus::Content;
                return Some(Task::Discover);
            }
            Action::Adopt => {
                self.tab = Tab::Files;
                self.shell.overlay = Some(Overlay::Form(Form::Adopt));
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
                self.status = "Recovery display filter cleared; global blockers were never hidden.".into();
            }
            Action::ClearFilter => {
                self.inventory.search_mut(self.discovering).query.clear();
                self.clamp_file_selection();
                self.status = "File display filter cleared; apply scope was never narrowed.".into();
            }
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
            Action::Diagnostics => { self.showing_diagnostics = true; self.tab = Tab::Checks; return Some(Task::Diagnostics); }
            Action::About => self.shell.overlay = Some(Overlay::About),
            Action::Help => self.shell.overlay = Some(Overlay::Help),
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
                        self.status = "Action unavailable for this diagnosis. Inspect details; no automatic recovery. Refresh if history changed.".into();
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
        self.shell.text_entry()
            || (self.tab == Tab::Git && self.git_view.text_entry())
            || (self.tab == Tab::Recovery && self.history_search.editing)
            || (self.shell.overlay.is_none()
                && self.tab == Tab::Files
                && self.inventory.search(self.discovering).editing)
    }
    pub fn file_selection(&self) -> usize {
        if self.discovering {
            self.candidate_selected
        } else {
            self.selected
        }
    }
    pub fn visible_file_rows(&self) -> Vec<usize> {
        let search = self.inventory.search(self.discovering);
        if self.discovering {
            self.discovery
                .as_ref()
                .map(|d| {
                    d.report
                        .candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            search.matches(c.entry.path.to_str().expect("validated candidate"))
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default()
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
        let visible = self.visible_file_rows();
        let old = self.file_selection();
        if !visible.contains(&old)
            && let Some(index) = visible.iter().min_by_key(|&&index| index.abs_diff(old))
        {
            if self.discovering {
                self.candidate_selected = *index;
            } else {
                self.selected = *index;
            }
        }
    }
    fn move_file_selection(&mut self, backwards: bool) {
        let visible = self.visible_file_rows();
        let position = visible
            .iter()
            .position(|&i| i == self.file_selection())
            .unwrap_or(0);
        let next = if backwards {
            position.saturating_sub(1)
        } else {
            position
                .saturating_add(1)
                .min(visible.len().saturating_sub(1))
        };
        if let Some(&index) = visible.get(next) {
            if self.discovering {
                self.candidate_selected = index;
            } else {
                self.selected = index;
            }
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
        let visible = self.visible_history_rows();
        let position = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if backwards {
            position.saturating_sub(1)
        } else {
            position
                .saturating_add(1)
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
        self.tab = Tab::Git;
        self.shell.focus = Focus::Content;
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
            self.status =
                "Choose an exact Git-only root; no manifest or registration required.".into();
            return None;
        };
        match &self.git {
            Ok(executable) => Some(self.git_view.request(root, executable.clone())),
            Err(error) => {
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
        if !self.visible_file_rows().contains(&self.candidate_selected) {
            return;
        }
        if let Some(candidate) = self
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(self.candidate_selected))
        {
            self.shell.adoption.suggest(&candidate.entry.path);
            self.shell.scroll = 0;
            self.shell.overlay = Some(Overlay::Form(Form::Adopt));
        }
    }
    fn row_count(&self) -> usize {
        self.data.as_ref().map_or(0, |data| match self.tab {
            Tab::Files => data.files.len(),
            Tab::Recovery => data.history.len(),
            _ => 0,
        })
    }
    fn confirm_key(&mut self, key: KeyEvent) -> Option<Task> {
        let confirmation = self.confirmation.as_mut()?;
        match key.code {
            KeyCode::Esc => self.confirmation = None,
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
                        confirmation.input.clear();
                    } else {
                        self.status = "Exact file ID does not match; nothing executed.".into();
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
