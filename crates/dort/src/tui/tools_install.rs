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
    AptInputs,
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
    pub close_apt: bool,
    close_apt_draft: [String; 4],
    close_seeded: bool,
    close_initialized: [bool; 4],
    apt_fields: [String; 3],
    apt_drafts: std::collections::BTreeMap<PathBuf, [String; 3]>,
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
    FreshReview,
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
                | Mode::AptInputs
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
        if let Some(executable) = request.apt_executable().map(std::path::Path::to_owned) {
            if let Some(draft) = self.apt_drafts.get(&executable) {
                request.set_apt_programs(
                    draft
                        .clone()
                        .map(|s| (!s.is_empty()).then(|| PathBuf::from(s))),
                );
            }
            self.apt_fields = request
                .apt_programs()
                .expect("APT inputs")
                .clone()
                .map(|p| p.map(|p| p.display().to_string()).unwrap_or_default());
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
            .is_some_and(|r| r.brew_executable().is_some() || r.apt_executable().is_some())
        {
            "Review unavailable. a edits Advanced inputs; r requests a fresh preview; Esc returns."
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
        self.summary = if s.apt.is_some() {
            format!(
                "APT · ROOT RISK · {} · current, not pinned\nRoot scripts/hooks/dependency code CAN EXECUTE.\nDependencies upgrade; services start/restart.\nSystem/config/data/live links may change.\nNo sandbox or rollback; native root policy.\nsudo -n -T 600: denied policy fails, no prompt.\nOverdue supervisor: keep waiting joined.\na: Advanced · d: full scope · q waits",
                ui::inline(&s.package)
            )
        } else if let Some(brew) = &s.brew {
            format!(
                "Homebrew · provider-current · 10m deadline\nBuild/dependency code CAN EXECUTE.\nDependencies may upgrade; native links may\nexpose commands on PATH. No rollback.\n{} · Prefix: {}\nFresh help/layout/formula/catalog probes.\na: Advanced prefix · d: full scope · q waits",
                ui::inline(&s.package),
                ui::inline(&brew.prefix.display().to_string())
            )
        } else if let Some(npm) = &s.npm {
            format!(
                "npm install · scripts off · 10m deadline\nDependencies + bin/man links may expose\ncommands on PATH. No rollback. q waits.\nFresh target/runtime/prerequisite queries.\n{}@{}\nPrefix: {}\nd: full scope and query details",
                ui::inline(&s.package),
                ui::inline(&s.version),
                ui::inline(&npm.binding.prefix.display().to_string())
            )
        } else {
            format!(
                "mise · exact install · 10m deadline\nProvider/backend/package code CAN EXECUTE.\nDownloads can change data/caches; no rollback.\nNo activation; readiness is not verified.\nq/Ctrl-C waits for completion.\nFresh inspection, conditional install, re-inspect.\n{}@{}\nd: full scope and arguments",
                ui::inline(&s.package),
                ui::inline(&s.version)
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
                let summary = report.summary();
                let caption = if report.no_op {
                    "No installer ran; matching package reported."
                } else if installation::succeeded(&report) {
                    if report.scope.apt.is_some() {
                        "APT current package, not exact-pinned (root risk)."
                    } else if report.scope.brew.is_some() {
                        "Homebrew current package, not exact-pinned."
                    } else if report.scope.npm.is_some() {
                        "Exact npm package reported installed."
                    } else {
                        "Exact version reported installed (inactive)."
                    }
                } else {
                    summary.status.label()
                };
                self.summary = format!(
                    "{caption}\n{} · {}\n{}\n{}\nNo readiness/ownership/termination proof.\nNo rollback; effects may be partial/ongoing.\nd: full outcome · i: attempts",
                    summary.status.label(),
                    summary.process_label(),
                    summary.package_note(),
                    summary.binary_label()
                );
                self.metadata = serde_json::to_string_pretty(&report).expect("metadata serializes");
            }
            Err(error) => {
                self.summary = "Outcome unavailable; effects may be partial/ongoing.\ni: inspect unresolved attempts before another install.\nNo approval is retained or replayed; no rollback.\nf: start fresh review (navigation only).".into();
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
        if !self.close_seeded
            && !self.close_initialized[0]
            && self.fields[0].is_empty()
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
            self.close_npm = false;
            self.close_brew = false;
            self.close_apt = false;
            self.retain_close_draft();
        }
        if !self.close_initialized[1]
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
            self.close_initialized[1] = true;
            if !self.close_seeded && self.fields[0].is_empty() {
                self.fields = self.close_npm_draft.clone();
                self.close_npm = true;
                self.close_brew = false;
                self.close_apt = false;
                self.close_seeded = true;
            }
        }
        if !self.close_initialized[2]
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
            self.close_initialized[2] = true;
            if !self.close_seeded && self.fields[0].is_empty() {
                self.fields = self.close_brew_draft.clone();
                self.close_apt = false;
                self.close_seeded = true;
                self.close_brew = true;
                self.close_npm = false;
            }
        }
        if !self.close_initialized[3]
            && let Some(d) = snapshots
                .iter()
                .rev()
                .map(|s| &s.report.provider.description)
                .find(|d| d.query == dort_core::tools::inspection::Query::DpkgPackages)
        {
            self.close_apt_draft = [
                d.executable.display().to_string(),
                d.trusted_search_path
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
                String::new(),
                String::new(),
            ];
            self.close_initialized[3] = true;
            if !self.close_seeded && self.fields[0].is_empty() {
                self.fields = self.close_apt_draft.clone();
                self.close_seeded = true;
                self.close_apt = true;
                self.close_brew = false;
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
            apt: false,
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
    pub(super) fn close_field_count(&self) -> usize {
        if self.close_npm {
            4
        } else if self.close_brew {
            3
        } else {
            2
        }
    }
    fn retain_close_draft(&mut self) {
        self.close_seeded = true;
        let index = if self.close_apt {
            3
        } else if self.close_brew {
            2
        } else if self.close_npm {
            1
        } else {
            0
        };
        self.close_initialized[index] = true;
        match index {
            3 => self.close_apt_draft = self.fields.clone(),
            2 => self.close_brew_draft = self.fields.clone(),
            1 => self.close_npm_draft = self.fields.clone(),
            _ => self.close_mise_draft = [self.fields[0].clone(), self.fields[1].clone()],
        }
    }
    fn toggle_close_provider(&mut self) {
        self.retain_close_draft();
        if self.close_brew {
            self.close_brew_draft = self.fields.clone();
            self.fields = self.close_apt_draft.clone();
            self.close_brew = false;
            self.close_apt = true;
        } else if self.close_apt {
            self.close_apt_draft = self.fields.clone();
            self.fields = [
                self.close_mise_draft[0].clone(),
                self.close_mise_draft[1].clone(),
                String::new(),
                String::new(),
            ];
            self.close_apt = false;
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
        self.focus = 0;
    }
    pub fn close_reviewed(&mut self, preview: install::ClosePreview) {
        self.metadata =
            serde_json::to_string_pretty(&preview.review()).expect("metadata serializes");
        self.summary = format!(
            "Close unresolved attempt {}\nFresh provider query; close unchanged metadata.\nNo replay, repair, readiness or rollback.\nNo root-termination or install-success proof.\nConfig/plugins may run code; caches may change.\nd: full scope and query details",
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
    fn retain_apt_draft(&mut self) {
        if let Some(request) = self.request.as_mut() {
            if let Some(exe) = request.apt_executable().map(std::path::Path::to_owned) {
                self.apt_drafts.insert(exe, self.apt_fields.clone());
            }
            request.set_apt_programs(
                self.apt_fields
                    .clone()
                    .map(|s| (!s.is_empty()).then(|| PathBuf::from(s))),
            );
        }
    }
    pub fn key(&mut self, key: KeyEvent, dry_run: bool, snapshots: &[Snapshot]) -> Option<Outcome> {
        if self.mode == Mode::AptInputs {
            return match key.code {
                KeyCode::Esc => {
                    self.retain_apt_draft();
                    self.mode = Mode::LoadingInstall;
                    self.preparation_failed("APT inputs retained; request a fresh review.");
                    None
                }
                KeyCode::Tab | KeyCode::Down => {
                    self.focus = (self.focus + 1) % 3;
                    None
                }
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = (self.focus + 2) % 3;
                    None
                }
                KeyCode::Enter => {
                    if let Some(i) = self.apt_fields.iter().position(String::is_empty) {
                        self.focus = i;
                        self.error =
                            Some("Select all three trusted program paths before review.".into());
                        return None;
                    }
                    self.retain_apt_draft();
                    self.mode = Mode::LoadingInstall;
                    self.error = None;
                    self.request
                        .as_ref()
                        .map(|r| Outcome::task(Task::ToolInstallPrepare(r.clone())))
                }
                _ if self.focus < 3 => {
                    edit_text(&mut self.apt_fields[self.focus], key);
                    None
                }
                _ => None,
            };
        }
        if self.mode == Mode::PrefixInputs {
            return match key.code {
                KeyCode::Esc => {
                    self.retain_prefix_draft();
                    self.mode = Mode::LoadingInstall;
                    self.preparation_failed("Prefix edit retained; request a fresh review.");
                    None
                }
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                    self.focus = 0;
                    None
                }
                KeyCode::Enter => {
                    if self.brew_prefix.is_empty() {
                        self.focus = 0;
                        self.error =
                            Some("Select the expected physical prefix before review.".into());
                        return None;
                    }
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
                    self.retain_close_draft();
                    self.mode = Mode::Attempts;
                    None
                }
                KeyCode::Tab | KeyCode::Down => {
                    self.focus = (self.focus + 1) % count;
                    None
                }
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = (self.focus + count - 1) % count;
                    None
                }
                KeyCode::F(2) => {
                    self.toggle_close_provider();
                    None
                }
                KeyCode::Enter => {
                    if let Some(i) = self.fields[..count].iter().position(String::is_empty) {
                        self.focus = i;
                        self.error =
                            Some("Complete the displayed provider fields before review.".into());
                        return None;
                    }
                    self.retain_close_draft();
                    let request = self.close_request.as_mut()?;
                    request.executable = Some(PathBuf::from(&self.fields[0]));
                    request.search_dirs = self.fields[1].split(':').map(PathBuf::from).collect();
                    request.apt = self.close_apt;
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
                KeyCode::Char('f') if matches!(self.mode, Mode::Result | Mode::Attempts) => {
                    self.clear_approvals();
                    return Some(Outcome::FreshReview);
                }
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
                    && self.request.as_ref().is_some_and(|r| {
                        r.brew_executable().is_some() || r.apt_executable().is_some()
                    }) =>
            {
                self.preview = None;
                let apt = self
                    .request
                    .as_ref()
                    .is_some_and(|r| r.apt_executable().is_some());
                self.mode = if apt {
                    Mode::AptInputs
                } else {
                    Mode::PrefixInputs
                };
                self.focus = if apt {
                    self.apt_fields
                        .iter()
                        .position(String::is_empty)
                        .unwrap_or(0)
                } else {
                    0
                };
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
            KeyCode::Char('f') if matches!(self.mode, Mode::Result | Mode::Attempts) => {
                self.clear_approvals();
                return Some(Outcome::FreshReview);
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
    fn fresh_button(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(ui::button("Start fresh review", "f", true))),
            button,
        );
        hits.click(button, Target::ToolInstallFreshReview);
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        if self.details {
            ui::report(
                frame,
                area,
                if matches!(self.mode, Mode::Result | Mode::Attempts) {
                    "Tools installation details · d/Esc back · f fresh review"
                } else {
                    "Tools installation details · d/Esc back"
                },
                ui::review_text(&self.metadata),
                self.scroll,
                false,
            );
            return;
        }
        if matches!(
            self.mode,
            Mode::PrefixInputs | Mode::AptInputs | Mode::CloseInputs
        ) {
            let fields: Vec<(&str, &str)> = match self.mode {
                Mode::PrefixInputs => vec![(
                    "Expected physical Homebrew prefix · absolute",
                    &self.brew_prefix,
                )],
                Mode::AptInputs => [
                    "Trusted apt-cache · absolute",
                    "Trusted apt-get · absolute",
                    "Trusted sudo · absolute",
                ]
                .into_iter()
                .zip(self.apt_fields.iter().map(String::as_str))
                .collect(),
                _ => (0..self.close_field_count())
                    .map(|i| {
                        let label = match i {
                            0 if self.close_apt => "Trusted dpkg-query · absolute",
                            0 if self.close_brew => "Trusted Homebrew executable · absolute",
                            0 if self.close_npm => "Trusted Node executable · absolute",
                            0 => "Trusted mise executable · absolute",
                            1 => "Trusted child PATH · colon-separated",
                            2 if self.close_brew => "Original Homebrew prefix · absolute",
                            2 => "Trusted npm CLI script · absolute",
                            _ => "Original npm global prefix · absolute",
                        };
                        (label, self.fields[i].as_str())
                    })
                    .collect(),
            };
            let mut actions = vec![ui::ActionButton {
                label: if self.mode == Mode::CloseInputs {
                    "Review closure"
                } else {
                    "Review installation"
                },
                key: "Enter",
                target: Target::ToolInstallControl(fields.len(), true),
            }];
            if self.mode == Mode::CloseInputs {
                actions.push(ui::ActionButton {
                    label: if self.close_apt {
                        "Provider: apt"
                    } else if self.close_brew {
                        "Provider: brew"
                    } else if self.close_npm {
                        "Provider: npm"
                    } else {
                        "Provider: mise"
                    },
                    key: "F2",
                    target: Target::ToolInstallProvider,
                });
            }
            let hint = self
                .error
                .as_ref()
                .map(|e| ui::inline(e))
                .unwrap_or_else(|| {
                    "Tab: fields · Enter: fresh review · Esc: Back (draft retained)".into()
                });
            ui::field_page(
                frame,
                area,
                &fields,
                self.focus,
                hits,
                |i| Target::ToolInstallControl(i, false),
                ui::FormActions {
                    buttons: &actions,
                    hint: &hint,
                },
            );
            return;
        }
        if self.mode == Mode::Attempts {
            if let Some(error) = &self.error {
                ui::chrome_prose(
                    frame,
                    area,
                    &format!(
                        "Attempts unavailable: {}\nr refreshes · f fresh review · Esc back",
                        ui::inline(error)
                    ),
                    ui::warning(),
                );
                self.fresh_button(frame, area, hits);
                return;
            }
            let capacity = usize::from(area.height.saturating_sub(5)).max(1);
            let offset = self.selected.saturating_sub(capacity - 1);
            let mut lines = vec![Line::styled(
                "Installation attempts · Enter details · c close · f fresh review",
                ui::heading(),
            )];
            for (i, attempt) in self.attempts.iter().enumerate().skip(offset).take(capacity) {
                lines.push(Line::styled(
                    ui::clip(
                        &format!(
                            "{} | {} | {}",
                            attempt.summary().status.label(),
                            attempt.id,
                            attempt
                                .scope
                                .as_ref()
                                .map(|s| if s.brew.is_some() || s.apt.is_some() {
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
            if let Some(attempt) = self.attempts.get(self.selected) {
                let summary = attempt.summary();
                lines.push(Line::styled(
                    ui::clip(summary.package_note(), area.width),
                    ui::muted(),
                ));
                lines.push(Line::styled(
                    ui::clip(
                        if attempt.unresolved {
                            "Unresolved: review closure before another install."
                        } else if attempt.explicitly_closed {
                            "Closed metadata, not install success or repair."
                        } else if !attempt.outcome.is_some_and(install::Outcome::succeeded) {
                            "Effects may be partial/ongoing; f: fresh review."
                        } else {
                            "Historical only; fresh inventories + explicit binding."
                        },
                        area.width,
                    ),
                    ui::muted(),
                ));
            }
            frame.render_widget(Paragraph::new(lines), area);
            self.fresh_button(frame, area, hits);
            if self
                .attempts
                .get(self.selected)
                .is_some_and(|a| a.unresolved)
            {
                let button = Rect::new(area.x, area.bottom().saturating_sub(2), area.width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(ui::button("Review closure", "c", true))),
                    button,
                );
                hits.click(button, Target::ToolAttemptClose);
            }
            return;
        }
        let message = if let Some(error) = &self.error {
            format!("{}\n{}", self.summary, ui::inline(error))
        } else {
            self.summary.clone()
        };
        // Metadata must not wrap an unbounded package/prefix ahead of the
        // mandatory compact warning. Full identities remain in d / scope.
        let message = if matches!(self.mode, Mode::Review | Mode::CloseReview) {
            message
                .lines()
                .map(|line| ui::chrome_clip(line, area.width))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            message
        };
        ui::chrome_prose(
            frame,
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
            &message,
            ui::muted(),
        );
        if self.mode == Mode::Result {
            self.fresh_button(frame, area, hits);
        }
        if matches!(self.mode, Mode::Review | Mode::CloseReview) {
            let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(ui::button(
                    if dry_run {
                        "Preview only · execution disabled"
                    } else if self.mode == Mode::Review
                        && self.scope.as_ref().is_some_and(|s| s.apt.is_some())
                    {
                        "Install with root risk · sudo 10m"
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
