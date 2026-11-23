use super::Comparison;
use crate::tui::{
    components as ui,
    input::{Hits, Wheel},
};
use dort_core::git::Git;
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{cell::Cell, path::PathBuf};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Selection {
    pub root: PathBuf,
    pub comparison: Comparison,
    pub path: String,
    pub staged: Option<Change>,
    pub unstaged: Option<Change>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Change {
    File,
    RenameFrom(String),
}

impl Change {
    fn original_path(&self) -> Option<&str> {
        match self {
            Self::File => None,
            Self::RenameFrom(path) => Some(path),
        }
    }
}

pub struct Read {
    selection: Selection,
    token: u64,
    executable: PathBuf,
}

pub struct Report {
    token: u64,
    phase: Phase,
}

impl Read {
    pub fn perform(self) -> Report {
        let phase = match self.patch() {
            Ok(text) if text.is_empty() => Phase::Empty,
            Ok(text) => Phase::Ready(Patch::new(text)),
            Err(_) => Phase::Failed,
        };
        Report {
            token: self.token,
            phase,
        }
    }

    fn patch(&self) -> dort_core::Result<String> {
        let git = Git::new(self.executable.clone())?;
        let file = &self.selection;
        let read = |staged, change: &Option<Change>| {
            change.as_ref().map_or_else(
                || Ok(String::new()),
                |change| git.selected_patch(&file.root, staged, &file.path, change.original_path()),
            )
        };
        let staged = read(true, &file.staged)?;
        let unstaged = read(false, &file.unstaged)?;
        Ok(match (staged.is_empty(), unstaged.is_empty()) {
            (true, _) => unstaged,
            (_, true) => staged,
            _ => format!(
                "Staged (index vs HEAD)\n{staged}\nUnstaged (worktree vs index)\n{unstaged}"
            ),
        })
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Pending,
    Loading(u64),
    Ready(Patch),
    Empty,
    Failed,
}

#[derive(Default)]
pub(super) struct State {
    current: Option<(Selection, Phase)>,
    token: u64,
    scroll: usize,
    horizontal: usize,
    page: Cell<usize>,
}

impl State {
    pub fn sync(&mut self, selection: Option<Selection>) {
        if self.current.as_ref().map(|(key, _)| key) != selection.as_ref() {
            self.current = selection.map(|key| (key, Phase::Pending));
            self.scroll = 0;
            self.horizontal = 0;
        }
    }

    pub fn next(&mut self, executable: PathBuf) -> Option<Read> {
        let (key, phase @ Phase::Pending) = self.current.as_mut()? else {
            return None;
        };
        self.token = self.token.wrapping_add(1);
        *phase = Phase::Loading(self.token);
        Some(Read {
            selection: key.clone(),
            token: self.token,
            executable,
        })
    }

    pub fn accept(&mut self, report: Report) {
        if let Some((_, phase @ Phase::Loading(_))) = &mut self.current
            && matches!(phase, Phase::Loading(token) if *token == report.token)
        {
            *phase = report.phase;
        }
    }

    pub fn available(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|(_, phase)| !matches!(phase, Phase::Empty | Phase::Failed))
    }

    pub fn fallback(&self) -> Option<&'static str> {
        match self.current.as_ref().map(|(_, phase)| phase) {
            Some(Phase::Empty) => Some("No patch for the selected change. Refresh to check again."),
            Some(Phase::Failed) => Some("Could not read the selected patch. Refresh to retry."),
            _ => None,
        }
    }

    pub fn scroll(&mut self, code: KeyCode) -> bool {
        let Some((_, Phase::Ready(patch))) = &self.current else {
            return false;
        };
        let page = self.page.get().max(1);
        let end = patch.lines.len().saturating_sub(page);
        let row = self.scroll.min(end);
        self.scroll = match code {
            KeyCode::Up => row.saturating_sub(1),
            KeyCode::Down => row.saturating_add(1).min(end),
            KeyCode::PageUp => row.saturating_sub(page.saturating_sub(1).max(1)),
            KeyCode::PageDown => row.saturating_add(page.saturating_sub(1).max(1)).min(end),
            KeyCode::Home => {
                self.horizontal = 0;
                0
            }
            KeyCode::End => end,
            KeyCode::Left => {
                self.horizontal = self.horizontal.saturating_sub(4);
                row
            }
            KeyCode::Right => {
                self.horizontal = (self.horizontal + 4).min(patch.max_width);
                row
            }
            _ => return false,
        };
        true
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, hits: &Hits, focused: bool) -> bool {
        let Some((_, phase)) = &self.current else {
            return false;
        };
        if matches!(phase, Phase::Empty | Phase::Failed) {
            return false;
        }
        hits.wheel(area, Wheel::GitPatch);
        let block = ui::pane("Selected patch", focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let height = usize::from(inner.height);
        self.page.set(height);
        match phase {
            Phase::Ready(patch) => frame.render_widget(
                Paragraph::new(patch.viewport(
                    self.scroll.min(patch.lines.len().saturating_sub(height)),
                    self.horizontal,
                    usize::from(inner.width),
                    height,
                )),
                inner,
            ),
            _ => frame.render_widget(Paragraph::new("Reading selected patch..."), inner),
        }
        true
    }
}

struct Patch {
    text: String,
    lines: Vec<usize>,
    max_width: usize,
}

impl Patch {
    fn new(text: String) -> Self {
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .take_while(|&i| i < text.len())
            .collect();
        let max_width = text.lines().map(str::len).max().unwrap_or(0);
        Self {
            text,
            lines,
            max_width,
        }
    }

    fn viewport(
        &self,
        row: usize,
        column: usize,
        width: usize,
        height: usize,
    ) -> Vec<Line<'static>> {
        let digits = row.saturating_add(height).to_string().len().max(5);
        let width = width.saturating_sub(digits + 2);
        self.lines
            .iter()
            .enumerate()
            .skip(row)
            .take(height)
            .map(|(i, &start)| {
                let end = self.lines.get(i + 1).copied().unwrap_or(self.text.len());
                let original = self.text[start..end].trim_end_matches('\n');
                let column = column.min(original.len());
                let text = &original[column..(column + width).min(original.len())];
                let style = if ["diff ", "index ", "---", "+++", "Staged (", "Unstaged ("]
                    .iter()
                    .any(|prefix| original.starts_with(prefix))
                {
                    ui::heading()
                } else if original.starts_with("@@") {
                    ui::accent()
                } else if original.starts_with('+') {
                    ui::success()
                } else if original.starts_with('-') {
                    ui::danger()
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(format!("{:>digits$} │", i + 1), ui::muted()),
                    Span::styled(text.to_owned(), style),
                ])
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_viewport_styles_changes_even_when_scrolled_horizontally() {
        let patch = Patch::new("diff --git a/x b/x\n@@ -1 +1 @@\n-old\n+new\n context".into());
        let lines = patch.viewport(0, 0, 40, 5);
        assert_eq!(lines[0].spans[1].style, ui::heading());
        assert_eq!(lines[1].spans[1].style, ui::accent());
        assert_eq!(lines[2].spans[1].style, ui::danger());
        assert_eq!(lines[3].spans[1].style, ui::success());
        let horizontal = patch.viewport(2, 1, 30, 1);
        assert_eq!(horizontal[0].spans[1].style, ui::danger());
        assert!(horizontal[0].to_string().ends_with("old"));
    }

    #[test]
    fn patch_viewport_reaches_beyond_terminal_u16_scroll() {
        let patch = Patch::new(format!(
            "{}{}END\nLAST",
            "line\n".repeat(70_000),
            "x".repeat(70_000)
        ));
        assert_eq!(
            patch.viewport(70_000, 70_000, 16, 1)[0].spans[1].content,
            "END"
        );
        assert_eq!(patch.viewport(70_001, 0, 16, 1)[0].spans[1].content, "LAST");
    }
}
