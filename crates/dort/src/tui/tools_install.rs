//! Install records and record close. Closing does not replay an install.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use dort_core::tools::{inspection::NpmBinding, install};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::Line,
};
use std::path::PathBuf;
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Attempts,
    CloseInputs,
    LoadingClose,
    CloseReview,
}
#[derive(Default)]
pub struct State {
    pub mode: Mode,
    pub close_preview: Option<install::ClosePreview>,
    pub close_request: Option<install::CloseRequest>,
    pub attempts: Vec<install::Attempt>,
    pub error: Option<String>,
    pub selected: usize,
    viewport: super::navigation::ListViewport,
    pub fields: [String; 4],
    pub close_npm: bool,
    pub close_brew: bool,
    drafts: [[String; 4]; 2],
    pub focus: usize,
    pub details: bool,
    pub scroll: u16,
    pub scope: Option<install::Scope>,
}
pub enum Outcome {
    Back,
    Task(Box<Task>),
}
impl State {
    pub fn clear_approvals(&mut self) {
        self.close_preview = None;
        if matches!(self.mode, Mode::LoadingClose | Mode::CloseReview) {
            self.mode = Mode::Attempts;
        }
    }
    pub fn text_entry(&self) -> bool {
        self.mode == Mode::CloseInputs
    }
    pub fn open_attempts(&mut self) -> Task {
        self.clear_approvals();
        self.mode = Mode::Attempts;
        self.details = false;
        self.error = None;
        Task::ToolAttempts
    }
    pub fn attempts_ready(&mut self, result: Result<Vec<install::Attempt>, String>) {
        self.attempts.clear();
        match result {
            Ok(attempts) => {
                self.attempts = attempts;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.mode = Mode::Attempts;
        self.selected = self.selected.min(self.attempts.len().saturating_sub(1));
        self.scroll = 0;
    }
    pub fn preparation_failed(&mut self, error: &str) {
        self.clear_approvals();
        self.error = Some(error.into());
    }
    pub fn retry_preview(&mut self) -> Option<Task> {
        let request = self.close_request.clone()?;
        self.mode = Mode::LoadingClose;
        Some(Task::ToolClosePrepare(request))
    }
    fn prepare_close(&mut self) -> Option<Task> {
        let attempt = self.attempts.get(self.selected)?;
        if !attempt.unresolved {
            return None;
        }
        self.scope = attempt.scope.clone();
        let request = install::CloseRequest {
            id: attempt.id.clone(),
            executable: None,
            search_dirs: vec![],
            npm_binding: None,
            brew_prefix: None,
        };
        self.close_request = Some(request.clone());
        self.error = None;
        if attempt.scope.is_some() {
            self.mode = Mode::LoadingClose;
            Some(Task::ToolClosePrepare(request))
        } else {
            if !self.close_npm && !self.close_brew {
                self.close_brew = true;
            }
            self.mode = Mode::CloseInputs;
            self.focus = 0;
            None
        }
    }
    pub(super) fn close_field_count(&self) -> usize {
        if self.close_npm {
            4
        } else if self.close_brew {
            3
        } else {
            2
        }
    }
    pub(super) fn closure_provider(&self) -> usize {
        if self.close_npm { 0 } else { 1 }
    }
    pub(super) fn choose_closure_provider(&mut self, selected: usize) {
        if selected >= 2 || self.mode != Mode::CloseInputs {
            return;
        }
        self.drafts[self.closure_provider()] = self.fields.clone();
        self.close_npm = selected == 0;
        self.close_brew = selected == 1;
        self.fields = self.drafts[selected].clone();
        self.focus = 0;
    }
    pub fn close_reviewed(&mut self, preview: install::ClosePreview) {
        self.close_preview = Some(preview);
        self.mode = Mode::CloseReview;
        self.scroll = 0;
    }
    pub fn closed(&mut self, result: Result<install::Attempt, String>) {
        self.clear_approvals();
        self.mode = Mode::Attempts;
        self.details = true;
        match result {
            Ok(attempt) => {
                if let Some(old) = self.attempts.iter_mut().find(|a| a.id == attempt.id) {
                    *old = attempt;
                }
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn submit_fields(&mut self) -> Option<Task> {
        let count = self.close_field_count();
        if let Some(i) = self.fields[..count].iter().position(String::is_empty) {
            self.focus = i;
            self.error = Some("Complete the package manager settings first.".into());
            return None;
        }
        let request = self.close_request.as_mut()?;
        request.executable = Some(PathBuf::from(&self.fields[0]));
        request.search_dirs = self.fields[1]
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        request.brew_prefix = self.close_brew.then(|| PathBuf::from(&self.fields[2]));
        request.npm_binding = self.close_npm.then(|| NpmBinding {
            runtime_tool: "historical-runtime".into(),
            node: PathBuf::from(&self.fields[0]),
            npm_cli: PathBuf::from(&self.fields[2]),
            prefix: PathBuf::from(&self.fields[3]),
        });
        self.mode = Mode::LoadingClose;
        Some(Task::ToolClosePrepare(request.clone()))
    }
    pub fn key(&mut self, key: KeyEvent, dry_run: bool) -> Option<Outcome> {
        if self.mode == Mode::CloseInputs {
            let n = self.close_field_count();
            let task = match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Attempts;
                    None
                }
                KeyCode::Tab => {
                    self.focus = (self.focus + 1) % n;
                    None
                }
                KeyCode::BackTab => {
                    self.focus = (self.focus + n - 1) % n;
                    None
                }
                KeyCode::Enter => self.submit_fields(),
                _ => {
                    edit_text(&mut self.fields[self.focus.min(n - 1)], key);
                    None
                }
            };
            return task.map(|t| Outcome::Task(Box::new(t)));
        }
        if self.mode == Mode::CloseReview {
            match key.code {
                KeyCode::Esc => self.clear_approvals(),
                KeyCode::Enter if ui::approval::Approval::new(dry_run, false).allowed() => {
                    return self
                        .close_preview
                        .take()
                        .map(|p| Outcome::Task(Box::new(Task::ToolClose(Box::new(p.approve())))));
                }
                _ => self.scroll(key.code),
            }
            return None;
        }
        if self.details {
            match key.code {
                KeyCode::Esc => {
                    self.details = false;
                    self.scroll = 0;
                }
                KeyCode::Char('x') => {
                    return self.prepare_close().map(|t| Outcome::Task(Box::new(t)));
                }
                _ => self.scroll(key.code),
            }
            return None;
        }
        match key.code {
            KeyCode::Esc => return Some(Outcome::Back),
            KeyCode::Enter if !self.attempts.is_empty() => {
                self.details = true;
                self.scroll = 0;
            }
            KeyCode::Char('r') => return Some(Outcome::Task(Box::new(self.open_attempts()))),
            KeyCode::Char('x') => return self.prepare_close().map(|t| Outcome::Task(Box::new(t))),
            KeyCode::Down | KeyCode::PageDown => {
                let count = if key.code == KeyCode::Down {
                    1
                } else {
                    self.viewport.page()
                };
                self.selected = (self.selected + count).min(self.attempts.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::PageUp => {
                let count = if key.code == KeyCode::Up {
                    1
                } else {
                    self.viewport.page()
                };
                self.selected = self.selected.saturating_sub(count);
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.attempts.len().saturating_sub(1),
            _ => (),
        }
        None
    }
    fn scroll(&mut self, code: KeyCode) {
        ui::reader::ScrollKeys::new(2, 2)
            .with_end()
            .apply(&mut self.scroll, code);
    }
    fn draw_attempt(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let mut lines = vec![];
        if let Some(attempt) = self.attempts.get(self.selected) {
            let s = attempt.summary();
            lines.extend([
                ui::metadata("Record", &attempt.id),
                ui::metadata("Status", s.status.label()),
                Line::raw(s.explanation),
                Line::raw(s.package_note()),
                Line::raw(s.binary_note),
            ]);
            if let Some(scope) = &attempt.scope {
                lines.push(ui::metadata("Tool", &scope.tool));
                lines.push(ui::metadata(
                    "Package",
                    &format!("{} {}", scope.package, scope.version),
                ));
                lines.push(ui::metadata(
                    "Program",
                    &scope.executable.display().to_string(),
                ));
                lines.push(ui::metadata(
                    "Repository",
                    &scope.repository.display().to_string(),
                ));
            }
            lines.push(ui::metadata(
                "Record closed",
                if attempt.explicitly_closed {
                    "Yes; this does not prove installation succeeded"
                } else {
                    "No"
                },
            ));
        }
        if let Some(e) = &self.error {
            lines.push(ui::metadata("Problem", e));
        }
        ui::reader::Reader::new(hits)
            .with_wheel(super::input::Wheel::Tools)
            .report(
                frame,
                area,
                "Installation details",
                lines.into(),
                self.scroll,
                false,
            );
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        let action = |label, key, code| {
            hits.present(ui::ActionButton {
                label,
                key,
                target: Target::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            })
        };
        if self.mode == Mode::CloseInputs {
            let labels = if self.close_npm {
                [
                    "Node program",
                    "Command search directories",
                    "npm CLI script",
                    "Global prefix",
                ]
            } else {
                [
                    "Package manager program",
                    "Command search directories",
                    "Homebrew prefix",
                    "",
                ]
            };
            let fields = labels
                .iter()
                .zip(&self.fields)
                .take(self.close_field_count())
                .map(|(a, b)| (*a, b.as_str()))
                .collect::<Vec<_>>();
            ui::field_page(frame,area,&fields,self.focus,hits,|i|Target::ToolInstallControl(i,false),ui::FormActions {choices:&[],buttons:&[ui::ActionButton {label:"Review close",key:"Enter",target:Target::ToolInstallControl(4,true)},ui::ActionButton {label:"Change package manager",key:"F4",target:Target::ToolInstallProvider},ui::ActionButton {label:"Back",key:"Esc",target:Target::Back}],feedback:Some(ui::Feedback {text:self.error.as_deref().unwrap_or("The original record is damaged. Choose the package manager and its paths for a fresh check. Nothing is retried."),tone:ui::design::Tone::Attention})});
            return;
        }
        if let Some(preview) = &self.close_preview {
            let r = preview.review();
            let mut lines = vec![
                Line::raw("Close installation record"),
                ui::metadata("Record", r.attempt_id),
                ui::metadata("Program", &r.query.executable.display().to_string()),
                ui::metadata("Arguments", &r.query.arguments.join(" ")),
                ui::metadata("HOME", &r.query.home.display().to_string()),
                ui::metadata(
                    "Search path",
                    &r.query
                        .trusted_search_path
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(":"),
                ),
                Line::raw(r.explanation),
                Line::raw(r.query.warning),
            ];
            if let Some(binding) = &r.query.npm_binding {
                lines.push(ui::metadata("Node", &binding.node.display().to_string()));
                lines.push(ui::metadata(
                    "npm CLI",
                    &binding.npm_cli.display().to_string(),
                ));
                lines.push(ui::metadata(
                    "Prefix",
                    &binding.prefix.display().to_string(),
                ));
            }
            if let Some(prefix) = r.brew_prefix {
                lines.push(ui::metadata(
                    "Homebrew prefix",
                    &prefix.display().to_string(),
                ));
            }
            for args in &r.fresh_probes {
                lines.push(ui::metadata("Fresh probe arguments", &args.join(" ")));
            }
            ui::reader::Reader::new(hits)
                .with_wheel(super::input::Wheel::Tools)
                .report(
                    frame,
                    area,
                    "Close installation record",
                    lines.into(),
                    self.scroll,
                    false,
                );
            ui::approval::Approval::new(dry_run, false).present_key(hits, "Confirm close");
            action("Cancel", "Esc", KeyCode::Esc);
            return;
        }
        let selected = self.attempts.get(self.selected);
        if self.details {
            self.draw_attempt(frame, area, hits);
            if selected.is_some_and(|a| a.unresolved) {
                action("Close record", "x", KeyCode::Char('x'));
            }
            action("Back", "Esc", KeyCode::Esc);
            return;
        }
        let (list, details) = ui::reader::detail_panes(area, ui::reader::wide_details(area, 0));
        let capacity = ui::design::pair_capacity(list);
        let first = self.viewport.start(
            self.attempts.len(),
            capacity,
            selected.map(|_| self.selected),
        );
        let rows = self
            .attempts
            .iter()
            .enumerate()
            .skip(first)
            .take(capacity)
            .map(|(index, attempt)| {
                (
                    index,
                    attempt
                        .scope
                        .as_ref()
                        .map_or("Unknown tool", |s| s.tool.as_str()),
                    attempt.summary().status.label(),
                )
            })
            .map(|(identity, tool, status)| ui::design::PairRow {
                identity,
                values: [tool, status],
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            ui::design::notice(
                frame,
                area,
                if self.error.is_some() {
                    "History unavailable"
                } else {
                    "No installation history"
                },
                self.error
                    .as_deref()
                    .unwrap_or("Install records appear here after an approved install."),
                if self.error.is_some() {
                    ui::design::Tone::Attention
                } else {
                    ui::design::Tone::Neutral
                },
            );
        } else {
            ui::design::pairs(
                frame,
                list,
                ui::design::PairTable {
                    headers: ["TOOL", "OUTCOME"],
                    rows: &rows,
                    selected: self.selected,
                },
                hits,
                Target::ToolAttemptRow,
            );
            if let Some(pane) = details {
                self.draw_attempt(frame, pane, hits);
            }
            action("Install record", "Enter", KeyCode::Enter);
            if selected.is_some_and(|a| a.unresolved) {
                action("Close record", "x", KeyCode::Char('x'));
            }
        }
        action("Refresh history", "R", KeyCode::Char('R'));
        action("Back", "Esc", KeyCode::Esc);
    }
}
