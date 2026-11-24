//! Project one approval control into the existing keyboard and pointer dispatch.
use super::ActionButton;
use crate::tui::input::{Hits, Target};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

    pub fn present(self, hits: &Hits, label: &'static str, target: Target) {
        if self.allowed() {
            hits.present(ActionButton {
                label,
                key: "Enter",
                target,
            });
        }
    }

    pub fn present_key(self, hits: &Hits, label: &'static str) {
        self.present(
            hits,
            label,
            Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
    }
}
