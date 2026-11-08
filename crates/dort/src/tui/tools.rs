//! Explicit provider inspection, independent of repository observation and write approvals.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::tools::{self, Request};
use dort_core::tools::inspection::{Observation, Query};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{cell::Cell, path::PathBuf};

const QUERIES: [Query; 4] = [
    Query::BrewFormulae,
    Query::DpkgPackages,
    Query::MiseInstalled,
    Query::NpmGlobals,
];
const LABELS: [&str; 7] = [
    "Provider · Space cycles brew/apt/mise/npm",
    "Trusted executable (Node for npm) · absolute",
    "Trusted search directories · colon-separated",
    "Manifest · absolute (npm)",
    "Declared runtime tool ID (npm)",
    "Trusted npm CLI script · absolute",
    "Global npm prefix · absolute",
];
pub struct State {
    pub query: usize,
    pub fields: [String; 6],
    pub configuring: bool,
    pub focus: usize,
    pub preview: Option<tools::Preview>,
    pub confirmation: String,
    pub report: Option<tools::Report>,
    pub scope: String,
    pub search: String,
    pub searching: bool,
    pub details: bool,
    pub selected: usize,
    pub scroll: u16,
    matching: Vec<usize>,
    capacity: Cell<usize>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            query: 0,
            fields: Default::default(),
            configuring: false,
            focus: 0,
            preview: None,
            confirmation: String::new(),
            report: None,
            scope: String::new(),
            search: String::new(),
            searching: false,
            details: false,
            selected: 0,
            scroll: 0,
            matching: vec![],
            capacity: Cell::new(1),
        }
    }
}
impl State {
    fn controls(&self) -> Vec<usize> {
        if self.query == 3 {
            (0..9).collect()
        } else {
            vec![0, 1, 2, 7, 8]
        }
    }
    pub fn text_entry(&self) -> bool {
        self.configuring || self.preview.is_some() || self.searching
    }
    pub fn configure(&mut self) {
        self.leave();
        self.configuring = true;
        self.focus = 0;
    }
    pub fn leave(&mut self) {
        self.preview = None;
        self.confirmation.clear();
        self.configuring = false;
        self.searching = false;
    }
    pub fn shrink(&mut self) {
        self.confirmation.clear();
    }
    fn request(&self) -> Request {
        let npm = self.query == 3;
        Request {
            query: QUERIES[self.query],
            executable: self.fields[0].clone().into(),
            search_dirs: self.fields[1].split(':').map(PathBuf::from).collect(),
            manifest: npm.then(|| PathBuf::from(&self.fields[2])),
            runtime_tool: npm.then(|| self.fields[3].clone()),
            npm_cli: npm.then(|| PathBuf::from(&self.fields[4])),
            prefix: npm.then(|| PathBuf::from(&self.fields[5])),
        }
    }
    pub fn prepare(&mut self) -> Task {
        self.preview = None;
        self.confirmation.clear();
        self.configuring = false;
        self.searching = false;
        Task::ToolsPrepare(self.request())
    }
    pub fn reviewed(&mut self, preview: tools::Preview) {
        self.scope = serde_json::to_string_pretty(&preview.review())
            .expect("inspection metadata serializes");
        self.preview = Some(preview);
        self.confirmation.clear();
        self.configuring = false;
        self.scroll = 0;
    }
    pub fn completed(&mut self, report: tools::Report) {
        self.scope = serde_json::to_string_pretty(&tools::Review {
            manifest: report.manifest.as_deref(),
            provider: &report.provider.description,
        })
        .expect("inspection metadata serializes");
        self.leave();
        self.report = Some(report);
        self.details = false;
        self.selected = 0;
        self.scroll = 0;
        self.refilter();
    }
    fn refilter(&mut self) {
        let needle = self.search.to_lowercase();
        self.matching = match self.report.as_ref().map(|r| &r.provider.observation) {
            Some(Observation::Complete { installations }) => installations
                .iter()
                .enumerate()
                .filter(|(_, i)| {
                    format!("{} {}", i.package, i.version)
                        .to_lowercase()
                        .contains(&needle)
                })
                .map(|(i, _)| i)
                .collect(),
            _ => vec![],
        };
        self.selected = self.selected.min(self.matching.len().saturating_sub(1));
    }
    /// Some means locally handled, including printable text that must not become shortcuts.
    pub fn key(&mut self, key: KeyEvent, dry_run: bool) -> Option<Option<Task>> {
        if self.preview.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.preview = None;
                    self.confirmation.clear();
                    self.configuring = true;
                }
                KeyCode::Home => self.scroll = 0,
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
                KeyCode::Enter if !dry_run && self.confirmation == "inspect" => {
                    let preview = self.preview.take()?;
                    self.confirmation.clear();
                    self.report = None;
                    return Some(Some(Task::ToolsInspect(preview.approve())));
                }
                _ if !dry_run => edit_text(&mut self.confirmation, key),
                _ => (),
            }
            return Some(None);
        }
        if self.configuring {
            let controls = self.controls();
            let pos = controls.iter().position(|i| *i == self.focus).unwrap_or(0);
            match key.code {
                KeyCode::Esc => self.configuring = false,
                KeyCode::Tab | KeyCode::Down => self.focus = controls[(pos + 1) % controls.len()],
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = controls[(pos + controls.len() - 1) % controls.len()]
                }
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.focus == 0 => {
                    self.query = (self.query + if key.code == KeyCode::Left { 3 } else { 1 }) % 4;
                }
                KeyCode::Enter if self.focus == 8 => self.configuring = false,
                KeyCode::Enter if self.focus == 7 => return Some(Some(self.prepare())),
                KeyCode::Enter => self.focus = controls[(pos + 1) % controls.len()],
                _ if (1..=6).contains(&self.focus) => {
                    edit_text(&mut self.fields[self.focus - 1], key)
                }
                _ => (),
            }
            return Some(None);
        }
        if self.searching {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => self.searching = false,
                _ => {
                    edit_text(&mut self.search, key);
                    self.selected = 0;
                    self.refilter();
                }
            }
            return Some(None);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        match key.code {
            KeyCode::Char('c' | 'e') => self.configure(),
            KeyCode::Char('r') => return Some(Some(self.prepare())),
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('d') | KeyCode::Enter if self.report.is_some() => {
                self.details = !self.details;
                self.scroll = 0;
            }
            KeyCode::Esc if self.details => self.details = false,
            KeyCode::Esc => {
                self.search.clear();
                self.refilter();
            }
            KeyCode::Home => {
                self.selected = 0;
                self.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if !self.details => {
                self.selected = (self.selected + 1).min(self.matching.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') if !self.details => {
                self.selected = self.selected.saturating_sub(1)
            }
            KeyCode::PageDown if !self.details => {
                self.selected = self
                    .selected
                    .saturating_add(self.capacity.get())
                    .min(self.matching.len().saturating_sub(1))
            }
            KeyCode::PageUp if !self.details => {
                self.selected = self.selected.saturating_sub(self.capacity.get())
            }
            KeyCode::PageDown | KeyCode::Down if self.details => {
                self.scroll = self.scroll.saturating_add(3)
            }
            KeyCode::PageUp | KeyCode::Up if self.details => {
                self.scroll = self.scroll.saturating_sub(3)
            }
            _ => return None,
        }
        Some(None)
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        hits.wheel(area, super::input::Wheel::Tools);
        if self.configuring {
            self.form(frame, area, hits);
            return;
        }
        if self.preview.is_some() {
            let warning = "Risk: config/plugins may execute code; caches/logs may change. Not a sandbox. Esc returns to draft. PgUp/PgDn scroll exact query.";
            ui::prose(
                frame,
                Rect::new(area.x, area.y, area.width, 2),
                warning,
                ui::warning(),
            );
            let height = area.height.saturating_sub(5);
            ui::report(
                frame,
                Rect::new(area.x, area.y + 2, area.width, height),
                "Exact provider query · no installation",
                ui::review_text(&self.scope),
                self.scroll,
                false,
            );
            ui::input(
                frame,
                Rect::new(area.x, area.bottom().saturating_sub(3), area.width, 3),
                if dry_run {
                    "Preview only · execution disabled"
                } else {
                    "Type inspect then Enter · config/cache risks accepted"
                },
                &self.confirmation,
                !dry_run,
            );
            return;
        }
        let Some(report) = &self.report else {
            ui::prose(
                frame,
                area,
                "Tools · explicit provider inspection\n\nNo provider has been queried. Config/plugins may execute code and caches/logs may change.\n\nc: configure trusted paths, then review exact arguments and type inspect. No installs, ownership inference or automatic refresh.\n\nHomebrew formulae · apt via dpkg-query · mise · npm with explicit Node/CLI/prefix binding.\n\nUse Session > Preview-only for a process-free review.",
                ui::muted(),
            );
            return;
        };
        if self.details {
            let selected = match &report.provider.observation {
                Observation::Complete { installations } => self.matching.get(self.selected).map(|i| format!("Selected provider claim (not binary ownership):\nPackage: {}\nVersion: {}\n\n", installations[*i].package, installations[*i].version)).unwrap_or_default(),
                _ => String::new(),
            };
            ui::report(
                frame,
                area,
                "Inspection scope · Esc/Enter returns · PgUp/PgDn scroll",
                ui::review_text(&format!("{selected}{}", self.scope)),
                self.scroll,
                false,
            );
            return;
        }
        ui::prose(
            frame,
            Rect::new(area.x, area.y, area.width, 1),
            &format!(
                "Provider claims · {:?} · {}",
                report.provider.description.query,
                report.provider.description.executable.display()
            ),
            ui::heading(),
        );
        ui::input(
            frame,
            Rect::new(area.x, area.y + 1, area.width, 3),
            "Search metadata · / edits · c configure · r fresh review · d scope",
            &self.search,
            self.searching,
        );
        let rows = Rect::new(
            area.x,
            area.y + 4,
            area.width,
            area.height.saturating_sub(4),
        );
        match &report.provider.observation {
            Observation::Unavailable { reason } => ui::prose(
                frame,
                rows,
                &format!(
                    "UNAVAILABLE: {reason}\nNot a healthy empty inventory. r prepares a fresh review; it never replays approval."
                ),
                ui::warning(),
            ),
            Observation::Complete { installations } => {
                if installations.is_empty() {
                    ui::prose(
                        frame,
                        rows,
                        "Query complete · no installed packages reported. Not proof of binary absence or Dort ownership.",
                        ui::muted(),
                    );
                    return;
                }
                if self.matching.is_empty() {
                    ui::prose(
                        frame,
                        rows,
                        "No matches · Esc clears the display filter.",
                        ui::muted(),
                    );
                    return;
                }
                let capacity = usize::from(rows.height.saturating_sub(1)).max(1);
                self.capacity.set(capacity);
                let offset = self.selected.saturating_sub(capacity - 1);
                let mut lines = vec![Line::styled(
                    format!(
                        "{} shown / {} provider claims · package | version",
                        self.matching.len(),
                        installations.len()
                    ),
                    ui::muted(),
                )];
                for (position, index) in
                    self.matching.iter().enumerate().skip(offset).take(capacity)
                {
                    let entry = &installations[*index];
                    hits.click(
                        Rect::new(
                            rows.x,
                            rows.y + 1 + (position - offset) as u16,
                            rows.width,
                            1,
                        ),
                        Target::ToolsRow(position),
                    );
                    lines.push(Line::styled(
                        ui::clip(
                            &format!("{} | {}", entry.package, entry.version),
                            rows.width,
                        ),
                        if position == self.selected {
                            ui::selected()
                        } else {
                            ui::muted()
                        },
                    ));
                }
                frame.render_widget(Paragraph::new(lines), rows);
            }
        }
    }
    fn form(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let controls = self.controls();
        let budget = usize::from(area.height.saturating_sub(1));
        let position = controls.iter().position(|i| *i == self.focus).unwrap_or(0);
        let height = |id: usize| if id < 7 { 3 } else { 1 };
        let mut start = 0;
        let mut used = 0;
        for i in (0..=position).rev() {
            if used + height(controls[i]) > budget {
                break;
            }
            used += height(controls[i]);
            start = i;
        }
        if controls.iter().map(|id| height(*id)).sum::<usize>() <= budget {
            start = 0;
        }
        let mut y = area.y;
        for &id in controls.iter().skip(start) {
            let h = height(id) as u16;
            if y + h > area.bottom().saturating_sub(1) {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, h);
            hits.click(rect, Target::ToolsControl(id, id >= 7));
            if id < 7 {
                let provider = format!("{:?}", QUERIES[self.query]);
                ui::input(
                    frame,
                    rect,
                    LABELS[id],
                    if id == 0 {
                        &provider
                    } else {
                        &self.fields[id - 1]
                    },
                    self.focus == id,
                );
            } else {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        if id == 7 {
                            "[Review exact query · Enter]"
                        } else {
                            "[Back · Enter]"
                        },
                        if self.focus == id {
                            ui::selected()
                        } else {
                            ui::accent()
                        },
                    ))),
                    rect,
                );
            }
            y += h;
        }
        ui::prose(
            frame,
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            "Tab/Shift-Tab move · Enter advances · Esc back · Ctrl-U clears",
            ui::muted(),
        );
    }
}
