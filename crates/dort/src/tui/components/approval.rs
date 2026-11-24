use super::ActionButton;
use crate::tui::input::{Hits, Target};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone, Copy)]
pub struct Approval {
    dry_run: bool,
    blocked: bool,
}
impl Approval {
    pub fn new(dry_run: bool, blocked: bool) -> Self {
        Self { dry_run, blocked }
    }

    pub fn allowed(self) -> bool {
        !self.dry_run && !self.blocked
    }

    pub const KEY: &'static str = "y";

    pub fn event() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)
    }

    pub fn accepts(self, key: KeyEvent) -> bool {
        self.allowed()
            && key.code == Self::event().code
            && key.modifiers == KeyModifiers::NONE
            && key.kind == KeyEventKind::Press
    }

    pub fn present(self, hits: &Hits, label: &'static str, target: Target) {
        if self.allowed() {
            hits.present(ActionButton {
                label,
                key: Self::KEY,
                target,
            });
        }
    }

    pub fn present_key(self, hits: &Hits, label: &'static str) {
        self.present(hits, label, Target::Continue);
    }
}
