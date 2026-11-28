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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Managed,
    Discover,
    Generated,
}
impl Page {
    pub fn location(self) -> &'static str {
        match self {
            Self::Managed => "Files / Managed",
            Self::Discover => "Files / Discover",
            Self::Generated => "Files / Generated",
        }
    }
}

#[derive(Default)]
pub struct Inventory {
    pub tree: std::cell::RefCell<super::discovery_tree::State>,
    candidate_viewport: super::navigation::ListViewport,
    managed_viewport: super::navigation::ListViewport,
    generated_viewport: super::navigation::ListViewport,
    pub managed: Search,
    pub candidates: Search,
    generated: Search,
    pub generated_selected: usize,
    pub saved_template: Option<String>,
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
    pub fn viewport(&self, page: Page) -> &super::navigation::ListViewport {
        match page {
            Page::Managed => &self.managed_viewport,
            Page::Discover => &self.candidate_viewport,
            Page::Generated => &self.generated_viewport,
        }
    }

    pub fn search(&self, page: Page) -> &Search {
        match page {
            Page::Managed => &self.managed,
            Page::Discover => &self.candidates,
            Page::Generated => &self.generated,
        }
    }

    pub fn search_mut(&mut self, page: Page) -> &mut Search {
        match page {
            Page::Managed => &mut self.managed,
            Page::Discover => &mut self.candidates,
            Page::Generated => &mut self.generated,
        }
    }
}
