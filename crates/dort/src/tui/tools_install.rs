//! Dedicated one-activation Tools approvals, never file-operation receipts.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::{installation, reconciliation::Snapshot};
use dort_core::tools::install;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::Line,
    widgets::Paragraph,
};
use std::path::PathBuf;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Attempts,
    LoadingInstall,
    PrefixInputs,
    Review,
    Result,
    CloseInputs,
    LoadingClose,
    CloseReview,
}
#[derive(Default)]
pub struct State {
    pub mode: Mode,
    pub preview: Option<install::Preview>,
    pub close_preview: Option<install::ClosePreview>,
    pub request: Option<Box<installation::Request>>,
    pub close_request: Option<install::CloseRequest>,
    pub attempts: Vec<install::Attempt>,
    pub error: Option<String>,
    pub selected: usize,
    pub fields: [String; 4],
    pub close_npm: bool,
    pub close_brew: bool,
    close_brew_draft: [String; 4],
    brew_prefix: String,
    brew_drafts: std::collections::BTreeMap<PathBuf, String>,
    close_mise_draft: [String; 2],
    close_npm_draft: [String; 4],
    pub focus: usize,
    pub details: bool,
    pub scroll: u16,
    pub scope: Option<install::Scope>,
    summary: String,
    metadata: String,
}
pub enum Outcome {
    Back,
    Task(Box<Task>),
}
impl Outcome {
    fn task(task: Task) -> Self {
        Self::Task(Box::new(task))
    }
}
impl State {
    pub fn clear_approvals(&mut self) {
        self.preview = None;
        self.close_preview = None;
        if matches!(
            self.mode,
            Mode::LoadingInstall | Mode::Review | Mode::LoadingClose | Mode::CloseReview
        ) {
            self.mode = Mode::Result;
            self.summary = "Review dismissed. Request a fresh review before execution.".into();
        }
    }
    pub fn text_entry(&self) -> bool {
        matches!(
            self.mode,
            Mode::CloseInputs
                | Mode::PrefixInputs
                | Mode::Review
                | Mode::CloseReview
                | Mode::LoadingInstall
                | Mode::LoadingClose
        )
    }
    pub fn prepare(&mut self, mut request: installation::Request) -> Task {
        if let Some(executable) = request.brew_executable().map(std::path::Path::to_owned) {
            if let Some(draft) = self.brew_drafts.get(&executable) {
                request.set_brew_prefix(PathBuf::from(draft));
            }
            self.brew_prefix = request
                .brew_prefix()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
        }
        self.clear_approvals();
        self.mode = Mode::LoadingInstall;
        self.details = false;
        self.error = None;
        self.summary = "Preparing process-free installation review…".into();
        self.request = Some(Box::new(request.clone()));
        Task::ToolInstallPrepare(Box::new(request))
    }
    pub fn preparation_failed(&mut self, error: &str) {
        self.summary = if self
            .request
            .as_ref()
            .is_some_and(|r| r.brew_executable().is_some())
        {
            "Review unavailable. a edits Advanced prefix; r requests a fresh preview; Esc returns."
        } else {
            "Review unavailable. r requests a fresh preview; Esc returns."
        }
        .into();
        self.error = Some(error.into());
    }
    pub fn retry_preview(&mut self) -> Option<Task> {
        self.error = None;
        match self.mode {
            Mode::LoadingInstall => self
                .request
                .as_ref()
                .map(|r| Task::ToolInstallPrepare(r.clone())),
            Mode::LoadingClose => self.close_request.clone().map(Task::ToolClosePrepare),
            _ => None,
        }
    }
    pub fn reviewed(&mut self, preview: install::Preview) {
        self.scope = Some(preview.review().scope.clone());
        let s = preview.review().scope;
        self.summary = if let Some(brew) = &s.brew {
            format!(
                "Homebrew · provider-current · 10m deadline\nBuild/dependency code CAN EXECUTE.\nDependencies may upgrade; native links may\nexpose commands on PATH. No rollback.\n{} · Prefix: {}\nFresh help/layout/formula/catalog probes.\na: Advanced prefix · d: full scope · q waits",
                s.package,
                ui::inline(&brew.prefix.display().to_string())
            )
        } else if let Some(npm) = &s.npm {
            format!(
                "npm install · scripts off · 10m deadline\nDependencies + bin/man links may expose\ncommands on PATH. No rollback. q waits.\nFresh target/runtime/prerequisite queries.\n{}@{}\nPrefix: {}\nd: full scope and query details",
                s.package,
                s.version,
                ui::inline(&npm.binding.prefix.display().to_string())
            )
        } else {
            format!(
                "Install {}@{} with mise\nFresh inspection, install only if still missing, then inspect again.\n10-minute install deadline; q/Ctrl-C waits. No activation.\nProvider/package code and downloads may change data; no rollback.",
                s.package, s.version
            )
        };
        self.metadata =
            serde_json::to_string_pretty(&preview.review()).expect("metadata serializes");
        self.preview = Some(preview);
        self.mode = Mode::Review;
        self.scroll = 0;
    }
    pub fn installed(&mut self, report: Result<install::Report, String>) {
        self.clear_approvals();
        self.mode = Mode::Result;
        self.details = false;
        self.error = None;
        match report {
            Ok(report) => {
                self.summary = if report.no_op {
                    "Matching package already reported; no installer ran."
                } else if installation::succeeded(&report) {
                    if report.scope.brew.is_some() {
                        "Homebrew provider-current formula reported present, not exact-pinned. Native links may expose commands on PATH."
                    } else if report.scope.npm.is_some() {
                        "Exact npm package reported installed; native links may expose commands on PATH."
                    } else { "Exact version reported installed. Inactive mise versions are normal." }
                } else {
                    "Installation incomplete or unverified; partial effects may remain."
                }
                .into();
                self.summary.push_str("\nBinary readiness and ownership are not verified.\nEsc, then c: collect fresh inventory before another install.\ni: inspect installation attempts · d: full outcome");
                self.metadata = serde_json::to_string_pretty(&report).expect("metadata serializes");
            }
            Err(error) => {
                self.summary = "Installation stopped; partial effects may remain.\ni: inspect unresolved attempts; nothing is replayed or rolled back.\nEsc, then c: collect fresh inventory before another install.".into();
                self.error = Some(error);
                self.metadata.clear();
            }
        }
    }
    pub fn open_attempts(&mut self, snapshots: &[Snapshot]) -> Task {
        self.clear_approvals();
        self.mode = Mode::Attempts;
        self.details = false;
        self.error = None;
        if self.fields[0].is_empty()
            && let Some(s) = snapshots.iter().rev().find(|s| {
                s.report.provider.description.query
                    == dort_core::tools::inspection::Query::MiseInstalled
            })
        {
            let d = &s.report.provider.description;
            self.fields[0] = d.executable.display().to_string();
            self.fields[1] = d
                .trusted_search_path
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(":");
        }
        if self.close_npm_draft[0].is_empty()
            && let Some(d) = snapshots
                .iter()
                .rev()
                .map(|s| &s.report.provider.description)
                .find(|d| d.npm_binding.is_some())
            && let Some(b) = &d.npm_binding
        {
            self.close_npm_draft = [
                b.node.display().to_string(),
                d.trusted_search_path
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
                b.npm_cli.display().to_string(),
                b.prefix.display().to_string(),
            ];
            if self.fields[0].is_empty() {
                self.fields = self.close_npm_draft.clone();
                self.close_npm = true;
            }
        }
        if self.close_brew_draft[0].is_empty()
            && let Some(d) = snapshots
                .iter()
                .rev()
                .map(|s| &s.report.provider.description)
                .find(|d| d.query == dort_core::tools::inspection::Query::BrewFormulae)
        {
            self.close_brew_draft = [
                d.executable.display().to_string(),
                d.trusted_search_path
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
                install::brew::prefix_hint(&d.executable)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                String::new(),
            ];
            if self.fields[0].is_empty() {
                self.fields = self.close_brew_draft.clone();
                self.close_brew = true;
                self.close_npm = false;
            }
        }
        Task::ToolAttempts
    }
    pub fn attempts_ready(&mut self, result: Result<Vec<install::Attempt>, String>) {
        self.attempts.clear();
        match result {
            Ok(attempts) => {
                self.attempts = attempts;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.mode = Mode::Attempts;
        self.selected = self.selected.min(self.attempts.len().saturating_sub(1));
        self.scroll = 0;
    }
    fn prepare_close(&mut self) -> Option<Task> {
        let attempt = self.attempts.get(self.selected)?;
        if !attempt.unresolved {
            return None;
        }
        let request = install::CloseRequest {
            id: attempt.id.clone(),
            executable: None,
            search_dirs: vec![],
            npm_binding: None,
            brew_prefix: None,
        };
        self.close_request = Some(request.clone());
        self.details = false;
        self.error = None;
        if attempt.scope.is_some() {
            self.mode = Mode::LoadingClose;
            Some(Task::ToolClosePrepare(request))
        } else {
            self.mode = Mode::CloseInputs;
            self.focus = 0;
            None
        }
    }
    fn close_field_count(&self) -> usize {
        if self.close_npm {
            4
        } else if self.close_brew {
            3
        } else {
            2
        }
    }
    fn toggle_close_provider(&mut self) {
        if self.close_brew {
            self.close_brew_draft = self.fields.clone();
            self.fields = [
                self.close_mise_draft[0].clone(),
                self.close_mise_draft[1].clone(),
                String::new(),
                String::new(),
            ];
            self.close_brew = false;
        } else if self.close_npm {
            self.close_npm_draft = self.fields.clone();
            self.fields = self.close_brew_draft.clone();
            self.close_npm = false;
            self.close_brew = true;
        } else {
            self.close_mise_draft = [self.fields[0].clone(), self.fields[1].clone()];
            self.fields = self.close_npm_draft.clone();
            self.close_npm = true;
        }
        self.focus = self.close_field_count() + 1;
    }
    pub fn close_reviewed(&mut self, preview: install::ClosePreview) {
        self.metadata =
            serde_json::to_string_pretty(&preview.review()).expect("metadata serializes");
        self.summary = format!(
            "Close unresolved attempt {}\nFreshly inspect its provider scope, then close unchanged metadata.\nNo package replay, repair, readiness verification or rollback.\nConfig/plugins may execute code and caches may change.",
            preview.review().attempt_id
        );
        self.close_preview = Some(preview);
        self.mode = Mode::CloseReview;
        self.details = false;
        self.scroll = 0;
    }
    pub fn closed(&mut self, result: Result<install::Attempt, String>) {
        self.close_preview = None;
        self.mode = Mode::Attempts;
        self.details = false;
        match result {
            Ok(attempt) => {
                self.summary = "Attempt metadata explicitly closed after fresh inspection; no rollback or install success is implied.".into();
                self.metadata =
                    serde_json::to_string_pretty(&attempt).expect("metadata serializes");
                if let Some(old) = self.attempts.iter_mut().find(|a| a.id == attempt.id) {
                    *old = attempt;
                }
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
                self.mode = Mode::Result;
                self.summary = "Closure not completed. i reopens attempts for a fresh review; no approval is retained.".into();
            }
        }
    }
    fn retain_prefix_draft(&mut self) {
        if let Some(request) = self.request.as_mut() {
            if let Some(exe) = request.brew_executable().map(std::path::Path::to_owned) {
                self.brew_drafts.insert(exe, self.brew_prefix.clone());
            }
            request.set_brew_prefix(PathBuf::from(&self.brew_prefix));
        }
    }
    pub fn key(&mut self, key: KeyEvent, dry_run: bool, snapshots: &[Snapshot]) -> Option<Outcome> {
        if self.mode == Mode::PrefixInputs {
            return match key.code {
                KeyCode::Esc => {
                    self.retain_prefix_draft();
                    self.mode = Mode::LoadingInstall;
                    self.preparation_failed("Prefix edit retained; request a fresh review.");
                    None
                }
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                    self.focus = 1 - self.focus.min(1);
                    None
                }
                KeyCode::Enter if self.focus == 0 => {
                    self.focus = 1;
                    None
                }
                KeyCode::Enter => {
                    self.retain_prefix_draft();
                    let request = self.request.as_mut()?;
                    self.mode = Mode::LoadingInstall;
                    self.error = None;
                    Some(Outcome::task(Task::ToolInstallPrepare(request.clone())))
                }
                _ if self.focus == 0 => {
                    edit_text(&mut self.brew_prefix, key);
                    None
                }
                _ => None,
            };
        }
        if self.mode == Mode::CloseInputs {
            let count = self.close_field_count();
            return match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Attempts;
                    None
                }
                KeyCode::Tab | KeyCode::Down => {
                    self.focus = (self.focus + 1) % (count + 2);
                    None
                }
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = (self.focus + count + 1) % (count + 2);
                    None
                }
                KeyCode::Enter if self.focus == count + 1 => {
                    self.toggle_close_provider();
                    None
                }
                KeyCode::Enter if self.focus == count => {
                    let request = self.close_request.as_mut()?;
                    request.executable = Some(PathBuf::from(&self.fields[0]));
                    request.search_dirs = self.fields[1].split(':').map(PathBuf::from).collect();
                    request.brew_prefix = self.close_brew.then(|| PathBuf::from(&self.fields[2]));
                    request.npm_binding =
                        self.close_npm
                            .then(|| dort_core::tools::inspection::NpmBinding {
                                runtime_tool: "historical-runtime".into(),
                                node: PathBuf::from(&self.fields[0]),
                                npm_cli: PathBuf::from(&self.fields[2]),
                                prefix: PathBuf::from(&self.fields[3]),
                            });
                    self.mode = Mode::LoadingClose;
                    Some(Outcome::task(Task::ToolClosePrepare(request.clone())))
                }
                KeyCode::Enter => {
                    self.focus = (self.focus + 1) % (count + 2);
                    None
                }
                _ if self.focus < count => {
                    edit_text(&mut self.fields[self.focus], key);
                    None
                }
                _ => None,
            };
        }
        if self.details {
            match key.code {
                KeyCode::Esc | KeyCode::Char('d') => self.details = false,
                KeyCode::Home => self.scroll = 0,
                KeyCode::PageDown | KeyCode::Down => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp | KeyCode::Up => self.scroll = self.scroll.saturating_sub(3),
                _ => (),
            }
            return None;
        }
        match key.code {
            KeyCode::Char('a')
                if matches!(self.mode, Mode::Review | Mode::LoadingInstall)
                    && self
                        .request
                        .as_ref()
                        .is_some_and(|r| r.brew_executable().is_some()) =>
            {
                self.preview = None;
                self.mode = Mode::PrefixInputs;
                self.focus = usize::from(!self.brew_prefix.is_empty());
                self.error = None;
            }
            KeyCode::Esc if self.mode == Mode::CloseReview => {
                self.close_preview = None;
                self.mode = Mode::Attempts;
            }
            KeyCode::Esc => {
                self.clear_approvals();
                return Some(Outcome::Back);
            }
            KeyCode::Char('r')
                if matches!(self.mode, Mode::LoadingInstall | Mode::LoadingClose) =>
            {
                return self.retry_preview().map(Outcome::task);
            }
            KeyCode::Char('i') => return Some(Outcome::task(self.open_attempts(snapshots))),
            KeyCode::Char('d') | KeyCode::Enter if self.mode == Mode::Attempts => {
                if let Some(a) = self.attempts.get(self.selected) {
                    self.metadata = serde_json::to_string_pretty(a).expect("metadata serializes");
                    self.details = true;
                    self.scroll = 0;
                }
            }
            KeyCode::Char('d') => {
                self.details = true;
                self.scroll = 0;
            }
            KeyCode::Enter if !dry_run && self.mode == Mode::Review => {
                let preview = self.preview.take()?;
                return Some(Outcome::task(Task::ToolInstall(Box::new(
                    preview.approve(),
                ))));
            }
            KeyCode::Enter if !dry_run && self.mode == Mode::CloseReview => {
                let preview = self.close_preview.take()?;
                return Some(Outcome::task(Task::ToolClose(Box::new(preview.approve()))));
            }
            KeyCode::Char('c') if self.mode == Mode::Attempts => {
                return self.prepare_close().map(Outcome::task);
            }
            KeyCode::Char('r') if self.mode == Mode::Attempts => {
                return Some(Outcome::task(Task::ToolAttempts));
            }
            KeyCode::Down if self.mode == Mode::Attempts => {
                self.selected = (self.selected + 1).min(self.attempts.len().saturating_sub(1))
            }
            KeyCode::Up if self.mode == Mode::Attempts => {
                self.selected = self.selected.saturating_sub(1)
            }
            _ => (),
        }
        None
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        if self.details {
            ui::report(
                frame,
                area,
                "Tools installation details · d/Esc back",
                ui::review_text(&self.metadata),
                self.scroll,
                false,
            );
            return;
        }
        if self.mode == Mode::PrefixInputs {
            let rect = Rect::new(area.x, area.y, area.width, 3.min(area.height));
            hits.click(rect, Target::ToolInstallControl(0, false));
            ui::input(
                frame,
                rect,
                "Expected physical Homebrew prefix · absolute",
                &self.brew_prefix,
                self.focus == 0,
            );
            if area.height > 3 {
                let rect = Rect::new(area.x, area.y + 3, area.width, 1);
                hits.click(rect, Target::ToolInstallControl(1, true));
                frame.render_widget(
                    Paragraph::new(Line::from(ui::button(
                        "Review installation",
                        "Enter",
                        self.focus == 1,
                    ))),
                    rect,
                );
            }
            return;
        }
        if self.mode == Mode::CloseInputs {
            let count = self.close_field_count();
            let budget = area.height.saturating_sub(1);
            let first = if usize::from(budget) < count * 3 + 2 {
                self.focus.min(count)
            } else {
                0
            };
            let mut y = area.y;
            for i in first..count + 2 {
                let h = if i < count { 3 } else { 1 };
                if y + h > area.bottom().saturating_sub(1) {
                    break;
                }
                let rect = Rect::new(area.x, y, area.width, h);
                hits.click(rect, Target::ToolInstallControl(i, i >= count));
                if i < count {
                    let title = match i {
                        0 if self.close_brew => "Trusted Homebrew executable · absolute",
                        0 if self.close_npm => "Trusted Node executable · absolute",
                        0 => "Trusted mise executable · absolute",
                        1 => "Trusted child PATH · colon-separated",
                        2 if self.close_brew => "Original Homebrew prefix · absolute",
                        2 => "Trusted npm CLI script · absolute",
                        _ => "Original npm global prefix · absolute",
                    };
                    ui::input(frame, rect, title, &self.fields[i], self.focus == i);
                } else {
                    let label = if i == count {
                        "Review closure"
                    } else if self.close_brew {
                        "Provider: brew · change to mise"
                    } else if self.close_npm {
                        "Provider: npm · change to brew"
                    } else {
                        "Provider: mise · change to npm"
                    };
                    frame.render_widget(
                        Paragraph::new(Line::from(ui::button(label, "Enter", self.focus == i))),
                        rect,
                    );
                }
                y += h;
            }
            ui::prose(
                frame,
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
                "Missing provider scope · Tab moves · Esc back",
                ui::muted(),
            );
            return;
        }
        if self.mode == Mode::Attempts {
            if let Some(error) = &self.error {
                ui::prose(
                    frame,
                    area,
                    &format!("Attempts unavailable: {error}\nr refreshes · Esc back"),
                    ui::warning(),
                );
                return;
            }
            let capacity = usize::from(area.height.saturating_sub(3)).max(1);
            let offset = self.selected.saturating_sub(capacity - 1);
            let mut lines = vec![Line::styled(
                "Installation attempts · Enter details · c close · r refresh",
                ui::heading(),
            )];
            for (i, attempt) in self.attempts.iter().enumerate().skip(offset).take(capacity) {
                lines.push(Line::styled(
                    ui::clip(
                        &format!(
                            "{} | {} | {}",
                            if attempt.unresolved {
                                "UNRESOLVED"
                            } else if attempt.explicitly_closed {
                                "Explicitly closed"
                            } else {
                                "Observed outcome"
                            },
                            attempt.id,
                            attempt
                                .scope
                                .as_ref()
                                .map(|s| if s.brew.is_some() {
                                    format!("{} (provider-current)", s.package)
                                } else {
                                    format!("{}@{}", s.package, s.version)
                                })
                                .unwrap_or("Unknown scope".into())
                        ),
                        area.width,
                    ),
                    if i == self.selected {
                        ui::selected()
                    } else {
                        ui::muted()
                    },
                ));
                hits.click(
                    Rect::new(area.x, area.y + (i - offset) as u16 + 1, area.width, 1),
                    Target::ToolAttemptRow(i),
                );
            }
            if self.attempts.is_empty() {
                lines.push(Line::raw("No installation attempts recorded."));
            }
            frame.render_widget(Paragraph::new(lines), area);
            if self
                .attempts
                .get(self.selected)
                .is_some_and(|a| a.unresolved)
            {
                let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(ui::button("Review closure", "c", true))),
                    button,
                );
                hits.click(button, Target::ToolAttemptClose);
            }
            return;
        }
        let message = if let Some(error) = &self.error {
            format!("{}\n{}", self.summary, error)
        } else {
            self.summary.clone()
        };
        ui::prose(
            frame,
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
            &message,
            ui::muted(),
        );
        if matches!(self.mode, Mode::Review | Mode::CloseReview) {
            let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(ui::button(
                    if dry_run {
                        "Preview only · execution disabled"
                    } else if self.mode == Mode::Review
                        && self.scope.as_ref().is_some_and(|s| s.brew.is_some())
                    {
                        "Install core formula · 10m"
                    } else if self.mode == Mode::Review {
                        "Install exact version · 10m"
                    } else {
                        "Inspect and close attempt"
                    },
                    "Enter",
                    !dry_run,
                ))),
                button,
            );
            if !dry_run {
                hits.click(button, Target::ToolInstallControl(2, true));
            }
        }
    }
}
