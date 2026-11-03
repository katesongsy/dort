use super::{
    actions::{Data, Output, Pending, Task},
    shell::{Action, Focus, Form, Intent, Overlay, Shell},
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
}
impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Files,
            Self::Files => Self::Recovery,
            Self::Recovery => Self::Overview,
        }
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
    pub result: Option<String>,
    pub data: Option<Data>,
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
            result: None,
            data: None,
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
        match result {
            Err(error) => {
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
                self.error = Some(self.status.clone());
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
            Ok(Output::Selected(repository, data)) => {
                self.origin = if repository.is_some() {
                    "session choice"
                } else {
                    "registered active"
                };
                self.repository = repository;
                self.result = None;
                self.data = None;
                self.selected = 0;
                self.details_scroll = 0;
                self.preview = None;
                self.confirmation = None;
                self.shell.overlay = None;
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
                Err(error) => self.status = error.to_string(),
            },
            Ok(Output::Completed(completion)) => {
                if let Some(root) = completion.repository {
                    self.repository = Some(root);
                    self.origin = "approved create/attach";
                    self.tab = Tab::Overview;
                    self.data = None;
                    self.selected = 0;
                    self.details_scroll = 0;
                }
                self.shell.overlay = None;
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
                && (self.preview.is_some() || !self.shell.text_entry()))
        {
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
                self.preview = None;
                self.shell.overlay = None;
                self.error = None;
            }
            self.status =
                "Resize to at least 48x14; no approval is accepted in a small layout.".into();
            return None;
        }
        if self.busy {
            if key.code == KeyCode::Char('?') {
                self.shell.scroll = 0;
                self.shell.overlay = Some(Overlay::Help);
            }
            if key.code == KeyCode::Esc {
                self.shell.overlay = None;
            }
            if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                self.tab = if key.code == KeyCode::Left {
                    self.tab.next().next()
                } else {
                    self.tab.next()
                };
                self.selected = 0;
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
                KeyCode::Esc => {
                    self.error = None;
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
                    self.tab.next().next()
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
                self.shell.focus = Focus::Content;
                self.result = None;
            }
            KeyCode::Char(' ') => self.shell.open_actions(false),
            KeyCode::Char('/') => self.shell.open_actions(true),
            KeyCode::Char('p') => return self.activate(Action::Choose),
            KeyCode::Char('n') => return self.activate(Action::Init),
            KeyCode::Char('1') => {
                self.tab = Tab::Overview;
                self.selected = 0;
            }
            KeyCode::Char('2') => {
                self.tab = Tab::Files;
                self.selected = 0;
            }
            KeyCode::Char('3') => {
                self.tab = Tab::Recovery;
                self.selected = 0;
            }
            KeyCode::Char('?') => return self.activate(Action::Help),
            KeyCode::Char('r') => return Some(Task::Refresh),
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
        None
    }
    pub fn resize(&mut self, width: u16, height: u16) {
        if width < 48 || height < 14 {
            self.confirmation = None;
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
        self.data = Some(data);
        self.selected = position
            .unwrap_or(self.selected)
            .min(self.row_count().saturating_sub(1));
    }
    fn intent(&mut self, intent: Intent) -> Option<Task> {
        match intent {
            Intent::Action(action) => {
                self.shell.overlay = None;
                self.activate(action)
            }
            Intent::Active => Some(Task::Select(None)),
            Intent::Known(index) => self
                .data
                .as_ref()?
                .known_roots
                .get(index)
                .map(|root| Task::Select(Some(root.clone()))),
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
        match action {
            Action::Destination(tab) => { self.tab = tab; self.selected = 0; self.shell.focus = Focus::Content; }
            Action::Choose => self.shell.overlay = Some(Overlay::Repositories { selected: 0 }),
            Action::Init => self.shell.overlay = Some(Overlay::Form(Form::Init)),
            Action::Refresh => return Some(Task::Refresh),
            Action::PreviewOnly => {
                if self.preview_only_locked { self.status = "--dry-run locks this session to preview-only.".into(); }
                else { self.dry_run = !self.dry_run; self.status = "Preview-only mode changed; approval always requires a fresh preview.".into(); }
            }
            Action::Help => self.shell.overlay = Some(Overlay::Help),
            Action::Quit => self.quit = true,
            Action::Apply if self.data.as_ref().and_then(|d| d.repository.as_ref()).is_some() => return Some(Task::Files(Request::Apply)),
            Action::Keep | Action::Remove if self.tab == Tab::Files => {
                if let Some((id, _)) = self.data.as_ref().and_then(|data| data.files.get(self.selected)) {
                    return Some(Task::Files(Request::Unmanage {
                        id: id.clone(), mode: if matches!(action, Action::Keep) { UnmanageMode::Keep } else { UnmanageMode::Remove },
                    }));
                }
                self.status = "Choose a managed mapping in Files first.".into();
            }
            Action::Rollback | Action::Resume | Action::Abandon if self.tab == Tab::Recovery => {
                if let Some(row) = self.data.as_ref().and_then(|data| data.history.get(self.selected)) {
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
        None
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
                        Err(error) => self.status = format!("Approval refused: {error}"),
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
