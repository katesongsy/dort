//! Reconciliation forms and typed metadata results. Never starts a provider query.
use super::{components as ui, input::Target, shell::edit_text};
use crate::services::reconciliation::{self, Prepared, Snapshot};
use dort_core::tools::planning::{PackageAction, PackageFinding};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::Line,
    widgets::Paragraph,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Inputs,
    Bindings,
    Results,
}
pub enum Outcome {
    Back,
    Install(String),
    FreshInventory,
    Load(reconciliation::Request),
    Calculate {
        prepared: Box<Prepared>,
        bindings: BTreeMap<String, String>,
    },
}
pub struct State {
    pub fields: [String; 3],
    pub focus: usize,
    pub mode: Mode,
    pub bindings: BTreeMap<String, String>,
    pub stale_for_install: BTreeSet<String>,
    pub result: Option<reconciliation::Report>,
    pub failure: Option<String>,
    pub selected: usize,
    pub details: bool,
    binding_details: bool,
    pub scroll: usize,
    prepared: Option<Prepared>,
    detail: String,
    capacity: Cell<usize>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            fields: Default::default(),
            focus: 0,
            mode: Mode::Inputs,
            bindings: BTreeMap::new(),
            stale_for_install: BTreeSet::new(),
            result: None,
            failure: None,
            selected: 0,
            details: false,
            binding_details: false,
            scroll: 0,
            prepared: None,
            detail: String::new(),
            capacity: Cell::new(1),
        }
    }
}
impl State {
    fn may_install(&self, row: &dort_core::tools::planning::Row) -> bool {
        crate::services::installation::eligible(row)
            && self
                .result
                .as_ref()
                .and_then(|r| r.bindings.get(&row.id))
                .is_some_and(|id| !self.stale_for_install.contains(id))
    }
    fn needs_inventory(&self) -> bool {
        self.result
            .as_ref()
            .and_then(|r| r.plan.rows.get(self.selected))
            .is_some_and(|r| crate::services::installation::eligible(r) && !self.may_install(r))
    }
    pub fn text_entry(&self) -> bool {
        self.mode != Mode::Results
    }
    pub fn ready(&mut self, prepared: Prepared) {
        self.bindings
            .retain(|id, _| prepared.tools.iter().any(|t| &t.id == id));
        self.prepared = Some(prepared);
        self.mode = Mode::Bindings;
        self.focus = 0;
        self.failure = None;
        self.binding_details = false;
    }
    pub fn completed(&mut self, report: Result<reconciliation::Report, String>) {
        self.prepared = None;
        self.binding_details = false;
        self.details = false;
        self.selected = 0;
        self.scroll = 0;
        self.detail.clear();
        match report {
            Ok(report) => {
                self.result = Some(report);
                self.mode = Mode::Results;
                self.failure = None;
            }
            Err(error) => {
                self.result = None;
                self.mode = Mode::Inputs;
                self.failure = Some(error);
                self.focus = 3;
            }
        }
    }
    pub fn invalidate(&mut self) {
        self.result = None;
        self.detail.clear();
        self.prepared = None;
        self.mode = Mode::Inputs;
        self.binding_details = false;
    }
    fn load(&mut self) -> Outcome {
        self.result = None;
        self.failure = None;
        self.prepared = None;
        Outcome::Load(self.request())
    }
    pub fn request(&self) -> reconciliation::Request {
        reconciliation::Request {
            manifest: self.fields[0].clone().into(),
            tool_search_dirs: self.fields[1].split(':').map(Into::into).collect(),
            profiles: self.fields[2]
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }
    pub fn key(&mut self, key: KeyEvent, snapshots: &[Snapshot]) -> Option<Outcome> {
        if self.binding_details {
            match key.code {
                KeyCode::Esc | KeyCode::Char('d') => self.binding_details = false,
                KeyCode::Home => self.scroll = 0,
                KeyCode::End => self.scroll = usize::MAX,
                KeyCode::PageDown | KeyCode::Down => {
                    self.scroll = self.scroll.saturating_add(self.capacity.get().max(1))
                }
                KeyCode::PageUp | KeyCode::Up => {
                    self.scroll = self.scroll.saturating_sub(self.capacity.get().max(1))
                }
                _ => (),
            }
            return None;
        }
        if self.mode == Mode::Inputs {
            match key.code {
                KeyCode::Esc => return Some(Outcome::Back),
                KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % 5,
                KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + 4) % 5,
                KeyCode::Enter if self.focus == 3 => return Some(self.load()),
                KeyCode::Enter if self.focus == 4 => return Some(Outcome::Back),
                KeyCode::Enter => self.focus = (self.focus + 1) % 5,
                _ if self.focus < 3 => edit_text(&mut self.fields[self.focus], key),
                _ => (),
            }
            return None;
        }
        if self.mode == Mode::Bindings {
            let prepared = self.prepared.as_ref()?;
            let count = prepared.tools.len();
            match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Inputs;
                    self.focus = 3;
                }
                KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % (count + 2),
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = (self.focus + count + 1) % (count + 2)
                }
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.focus < count => {
                    let tool = &prepared.tools[self.focus];
                    let ids: Vec<_> = snapshots
                        .iter()
                        .filter(|s| {
                            Some(s.report.provider.description.query.provider()) == tool.provider
                        })
                        .map(|s| s.id.clone())
                        .collect();
                    let position = self
                        .bindings
                        .get(&tool.id)
                        .and_then(|id| ids.iter().position(|s| s == id))
                        .map_or(0, |i| i + 1);
                    let next = (position
                        + if key.code == KeyCode::Left {
                            ids.len()
                        } else {
                            1
                        })
                        % (ids.len() + 1);
                    if next == 0 {
                        self.bindings.remove(&tool.id);
                    } else {
                        self.bindings.insert(tool.id.clone(), ids[next - 1].clone());
                    }
                }
                KeyCode::Char('d') if self.focus < count => {
                    let tool = &prepared.tools[self.focus];
                    if let Some(snapshot) = self
                        .bindings
                        .get(&tool.id)
                        .and_then(|id| snapshots.iter().find(|s| &s.id == id))
                    {
                        self.detail = super::views::safe(&serde_json::to_string_pretty(&serde_json::json!({"snapshot":&snapshot.id,"review":crate::services::tools::Review {manifest:snapshot.report.manifest.as_deref(),provider:&snapshot.report.provider.description}})).expect("metadata serializes"));
                        self.binding_details = true;
                        self.scroll = 0;
                    }
                }
                KeyCode::Enter if self.focus == count => {
                    let prepared = Box::new(self.prepared.take()?);
                    self.mode = Mode::Results;
                    self.result = None;
                    return Some(Outcome::Calculate {
                        prepared,
                        bindings: self.bindings.clone(),
                    });
                }
                KeyCode::Enter if self.focus == count + 1 => {
                    self.mode = Mode::Inputs;
                    self.focus = 3;
                }
                KeyCode::Enter => self.focus = (self.focus + 1) % (count + 2),
                _ => (),
            }
            return None;
        }
        let count = self.result.as_ref().map_or(0, |r| r.plan.rows.len());
        match key.code {
            KeyCode::Esc if self.details => {
                self.details = false;
                self.scroll = 0;
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                self.mode = Mode::Inputs;
                self.focus = 3;
            }
            KeyCode::Char('c') if !self.details && self.needs_inventory() => {
                return Some(Outcome::FreshInventory);
            }
            KeyCode::Char('i') if !self.details => {
                if let Some(row) = self
                    .result
                    .as_ref()
                    .and_then(|r| r.plan.rows.get(self.selected))
                    && self.may_install(row)
                {
                    return Some(Outcome::Install(row.id.clone()));
                }
            }
            KeyCode::Char('d') | KeyCode::Enter if count > 0 => {
                self.details = !self.details;
                self.scroll = 0;
                self.cache_detail();
            }
            KeyCode::Home if self.details => self.scroll = 0,
            KeyCode::Home => self.selected = 0,
            KeyCode::End if self.details => self.scroll = usize::MAX,
            KeyCode::End => self.selected = count.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if !self.details => {
                self.selected = self.selected.saturating_add(1).min(count.saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') if !self.details => {
                self.selected = self.selected.saturating_sub(1)
            }
            KeyCode::PageDown if !self.details => {
                self.selected = self
                    .selected
                    .saturating_add(self.capacity.get())
                    .min(count.saturating_sub(1))
            }
            KeyCode::PageUp if !self.details => {
                self.selected = self.selected.saturating_sub(self.capacity.get())
            }
            KeyCode::Down | KeyCode::PageDown if self.details => {
                self.scroll = self.scroll.saturating_add(self.capacity.get().max(1))
            }
            KeyCode::Up | KeyCode::PageUp if self.details => {
                self.scroll = self.scroll.saturating_sub(self.capacity.get().max(1))
            }
            _ => (),
        }
        None
    }
    fn cache_detail(&mut self) {
        let Some(report) = &self.result else {
            return;
        };
        let Some(row) = report.plan.rows.get(self.selected) else {
            return;
        };
        self.detail = super::views::safe(&serde_json::to_string_pretty(&serde_json::json!({
            "analysis_scope": &report.scope, "snapshot": report.bindings.get(&row.id), "package_plan": row,
            "binary_metadata": report.binaries.tools.get(&row.id), "authority": "Snapshot claims only; not tool readiness, ownership or execution authorization"
        })).expect("metadata serializes"));
    }
    pub fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        snapshots: &[Snapshot],
        hits: &super::input::Hits,
    ) {
        if self.binding_details {
            self.draw_detail(frame, area);
            return;
        }
        if self.mode != Mode::Results {
            self.form(frame, area, snapshots, hits);
            return;
        }
        if self.details {
            self.draw_detail(frame, area);
            return;
        }
        let Some(report) = &self.result else {
            ui::prose(
                frame,
                area,
                "Reconciliation metadata unavailable. Esc returns to retained inputs.",
                ui::warning(),
            );
            return;
        };
        let header = if !report.binaries.available() {
            "Binary metadata incomplete"
        } else if report.plan.has_blockers() {
            "Package plan blocked"
        } else {
            "Package proposals/no-ops only"
        };
        ui::prose(
            frame,
            Rect::new(area.x, area.y, area.width, 2),
            &format!(
                "{header} · no install authority\nEnter/d details · Esc inputs · claims are not tool readiness"
            ),
            ui::heading(),
        );
        let eligible = report
            .plan
            .rows
            .get(self.selected)
            .is_some_and(|r| self.may_install(r));
        let needs_inventory = self.needs_inventory();
        let capacity = usize::from(area.height.saturating_sub(if eligible || needs_inventory {
            4
        } else {
            3
        }))
        .max(1);
        self.capacity.set(capacity);
        let offset = self.selected.saturating_sub(capacity - 1);
        let mut lines = vec![Line::styled(
            "TOOL | PACKAGE CLAIM | PACKAGE ACTION | BLOCKERS",
            ui::muted(),
        )];
        for (index, row) in report
            .plan
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(capacity)
        {
            let finding = match row.package {
                PackageFinding::UnsupportedPlatform => "Unsupported",
                PackageFinding::NotObserved => "Not observed",
                PackageFinding::Unavailable { .. } => "Unavailable",
                PackageFinding::ScopeMismatch { .. } => "Scope mismatch",
                PackageFinding::AmbiguousIdentity => "Ambiguous",
                PackageFinding::InvalidMetadata => "Invalid metadata",
                PackageFinding::Absent => "Absent in scope",
                PackageFinding::Present { .. } => "Claimed installed",
            };
            let action = match row.action {
                PackageAction::NoChange => "Package no-op",
                PackageAction::ReviewMissingPackage => "Review only",
                PackageAction::Blocked => "Blocked",
            };
            lines.push(Line::styled(
                ui::clip(
                    &format!("{} | {finding} | {action} | {}", row.id, row.blockers.len()),
                    area.width,
                ),
                if index == self.selected {
                    ui::selected()
                } else {
                    ui::muted()
                },
            ));
        }
        if needs_inventory {
            let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(ui::button("Collect fresh inventory", "c", true))),
                button,
            );
            hits.click(button, Target::ToolInstallCollect);
        }
        if eligible {
            let button = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(ui::button(
                    "Review exact mise install",
                    "i",
                    true,
                ))),
                button,
            );
            hits.click(button, Target::ToolInstallSelected);
        }
        if report.plan.rows.is_empty() {
            lines.push(Line::raw(
                "No selected tools declared. No provider was refreshed.",
            ));
        }
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(
                area.x,
                area.y + 2,
                area.width,
                area.height
                    .saturating_sub(if eligible || needs_inventory { 3 } else { 2 }),
            ),
        );
    }
    fn form(
        &self,
        frame: &mut Frame,
        area: Rect,
        snapshots: &[Snapshot],
        hits: &super::input::Hits,
    ) {
        let mut fields: Vec<(String, String)> = if self.mode == Mode::Inputs {
            [
                "Manifest · absolute",
                "Tool search directories · colon-separated",
                "Temporary profiles · comma-separated",
            ]
            .iter()
            .enumerate()
            .map(|(i, label)| ((*label).into(), self.fields[i].clone()))
            .collect()
        } else {
            self.prepared
                .as_ref()
                .map(|p| {
                    p.tools
                        .iter()
                        .map(|t| {
                            let value = self
                                .bindings
                                .get(&t.id)
                                .cloned()
                                .unwrap_or_else(|| "Not observed".into());
                            let value = if value != "Not observed"
                                && !snapshots.iter().any(|s| s.id == value)
                            {
                                format!("{value} (forgotten; choose again)")
                            } else {
                                value
                            };
                            (
                                format!(
                                    "{} ({}) · {:?} · Space chooses",
                                    t.id, t.binary, t.provider
                                ),
                                value,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let field_count = fields.len();
        fields.push((
            if self.mode == Mode::Inputs {
                "Load declarations · Enter"
            } else {
                "Reconcile metadata · Enter"
            }
            .into(),
            String::new(),
        ));
        fields.push(("Back · Enter".into(), String::new()));
        let reserved = if self.failure.is_some() { 2 } else { 1 };
        let budget = usize::from(area.height.saturating_sub(reserved));
        let height = |i: usize| if i < field_count { 3 } else { 1 };
        let focus = self.focus.min(fields.len() - 1);
        let mut start = 0;
        let mut used = 0;
        for i in (0..=focus).rev() {
            if used + height(i) > budget {
                break;
            }
            used += height(i);
            start = i;
        }
        if (0..fields.len()).map(height).sum::<usize>() <= budget {
            start = 0;
        }
        let mut y = area.y;
        for (i, (label, value)) in fields.iter().enumerate().skip(start) {
            let h = height(i) as u16;
            if y + h > area.bottom().saturating_sub(reserved) {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, h);
            hits.click(rect, Target::ToolsReconControl(i, i >= field_count));
            if i < field_count {
                ui::input(frame, rect, label, value, i == focus);
            } else {
                frame.render_widget(
                    Paragraph::new(Line::from(ui::button(label, "", i == focus))),
                    rect,
                );
            }
            y += h;
        }
        let message = self
            .failure
            .as_deref()
            .unwrap_or(if self.mode == Mode::Inputs {
                "Metadata only · no query. Tool dirs are NOT provider child PATH."
            } else {
                "Space chooses · d inspects scope · unbound stays not observed."
            });
        ui::prose(
            frame,
            Rect::new(
                area.x,
                area.bottom().saturating_sub(reserved),
                area.width,
                reserved,
            ),
            message,
            if self.failure.is_some() {
                ui::warning()
            } else {
                ui::muted()
            },
        );
    }
    fn draw_detail(&self, frame: &mut Frame, area: Rect) {
        let block = ui::pane("Full metadata · PgUp/PgDn · Home/End · Esc", false);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let width = usize::from(inner.width).max(1);
        let height = usize::from(inner.height);
        if height == 0 {
            return;
        }
        self.capacity.set(height.max(1));
        let total: usize = self
            .detail
            .lines()
            .map(|l| l.len().div_ceil(width).max(1))
            .sum();
        let start = self.scroll.min(total.saturating_sub(height));
        let mut position = 0;
        let mut visible = vec![];
        'lines: for line in self.detail.lines() {
            for chunk in 0..line.len().div_ceil(width).max(1) {
                if position >= start {
                    let begin = (chunk * width).min(line.len());
                    let end = (begin + width).min(line.len());
                    visible.push(Line::raw(line[begin..end].to_owned()));
                    if visible.len() == height {
                        break 'lines;
                    }
                }
                position += 1;
            }
        }
        frame.render_widget(Paragraph::new(visible), inner);
    }
}
