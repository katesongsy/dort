//! Read-only Git presentation. Patch payloads have no Debug/Serialize implementation and
//! never enter generic reports, errors, progress, retained drafts or metadata inventories.
use super::{actions::Task, shell::edit_text, views::safe};
use dort_core::git::{Diff, Git, Status};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::path::PathBuf;

mod metadata;

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
// Only this module can construct a content-disclosure read, after consuming confirmation.
pub struct Read {
    root: PathBuf,
    executable: PathBuf,
    comparison: Comparison,
    patch: bool,
}
pub enum Report {
    Summary(Metadata),
    Patch(Patch),
}
pub struct Patch {
    text: String,
    lines: Vec<usize>,
}
impl Patch {
    fn new(text: String) -> Self {
        // Backend reports are ASCII-escaped, with LF as the sole raw control byte.
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { text, lines }
    }
    fn viewport(&self, line: usize, column: usize, width: usize, height: usize) -> String {
        self.lines
            .iter()
            .enumerate()
            .skip(line)
            .take(height)
            .map(|(i, &start)| {
                let end = self.lines.get(i + 1).copied().unwrap_or(self.text.len());
                self.text[start..end]
                    .trim_end_matches('\n')
                    .get(column..)
                    .unwrap_or_default()
                    .chars()
                    .take(width)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[cfg(test)]
#[test]
fn patch_viewport_reaches_beyond_terminal_u16_scroll_without_retaining_history() {
    let patch = Patch::new(format!(
        "{}{}END\nLAST",
        "line\n".repeat(70_000),
        "x".repeat(70_000)
    ));
    assert_eq!(patch.viewport(70_000, 70_000, 10, 1), "END");
    assert_eq!(patch.viewport(70_001, 0, 10, 1), "LAST");
}
impl Read {
    pub fn perform(self) -> dort_core::Result<Report> {
        let git = Git::new(self.executable)?;
        if self.comparison == Comparison::Status {
            return git
                .status(&self.root)
                .map(|s| Report::Summary(Metadata::Status(s)));
        }
        let mut diff = git.diff(
            &self.root,
            self.comparison == Comparison::Staged,
            self.patch,
        )?;
        if self.patch {
            Ok(Report::Patch(Patch::new(
                diff.patch.take().unwrap_or_default(),
            )))
        } else {
            Ok(Report::Summary(Metadata::Diff(diff)))
        }
    }
}
#[derive(Default)]
pub struct State {
    pub root: Option<PathBuf>,
    pub follows_files: bool,
    pub comparison: Comparison,
    pub metadata: Option<Metadata>,
    pub draft: String,
    pub editing_root: bool,
    pub search: super::inventory::Search,
    selected: usize,
    scroll: u16,
    patch_scroll: usize,
    horizontal: usize,
    details: bool,
    confirmation: Option<String>,
    patch: Option<Patch>,
    awaiting_patch: bool,
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
        self.editing_root || self.search.editing || self.confirmation.is_some()
    }
    pub fn navigation_input(&self) -> bool {
        self.editing_root || self.confirmation.is_some()
    }
    pub fn sensitive(&self) -> bool {
        self.confirmation.is_some() || self.patch.is_some() || self.awaiting_patch
    }
    pub fn discard_patch(&mut self) {
        self.confirmation = None;
        self.patch = None;
        self.awaiting_patch = false;
        self.scroll = 0;
        self.horizontal = 0;
        self.patch_scroll = 0;
    }
    pub fn failed(&mut self) {
        if self.awaiting_summary {
            self.metadata = None;
        }
        self.awaiting_summary = false;
        self.discard_patch();
    }
    pub fn resize(&mut self) {
        self.discard_patch();
    }
    pub fn scope_changed(&mut self) {
        self.discard_patch();
        self.metadata = None;
        self.selected = 0;
        self.details = false;
    }
    pub fn accept(&mut self, report: Report, visible: bool) {
        match report {
            Report::Summary(metadata) => {
                self.awaiting_summary = false;
                let old = self
                    .metadata
                    .as_ref()
                    .and_then(|m| m.path(self.selected))
                    .map(str::to_owned);
                self.selected = old
                    .and_then(|path| {
                        (0..metadata.len()).find(|&i| metadata.path(i) == Some(path.as_str()))
                    })
                    .unwrap_or(self.selected)
                    .min(metadata.len().saturating_sub(1));
                self.metadata = Some(metadata);
                self.clamp();
            }
            Report::Patch(patch) if self.awaiting_patch && visible => {
                self.awaiting_patch = false;
                self.patch = Some(patch);
                self.scroll = 0;
            }
            Report::Patch(_) => self.discard_patch(),
        }
    }
    pub fn request(&mut self, root: PathBuf, executable: PathBuf) -> Task {
        self.discard_patch();
        if self.root.as_ref() != Some(&root) {
            self.scope_changed();
        }
        self.root = Some(root.clone());
        self.awaiting_summary = true;
        Task::Git(Read {
            root,
            executable,
            comparison: self.comparison,
            patch: false,
        })
    }
    pub fn confirm_patch(&mut self) {
        self.discard_patch();
        if self.comparison != Comparison::Status && self.metadata.is_some() {
            self.confirmation = Some(String::new());
        }
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
        if (self.confirmation.is_some() || self.editing_root)
            && matches!(key.code, KeyCode::PageDown | KeyCode::PageUp)
        {
            self.scroll = if key.code == KeyCode::PageDown {
                self.scroll.saturating_add(3)
            } else {
                self.scroll.saturating_sub(3)
            };
            return Some(None);
        }
        if self.confirmation.is_some() || self.editing_root {
            self.scroll = 0;
        }
        if let Some(input) = &mut self.confirmation {
            match key.code {
                KeyCode::Esc => self.discard_patch(),
                KeyCode::Enter if input == "view patch" => {
                    self.confirmation = None;
                    if let (Some(root), Ok(executable)) = (&self.root, executable) {
                        self.awaiting_patch = true;
                        return Some(Some(Task::Git(Read {
                            root: root.clone(),
                            executable: executable.clone(),
                            comparison: self.comparison,
                            patch: true,
                        })));
                    }
                }
                _ => edit_text(input, key),
            }
            return Some(None);
        }
        if let Some(patch) = &self.patch {
            match key.code {
                KeyCode::Esc => self.discard_patch(),
                KeyCode::Down | KeyCode::PageDown => {
                    self.patch_scroll = self
                        .patch_scroll
                        .saturating_add(if key.code == KeyCode::Down { 1 } else { 8 })
                        .min(patch.lines.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::PageUp => {
                    self.patch_scroll = self
                        .patch_scroll
                        .saturating_sub(if key.code == KeyCode::Up { 1 } else { 8 })
                }
                KeyCode::Right => self.horizontal = self.horizontal.saturating_add(4),
                KeyCode::Left => self.horizontal = self.horizontal.saturating_sub(4),
                _ => {
                    self.discard_patch();
                    return None;
                }
            }
            return Some(None);
        }
        if self.editing_root {
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
        if self.search.editing {
            self.scroll = 0;
            self.search.key(key);
            self.clamp();
            return Some(None);
        }
        match key.code {
            KeyCode::Char('/') => {
                self.search.editing = true;
                self.scroll = 0;
            }
            KeyCode::Delete => self.clear_filter(),
            KeyCode::Up | KeyCode::Down => {
                let visible = self.visible();
                let pos = visible
                    .iter()
                    .position(|&i| i == self.selected)
                    .unwrap_or(0);
                let next = if key.code == KeyCode::Up {
                    pos.saturating_sub(1)
                } else {
                    (pos + 1).min(visible.len().saturating_sub(1))
                };
                if let Some(&i) = visible.get(next) {
                    self.selected = i;
                }
                self.scroll = 0;
            }
            KeyCode::Enter | KeyCode::Char('i') => {
                self.details = true;
                self.scroll = 0;
            }
            KeyCode::Esc => {
                self.details = false;
                self.scroll = 0;
            }
            // The compact metadata detail viewport has three rows. Overlap
            // scroll steps so no wrapped path row can be skipped at the floor.
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
            KeyCode::Home => self.scroll = 0,
            _ => return None,
        }
        Some(None)
    }
    pub fn has_selection(&self) -> bool {
        self.visible().contains(&self.selected)
    }
    pub fn clear_filter(&mut self) {
        self.search.query.clear();
        self.clamp();
    }
    pub fn inspect_metadata(&mut self) {
        if !self.sensitive() && !self.editing_root {
            self.details = true;
        }
    }
    pub fn select_metadata(&mut self, index: usize) {
        if !self.sensitive() && !self.editing_root && self.visible().contains(&index) {
            self.selected = index;
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
        if self.sensitive() || self.editing_root {
            return false;
        }
        super::shell::scroll_details(&mut self.scroll, code)
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, busy: bool, hits: &super::input::Hits) {
        hits.wheel(
            area,
            if self.sensitive() || self.editing_root {
                super::input::Wheel::GitPrivate
            } else {
                super::input::Wheel::Details
            },
        );
        let scope = format!(
            "Git-only root: {:?} ({})\n{}\nr refresh | / metadata search | w View patch content\n",
            self.root,
            if self.follows_files {
                "current file repository"
            } else {
                "independent session choice"
            },
            self.comparison.label()
        );
        if let Some(patch) = &self.patch {
            // Backend has already escaped unsafe bytes. Do not route through generic safe/report paths.
            let heading = format!(
                "{}\n{}",
                super::views::short_field(
                    "Git root",
                    self.root
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("not selected"),
                    area.width
                ),
                self.comparison.label()
            );
            frame.render_widget(
                Paragraph::new(safe(&heading)),
                Rect::new(area.x, area.y, area.width, 2),
            );
            let area = Rect::new(
                area.x,
                area.y + 2,
                area.width,
                area.height.saturating_sub(2),
            );
            frame.render_widget(
                Paragraph::new(patch.viewport(
                    self.patch_scroll,
                    self.horizontal,
                    usize::from(area.width.saturating_sub(2)),
                    usize::from(area.height.saturating_sub(2)),
                ))
                .block(
                    Block::default()
                        .title(" PATCH CONTENT: may contain secrets | Esc drops view ")
                        .borders(Borders::ALL),
                ),
                area,
            );
            return;
        }
        let body = if let Some(input) = &self.confirmation {
            format!(
                "May expose secrets: whole comparison\nType literal view patch; Enter once\n{}\n\n{scope}\nNo history/export. Esc cancels.\nClosing drops reference, not secure memory erasure.\nFull confirmation input: {input:?}",
                super::views::short_field("Confirm", input, area.width)
            )
        } else if self.editing_root {
            format!(
                "Exact Git root (no manifest needed)\nEnter reads; Esc back; PgDn full input\n{}\n\n{scope}\nFull path input: {:?}",
                super::views::short_field("Path", &self.draft, area.width),
                self.draft
            )
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
                    details: self.details,
                    scroll: self.scroll,
                    busy,
                },
            );
            return;
        };
        frame.render_widget(
            Paragraph::new(safe(&body))
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(
                    Block::default()
                        .title(format!(" Git: read-only | {:?} ", self.comparison))
                        .borders(Borders::ALL),
                ),
            area,
        );
    }
}
