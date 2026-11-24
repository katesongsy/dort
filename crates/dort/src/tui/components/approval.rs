use super::{ActionButton, reader::ReviewStage};
use crate::tui::input::{Hits, Target};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub enum Approval {
    Disabled,
    Unread,
    Ready,
}
impl Approval {
    pub fn review(hits: &Hits, stage: ReviewStage, dry_run: bool, blocked: bool) -> Self {
        if dry_run || blocked {
            Self::Disabled
        } else if hits.review_tail_drawn(stage) {
            Self::Ready
        } else {
            Self::Unread
        }
    }
    pub const KEY: &'static str = "y";
    pub fn event() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)
    }
    pub fn accepts(self, key: KeyEvent) -> bool {
        matches!(self, Self::Ready)
            && key.code == Self::event().code
            && key.modifiers == KeyModifiers::NONE
            && key.kind == KeyEventKind::Press
    }
    pub fn present_key(self, hits: &Hits, label: &'static str) {
        let action = match self {
            Self::Disabled => return,
            Self::Unread => ActionButton {
                label: "Read remaining effects",
                key: "End",
                target: Target::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
            },
            Self::Ready => ActionButton {
                label,
                key: Self::KEY,
                target: Target::Continue,
            },
        };
        hits.present(action);
    }
}
