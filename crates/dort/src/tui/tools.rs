//! Explicit provider inspection, independent of repository observation and write approvals.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::{
    reconciliation::{self, Snapshot},
    tools::{self, Request},
};
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
    pub report: Option<std::sync::Arc<tools::Report>>,
    pub snapshots: Vec<Snapshot>,
    pub current: usize,
    next_id: u64,
    pub catalog_browser: bool,
    pub recon_open: bool,
    pub reconciliation: super::tools_reconcile::State,
    pub notice: Option<&'static str>,
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
            snapshots: vec![],
            current: 0,
            next_id: 1,
            catalog_browser: false,
            recon_open: false,
            reconciliation: Default::default(),
            notice: None,
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
        self.configuring
            || self.preview.is_some()
            || self.searching
            || (self.recon_open && self.reconciliation.text_entry())
    }
    pub fn configure(&mut self) {
        self.leave();
        self.notice = None;
        self.configuring = true;
        self.focus = 0;
    }
    pub fn leave(&mut self) {
        self.preview = None;
        self.confirmation.clear();
        self.configuring = false;
        self.searching = false;
        self.catalog_browser = false;
        self.recon_open = false;
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
        if self.snapshots.len() >= reconciliation::MAX_SNAPSHOTS
            || reconciliation::snapshot_records(&self.snapshots)
                >= reconciliation::MAX_SNAPSHOT_RECORDS
            || self.next_id == u64::MAX
        {
            self.leave();
            self.notice = Some(
                "Session snapshot limit reached. s opens snapshots; x forgets metadata before a fresh query.",
            );
            return;
        }
        self.notice = None;
        self.scope = serde_json::to_string_pretty(&preview.review())
            .expect("inspection metadata serializes");
        self.preview = Some(preview);
        self.confirmation.clear();
        self.configuring = false;
        self.scroll = 0;
    }
    pub fn completed(&mut self, report: tools::Report) -> bool {
        self.leave();
        let count = match &report.provider.observation {
            Observation::Complete { installations } => installations.len(),
            _ => 0,
        };
        if self.snapshots.len() >= reconciliation::MAX_SNAPSHOTS
            || reconciliation::snapshot_records(&self.snapshots).saturating_add(count)
                > reconciliation::MAX_SNAPSHOT_RECORDS
        {
            self.select_snapshot(self.current);
            self.notice = Some(
                "Inspection ran, but its metadata exceeded the session limit and was not stored. Forget metadata before a fresh approved query.",
            );
            return false;
        }
        self.snapshots.push(Snapshot {
            id: format!("s{}", self.next_id),
            report: std::sync::Arc::new(report),
        });
        self.next_id += 1;
        self.notice = None;
        self.reconciliation.invalidate();
        self.select_snapshot(self.snapshots.len() - 1);
        true
    }
    fn select_snapshot(&mut self, index: usize) {
        self.notice = None;
        self.current = index.min(self.snapshots.len().saturating_sub(1));
        self.report = self.snapshots.get(self.current).map(|s| s.report.clone());
        self.scope.clear();
        if let Some(report) = &self.report {
            self.scope = serde_json::to_string_pretty(&tools::Review {
                manifest: report.manifest.as_deref(),
                provider: &report.provider.description,
            })
            .expect("inspection metadata serializes");
        }
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
        if self.recon_open {
            let outcome = self.reconciliation.key(key, &self.snapshots);
            return Some(match outcome {
                Some(super::tools_reconcile::Outcome::Back) => {
                    self.recon_open = false;
                    None
                }
                Some(super::tools_reconcile::Outcome::Load(request)) => {
                    Some(Task::ToolsReconcilePrepare(request))
                }
                Some(super::tools_reconcile::Outcome::Calculate { prepared, bindings }) => {
                    Some(Task::ToolsReconcile {
                        prepared,
                        bindings,
                        snapshots: self.snapshots.clone(),
                    })
                }
                None => None,
            });
        }
        if self.catalog_browser {
            match key.code {
                KeyCode::Esc => self.catalog_browser = false,
                KeyCode::Up => self.current = self.current.saturating_sub(1),
                KeyCode::Down => {
                    self.current = (self.current + 1).min(self.snapshots.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    self.select_snapshot(self.current);
                    self.catalog_browser = false;
                }
                KeyCode::Char('x') if !self.snapshots.is_empty() => {
                    self.snapshots.remove(self.current);
                    self.reconciliation.invalidate();
                    self.select_snapshot(self.current);
                }
                _ => (),
            }
            return Some(None);
        }
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
            KeyCode::Char('s') => self.catalog_browser = true,
            KeyCode::Char('p') => {
                self.recon_open = true;
            }
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
        if self.recon_open {
            self.reconciliation.draw(frame, area, &self.snapshots, hits);
            return;
        }
        if self.catalog_browser {
            let mut lines = vec![Line::styled(
                "Session snapshots · Enter selects · x forgets metadata · Esc back",
                ui::heading(),
            )];
            let capacity = usize::from(area.height.saturating_sub(2)).max(1);
            let offset = self.current.saturating_sub(capacity - 1);
            for (i, snapshot) in self
                .snapshots
                .iter()
                .enumerate()
                .skip(offset)
                .take(capacity)
            {
                let d = &snapshot.report.provider.description;
                lines.push(Line::styled(
                    ui::clip(
                        &format!(
                            "{} | {:?} | {} | {}",
                            snapshot.id,
                            d.query,
                            if snapshot.report.available() {
                                "Complete"
                            } else {
                                "Unavailable"
                            },
                            d.executable.display()
                        ),
                        area.width,
                    ),
                    if i == self.current {
                        ui::selected()
                    } else {
                        ui::muted()
                    },
                ));
            }
            if self.snapshots.is_empty() {
                lines.push(Line::raw(
                    "No snapshots collected. Esc then c configures an explicit query.",
                ));
            }
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        if let Some(notice) = self.notice {
            ui::prose(frame, area, notice, ui::warning());
            return;
        }
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
            if self.next_id > 1 {
                ui::prose(
                    frame,
                    area,
                    "No snapshots retained. Earlier inspections may have run; forgetting metadata does not undo provider side effects.\n\nc configures a fresh approved query; s browses snapshots; p reconciles explicit bindings without provider queries.",
                    ui::muted(),
                );
                return;
            }
            ui::prose(
                frame,
                area,
                "Tools · explicit provider inspection\n\nNo provider has been queried. Config/plugins may execute code and caches/logs may change.\n\nc: configure trusted paths, review and type inspect. s: session snapshots; p: reconcile explicit bindings. No installs, ownership inference or automatic refresh.\n\nHomebrew formulae · apt via dpkg-query · mise · npm with explicit Node/CLI/prefix binding.\n\nUse Session > Preview-only for a process-free review.",
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
                "Provider claims · Snapshot {} · {:?} · {}",
                self.snapshots
                    .get(self.current)
                    .map(|s| s.id.as_str())
                    .unwrap_or("unknown"),
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
