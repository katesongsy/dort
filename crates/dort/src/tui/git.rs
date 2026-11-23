//! Read-only Git view. Patch content stays out of reports.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use dort_core::git::{Diff, Git, Status};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
};
use std::path::PathBuf;

mod metadata;
mod preview;
pub use preview::{Read as PreviewRead, Report as PreviewReport};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Comparison {
    #[default]
    Status,
    Unstaged,
    Staged,
}
impl Comparison {
    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Unstaged => "Unstaged (worktree vs index)",
            Self::Staged => "Staged (index vs HEAD; unborn supported)",
        }
    }
}
pub enum Metadata {
    Status(Status),
    Diff(Diff),
}
impl Metadata {
    fn len(&self) -> usize {
        match self {
            Self::Status(status) => status.entries.len(),
            Self::Diff(diff) => diff.entries.len(),
        }
    }
    fn path(&self, index: usize) -> Option<&str> {
        match self {
            Self::Status(status) => status.entries.get(index).map(|e| e.path.as_str()),
            Self::Diff(diff) => diff.entries.get(index).map(|e| e.path.as_str()),
        }
    }
    fn matches(&self, index: usize, search: &super::inventory::Search) -> bool {
        // Retain labeled/escaped metadata search, never parse it for identity or policy.
        let text = match self {
            Self::Status(status) => status.entries.get(index).map(|e| {
                format!(
                    "Kind: {} | XY: {}\nPath: {:?}\nOriginal path: {:?}",
                    e.kind,
                    e.xy.as_deref().unwrap_or("-"),
                    e.path,
                    e.original_path
                )
            }),
            Self::Diff(diff) => diff.entries.get(index).map(|e| {
                format!(
                    "Change: {} | modes: {} -> {}\nPath: {:?}",
                    e.change, e.old_mode, e.new_mode, e.path
                )
            }),
        };
        text.is_some_and(|text| search.matches(&text))
    }
}
pub struct Read {
    root: PathBuf,
    executable: PathBuf,
    comparison: Comparison,
}
impl Read {
    pub fn perform(self) -> dort_core::Result<Metadata> {
        let git = Git::new(self.executable)?;
        match self.comparison {
            Comparison::Status => git.status(&self.root).map(Metadata::Status),
            comparison => git
                .diff(&self.root, comparison == Comparison::Staged, false)
                .map(Metadata::Diff),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Reader {
    #[default]
    Entries,
    Details,
    Patch,
}

#[derive(Default)]
pub struct State {
    pub root: Option<PathBuf>,
    pub follows_files: bool,
    pub comparison: Comparison,
    pub metadata: Option<Metadata>,
    pub draft: String,
    pub editing_root: bool,
    root_details: bool,
    pub search: super::inventory::Search,
    selected: usize,
    pub viewport: super::navigation::ListViewport,
    scroll: u16,
    reader: Reader,
    preview: preview::State,
    awaiting_summary: bool,
}
impl State {
    pub fn new() -> Self {
        Self {
            follows_files: true,
            ..Self::default()
        }
    }
    pub fn text_entry(&self) -> bool {
        (self.editing_root && !self.root_details) || self.search.editing
    }
    pub fn leave_page(&mut self) {
        self.editing_root = false;
        self.root_details = false;
        self.search.editing = false;
        self.reader = Reader::Entries;
        self.discard_preview();
    }
    pub fn discard_preview(&mut self) {
        self.preview.sync(None);
        if self.reader == Reader::Patch {
            self.reader = Reader::Entries;
        }
    }
    pub fn failed(&mut self) {
        if self.awaiting_summary {
            self.metadata = None;
        }
        self.awaiting_summary = false;
        self.discard_preview();
    }
    pub fn scope_changed(&mut self) {
        self.discard_preview();
        self.metadata = None;
        self.selected = 0;
        self.reader = Reader::Entries;
        self.scroll = 0;
    }
    pub fn accept(&mut self, metadata: Metadata) {
        self.awaiting_summary = false;
        let old = self.metadata.as_ref().and_then(|m| m.path(self.selected));
        self.selected = old
            .and_then(|path| (0..metadata.len()).find(|&i| metadata.path(i) == Some(path)))
            .unwrap_or(self.selected)
            .min(metadata.len().saturating_sub(1));
        self.metadata = Some(metadata);
        self.clamp();
    }
    pub fn request(&mut self, root: PathBuf, executable: PathBuf) -> Task {
        self.discard_preview();
        if self.root.as_ref() != Some(&root) {
            self.scope_changed();
        }
        self.root = Some(root.clone());
        self.awaiting_summary = true;
        Task::Git(Read {
            root,
            executable,
            comparison: self.comparison,
        })
    }
    fn selection(&self) -> Option<preview::Selection> {
        if self.awaiting_summary || self.editing_root {
            return None;
        }
        let metadata = self.metadata.as_ref()?;
        let path = metadata.path(self.selected)?;
        if !self.search.query.is_empty() && !metadata.matches(self.selected, &self.search) {
            return None;
        }
        let (staged, unstaged) = match metadata {
            Metadata::Status(status) => {
                let row = status.entries.get(self.selected)?;
                let [index, worktree] = row.xy.as_ref()?.as_bytes() else {
                    return None;
                };
                let change = |code| match code {
                    b'.' | b'?' | b'!' => None,
                    b'R' => Some(
                        row.original_path
                            .clone()
                            .map_or(preview::Change::File, preview::Change::RenameFrom),
                    ),
                    _ => Some(preview::Change::File),
                };
                (change(*index), change(*worktree))
            }
            Metadata::Diff(diff) if diff.staged => (Some(preview::Change::File), None),
            Metadata::Diff(_) => (None, Some(preview::Change::File)),
        };
        if staged.is_none() && unstaged.is_none() {
            return None;
        }
        Some(preview::Selection {
            root: self.root.clone()?,
            comparison: self.comparison,
            path: path.to_owned(),
            staged,
            unstaged,
        })
    }
    pub fn sync_preview(&mut self, enabled: bool) {
        if let Some(selection) = enabled.then(|| self.selection()).flatten() {
            self.preview.sync(Some(selection));
        } else {
            self.discard_preview();
        }
    }
    pub fn next_preview(&mut self, executable: PathBuf) -> Option<Task> {
        self.preview.next(executable).map(Task::GitPreview)
    }
    pub fn accept_preview(&mut self, report: PreviewReport) {
        self.preview.accept(report);
    }
    pub fn choose(&mut self, comparison: Comparison) {
        if self.comparison != comparison {
            self.scope_changed();
            self.comparison = comparison;
        }
    }
    fn visible(&self) -> Vec<usize> {
        self.metadata
            .as_ref()
            .map(|m| {
                (0..m.len())
                    .filter(|&i| m.matches(i, &self.search))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn clamp(&mut self) {
        let visible = self.visible();
        if !visible.contains(&self.selected) {
            self.selected = visible.first().copied().unwrap_or(0);
        }
    }
    // None means the outer shell handles the key; Some(None) consumes it locally.
    pub fn key(
        &mut self,
        key: KeyEvent,
        launch: &std::path::Path,
        executable: &Result<PathBuf, String>,
    ) -> Option<Option<Task>> {
        if self.editing_root && self.root_details {
            match key.code {
                KeyCode::Esc | KeyCode::F(3) => self.root_details = false,
                KeyCode::Home => self.scroll = 0,
                KeyCode::Down | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
                KeyCode::Up | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
                _ => (),
            }
            return Some(None);
        }
        if self.editing_root {
            if key.code == KeyCode::F(3) {
                self.root_details = true;
                self.scroll = 0;
                return Some(None);
            }
            match key.code {
                KeyCode::Esc => self.editing_root = false,
                KeyCode::Enter if !self.draft.is_empty() => {
                    let root =
                        crate::services::resolve_root(std::path::Path::new(&self.draft), launch);
                    self.scope_changed();
                    self.root = Some(root.clone());
                    self.follows_files = false;
                    self.editing_root = false;
                    if let Ok(executable) = executable {
                        return Some(Some(self.request(root, executable.clone())));
                    }
                }
                _ => edit_text(&mut self.draft, key),
            }
            return Some(None);
        }
        if key.code == KeyCode::F(3) {
            self.search.editing = false;
            self.reader = if self.reader == Reader::Details {
                Reader::Entries
            } else {
                Reader::Details
            };
            self.scroll = 0;
            return Some(None);
        }
        if self.search.editing {
            self.scroll = 0;
            self.search.key(key);
            self.clamp();
            return Some(None);
        }
        if self.reader != Reader::Entries {
            match key.code {
                KeyCode::Esc => {
                    self.reader = Reader::Entries;
                    self.scroll = 0;
                    return Some(None);
                }
                KeyCode::Enter => return Some(None),
                code if self.reader == Reader::Patch && self.preview.scroll(code) => {
                    return Some(None);
                }
                KeyCode::Left | KeyCode::Right if self.reader == Reader::Patch => {
                    return Some(None);
                }
                code if super::shell::scroll_details(&mut self.scroll, code) => return Some(None),
                _ => (),
            }
        }
        match key.code {
            KeyCode::Char('/') => self.edit_filter(),
            KeyCode::Delete => self.clear_filter(),
            KeyCode::Up | KeyCode::Down => self.move_metadata(key.code == KeyCode::Up),
            KeyCode::Enter => {
                if self.metadata.is_none() {
                    self.editing_root = true;
                } else if self.has_selection() {
                    self.reader = if self.preview.available() {
                        Reader::Patch
                    } else {
                        Reader::Details
                    };
                }
                self.scroll = 0;
            }
            _ => return None,
        }
        Some(None)
    }
    pub fn root_metadata(&self) -> bool {
        self.editing_root && self.root_details
    }
    pub fn browsing_details(&self) -> bool {
        self.reader == Reader::Details
    }
    pub fn browsing_entries(&self) -> bool {
        self.reader == Reader::Entries && !self.editing_root
    }
    pub fn page_metadata(&mut self, backwards: bool, count: usize) {
        let visible = self.visible();
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if backwards {
            pos.saturating_sub(count)
        } else {
            pos.saturating_add(count)
                .min(visible.len().saturating_sub(1))
        };
        if let Some(&index) = visible.get(next) {
            self.select_metadata(index);
        }
    }
    pub fn has_selection(&self) -> bool {
        self.visible().contains(&self.selected)
    }
    pub fn clear_filter(&mut self) {
        self.search.query.clear();
        self.clamp();
    }
    pub fn edit_filter(&mut self) {
        self.search.editing = true;
        self.reader = Reader::Entries;
        self.scroll = 0;
    }
    pub fn inspect_metadata(&mut self) {
        if !self.editing_root {
            self.reader = Reader::Details;
            self.scroll = 0;
        }
    }
    pub fn select_metadata(&mut self, index: usize) {
        if !self.editing_root && self.visible().contains(&index) {
            self.selected = index;
            self.reader = Reader::Entries;
            self.scroll = 0;
        }
    }
    pub fn move_metadata(&mut self, backwards: bool) {
        let visible = self.visible();
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if backwards {
            pos.saturating_sub(1)
        } else {
            (pos + 1).min(visible.len().saturating_sub(1))
        };
        if let Some(&index) = visible.get(next) {
            self.select_metadata(index);
        }
    }
    pub fn scroll_metadata_details(&mut self, code: KeyCode) -> bool {
        super::shell::scroll_details(&mut self.scroll, code)
    }
    pub fn scroll_patch(&mut self, code: KeyCode) {
        self.preview.scroll(code);
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, busy: bool, hits: &super::input::Hits) {
        if self.editing_root {
            use ratatui::crossterm::event::KeyModifiers;
            hits.present(ui::ActionButton {
                label: if self.root_details {
                    "Fields"
                } else {
                    "Full draft"
                },
                key: "F3",
                target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            });
            if self.root_details {
                ui::report(frame, area, "Git root draft", vec![ui::metadata("Full root draft", &self.draft), ratatui::text::Line::raw("Relative paths resolve from the launch directory. This draft is not saved and shows no patch content.")].into(), self.scroll, false);
                hits.present(ui::ActionButton {
                    label: "Back",
                    key: "Esc",
                    target: Target::Back,
                });
                return;
            }
            ui::field_page(
                frame,
                area,
                &[("Exact Git root", &self.draft)],
                0,
                hits,
                |_| Target::Key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE)),
                ui::FormActions {
                    choices: &[],
                    buttons: &[
                        ui::ActionButton {
                            label: "Read Git",
                            key: "Enter",
                            target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        },
                        ui::ActionButton {
                            label: "Back",
                            key: "Esc",
                            target: Target::Back,
                        },
                    ],
                    feedback: Some(ui::Feedback {
                        text: "This selection applies to this session only.",
                        tone: ui::design::Tone::Information,
                    }),
                },
            );
        } else {
            let visible = self.visible();
            metadata::draw(
                frame,
                area,
                metadata::View {
                    hits,
                    root: self.root.as_deref(),
                    follows_files: self.follows_files,
                    comparison: self.comparison,
                    metadata: self.metadata.as_ref(),
                    search: &self.search,
                    visible: &visible,
                    selected: self.selected,
                    viewport: &self.viewport,
                    reader: self.reader,
                    preview: &self.preview,
                    scroll: self.scroll,
                    busy,
                },
            );
        }
    }
}
