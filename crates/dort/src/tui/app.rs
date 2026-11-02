use super::actions::{Data, Output, Pending, Task};
use dort_core::{
    bootstrap::RecoveryAction,
    planner::{Request, UnmanageMode},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Files,
    Recovery,
    Help,
}
impl Tab {
    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Files,
            Self::Files => Self::Recovery,
            Self::Recovery => Self::Help,
            Self::Help => Self::Overview,
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
    pub status: String,
    pub error: Option<String>,
}
impl App {
    pub fn new(dry_run: bool) -> Self {
        Self {
            tab: Tab::Overview,
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
        self.busy = false;
        self.writing = false;
        if self.exit_requested {
            self.quit = true;
        }
        match result {
            Err(error) => {
                self.status = format!(
                    "Error: {error}\nA failed write may leave partial effects. Inspect Recovery; nothing is automatically undone."
                );
                self.error = Some(self.status.clone());
            }
            Ok(Output::Data(data)) => {
                self.data = Some(data);
                self.selected = 0;
                // Preserve execution errors/completion until the next explicit action.
                if self.status == "Reading..." {
                    self.status = "Read-only observations loaded.".into();
                } else if self.status.starts_with("Operation completed.") {
                    self.status = "Operation completed. Observations refreshed.".into();
                }
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
            Ok(Output::Completed) => {
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
        if interrupt || (key.code == KeyCode::Char('q') && self.confirmation.is_none()) {
            if self.busy {
                self.exit_requested = true;
                self.status = "Exit requested; waiting for the active worker. Approved writes are not cancelled.".into();
            } else {
                self.quit = true;
            }
            return None;
        }
        if width < 48 || height < 14 {
            self.status =
                "Resize to at least 48x14; no approval is accepted in a small layout.".into();
            return None;
        }
        if self.busy {
            if key.code == KeyCode::Tab {
                self.tab = self.tab.next();
                self.selected = 0;
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
        if !matches!(key.code, KeyCode::PageDown | KeyCode::PageUp) {
            self.details_scroll = 0;
        }
        match key.code {
            KeyCode::PageDown => self.details_scroll = self.details_scroll.saturating_add(4),
            KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(4),
            KeyCode::Tab => {
                self.tab = self.tab.next();
                self.selected = 0;
            }
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
            KeyCode::Char('?') => self.tab = Tab::Help,
            KeyCode::Char('r') => return Some(Task::Refresh),
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.row_count().saturating_sub(1))
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('a') if self.tab == Tab::Files || self.tab == Tab::Overview => {
                return Some(Task::Files(Request::Apply));
            }
            KeyCode::Char('k' | 'd') if self.tab == Tab::Files => {
                if let Some((id, _)) = self.data.as_ref().and_then(|d| d.files.get(self.selected)) {
                    return Some(Task::Files(Request::Unmanage {
                        id: id.clone(),
                        mode: if key.code == KeyCode::Char('k') {
                            UnmanageMode::Keep
                        } else {
                            UnmanageMode::Remove
                        },
                    }));
                }
            }
            KeyCode::Char('b' | 's' | 'x') if self.tab == Tab::Recovery => {
                if let Some(row) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.history.get(self.selected))
                {
                    let (r, t) = (row.repository_id.clone(), row.transaction_id.clone());
                    return Some(match key.code {
                        KeyCode::Char('b') => Task::Rollback(r, t),
                        KeyCode::Char('s') => Task::Bootstrap(r, t, RecoveryAction::Resume),
                        _ => Task::Bootstrap(r, t, RecoveryAction::Abandon),
                    });
                }
            }
            _ => (),
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
