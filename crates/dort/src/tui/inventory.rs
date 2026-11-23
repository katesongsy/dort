//! Display-only file filters.
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Default)]
pub struct Search {
    pub query: String,
    pub editing: bool,
}
impl Search {
    pub fn matches(&self, text: &str) -> bool {
        text.to_lowercase().contains(&self.query.to_lowercase())
    }
    pub fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.editing = false,
            _ => super::shell::edit_text(&mut self.query, key),
        }
    }
}

#[derive(Default)]
pub struct Inventory {
    pub tree: std::cell::RefCell<super::discovery_tree::State>,
    candidate_viewport: super::navigation::ListViewport,
    managed_viewport: super::navigation::ListViewport,
    pub managed: Search,
    pub candidates: Search,
    pub details: bool,
    pub mark_refusal: Option<String>,
    pub group: Option<usize>,
    pub candidate_mark_revision: u64,
    pub candidate_marks: std::collections::BTreeSet<std::path::PathBuf>,
    pub managed_marks: std::collections::BTreeSet<String>,
    pub candidate_folded: std::sync::Arc<super::visibility::Folds>,
    pub managed_folded: std::sync::Arc<super::visibility::Folds>,
}
impl Inventory {
    pub fn viewport(&self, discovering: bool) -> &super::navigation::ListViewport {
        if discovering {
            &self.candidate_viewport
        } else {
            &self.managed_viewport
        }
    }

    pub fn search(&self, discovering: bool) -> &Search {
        if discovering {
            &self.candidates
        } else {
            &self.managed
        }
    }
    pub fn search_mut(&mut self, discovering: bool) -> &mut Search {
        if discovering {
            &mut self.candidates
        } else {
            &mut self.managed
        }
    }
}
