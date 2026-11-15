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
    text::Line,
    widgets::Paragraph,
};
use std::{cell::Cell, collections::BTreeMap, path::PathBuf};

const QUERIES: [Query; 4] = [
    Query::BrewFormulae,
    Query::DpkgPackages,
    Query::MiseInstalled,
    Query::NpmGlobals,
];
const INVENTORY_ACTIONS: [(&str, KeyCode, &str); 9] = [
    ("Add selected to repository", KeyCode::Char('t'), "Draft"),
    ("Inspection details", KeyCode::Char('d'), "Metadata"),
    ("Inspect again", KeyCode::Char('r'), "Review"),
    ("Inspection settings", KeyCode::Char('e'), "Draft"),
    ("Filter packages", KeyCode::Char('/'), "Display"),
    ("Compare declared tools", KeyCode::Char('p'), "Read-only"),
    ("Add declaration", KeyCode::Char('a'), "Draft"),
    ("Previous inspections", KeyCode::Char('s'), "Metadata"),
    ("Installation attempts", KeyCode::Char('i'), "Metadata"),
];
const LABELS: [&str; 7] = [
    "Provider",
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
    pub config_details: bool,
    pub setup: bool,
    pub defaults: Option<crate::services::inspection_defaults::Defaults>,
    pub default_focus: usize,
    pub default_feedback: Option<&'static str>,
    pub default_query: usize,
    pub collection_result: bool,
    pub fresh_review: bool,
    drafts: [Option<[String; 6]>; 4],
    pub focus: usize,
    pub preview: Option<tools::Preview>,
    pub report: Option<std::sync::Arc<tools::Report>>,
    pub snapshots: Vec<Snapshot>,
    pub current: usize,
    next_id: u64,
    pub catalog_browser: bool,
    pub recon_open: bool,
    pub install_open: bool,
    pub installation: super::tools_install::State,
    pub declaration_open: bool,
    pub declaration: super::tool_declaration::State,
    pub platform: Option<dort_core::platform::Platform>,
    declaration_key: Option<String>,
    saved_declarations: BTreeMap<String, super::tool_declaration::State>,
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
            config_details: false,
            setup: false,
            defaults: None,
            default_focus: 0,
            default_feedback: None,
            default_query: 0,
            collection_result: false,
            fresh_review: false,
            drafts: Default::default(),
            focus: 0,
            preview: None,
            report: None,
            snapshots: vec![],
            current: 0,
            next_id: 1,
            catalog_browser: false,
            recon_open: false,
            install_open: false,
            installation: Default::default(),
            declaration_open: false,
            declaration: Default::default(),
            platform: None,
            declaration_key: None,
            saved_declarations: BTreeMap::new(),
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
            (0..7).collect()
        } else {
            vec![0, 1, 2]
        }
    }
    pub fn text_entry(&self) -> bool {
        (self.declaration_open && !self.declaration.details)
            || (self.install_open && self.installation.text_entry())
            || (self.configuring && !self.config_details)
            || self.searching
            || (self.recon_open && self.reconciliation.text_entry())
    }
    pub fn configure(&mut self) -> Task {
        if self.drafts[self.query].is_some() || self.fields.iter().any(|f| !f.is_empty()) {
            self.drafts[self.query] = Some(self.fields.clone());
        }
        self.leave();
        self.notice = None;
        self.setup = true;
        self.default_focus = self.query;
        self.default_feedback = None;
        self.default_query = self.query;
        self.details = false;
        let mut inputs = crate::environment::inspection_inputs();
        inputs.overrides = self
            .drafts
            .iter()
            .enumerate()
            .filter_map(|(query, fields)| {
                fields
                    .as_ref()
                    .filter(|f| f.iter().any(|v| !v.is_empty()))
                    .map(|f| Self::request_for(query, f))
            })
            .collect();
        Task::ToolsDetect(inputs)
    }
    pub fn advanced(&mut self) {
        self.notice = None;
        self.setup = false;
        self.configuring = true;
        self.config_details = false;
        self.focus = 0;
    }
    pub fn defaults_ready(&mut self, defaults: crate::services::inspection_defaults::Defaults) {
        if !defaults.choices.iter().any(|c| c.selected) {
            self.default_query = defaults
                .choices
                .iter()
                .position(|c| {
                    c.request
                        .as_ref()
                        .is_some_and(|r| !r.executable.as_os_str().is_empty())
                })
                .unwrap_or(0);
            self.default_focus = self.default_query;
        }
        if let Some(selected) = defaults.choices.iter().position(|c| c.selected) {
            self.default_focus = selected;
        }
        self.default_focus = self.default_focus.min(3);
        self.default_query = self.default_focus;
        self.defaults = Some(defaults);
        self.scroll = 0;
    }
    pub fn select_advanced(&mut self) {
        let query = if self.default_focus < 4 {
            self.default_focus
        } else {
            self.default_query
        };
        let retained = self.drafts[query].is_some()
            || (query == self.query && self.fields.iter().any(|f| !f.is_empty()));
        if !self.fresh_review
            || self.drafts[self.query].is_some()
            || self.fields.iter().any(|f| !f.is_empty())
        {
            self.drafts[self.query] = Some(self.fields.clone());
        }
        self.query = query;
        self.fields = self.drafts[query].clone().unwrap_or_default();
        if let Some(request) = self
            .defaults
            .as_ref()
            .and_then(|d| d.choices[query].request.as_ref())
            && !(self.fresh_review && retained)
        {
            let defaults = [
                request.executable.display().to_string(),
                request
                    .search_dirs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
                request
                    .manifest
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                request.runtime_tool.clone().unwrap_or_default(),
                request
                    .npm_cli
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                request
                    .prefix
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ];
            for (field, value) in self.fields.iter_mut().zip(defaults) {
                if field.is_empty() {
                    *field = value;
                }
            }
        }
        self.advanced();
        // Land on the genuinely missing detail, not a blank seven-field form.
        if let Some(missing) = self
            .fields
            .iter()
            .take(if self.query == 3 { 6 } else { 2 })
            .position(|f| f.is_empty())
        {
            self.focus = missing + 1;
        }
    }
    fn approve_selected(&mut self, dry_run: bool) -> Option<Task> {
        if dry_run {
            self.default_feedback = Some("Preview only: inventory execution is disabled.");
            return None;
        }
        let defaults = self.defaults.as_mut()?;
        let count = defaults
            .choices
            .iter()
            .filter(|c| c.selected && c.preview.is_some())
            .count();
        if count == 0 {
            self.default_feedback = Some(
                "No inventories selected. Space selects available rows; e edits missing paths.",
            );
            return None;
        }
        if self.snapshots.len().saturating_add(count) > reconciliation::MAX_SNAPSHOTS
            || reconciliation::snapshot_records(&self.snapshots)
                >= reconciliation::MAX_SNAPSHOT_RECORDS
            || self.next_id.checked_add(count as u64).is_none()
        {
            self.notice = Some(
                "Session snapshot limit reached. Esc, then s and x forget metadata before a fresh inspection.",
            );
            return None;
        }
        let approvals = defaults
            .choices
            .iter_mut()
            .filter(|c| c.selected)
            .filter_map(|c| c.preview.take())
            .map(tools::Preview::approve)
            .collect();
        self.leave();
        self.report = None;
        Some(Task::ToolsInspectSelected(approvals))
    }
    pub fn browsing_inventory(&self) -> bool {
        self.report.is_some()
            && !self.setup
            && !self.configuring
            && self.preview.is_none()
            && !self.catalog_browser
            && !self.recon_open
            && !self.install_open
            && !self.declaration_open
            && !self.details
    }
    pub fn leave(&mut self) {
        self.config_details = false;
        self.fresh_review = false;
        self.preview = None;
        self.defaults = None;
        self.setup = false;
        self.configuring = false;
        self.searching = false;
        self.catalog_browser = false;
        self.recon_open = false;
        self.install_open = false;
        self.installation.clear_approvals();
        self.declaration_open = false;
    }
    fn switch_declaration(&mut self, key: Option<String>) {
        if self.declaration_key != key {
            let old = std::mem::take(&mut self.declaration);
            self.saved_declarations
                .insert(self.declaration_key.take().unwrap_or_default(), old);
            self.declaration = self
                .saved_declarations
                .remove(key.as_deref().unwrap_or_default())
                .unwrap_or_default();
            self.declaration_key = key;
        }
        self.leave();
        self.declaration_open = true;
    }
    pub fn declare(&mut self) {
        self.switch_declaration(None);
    }
    fn can_declare_selected(&self) -> bool {
        use dort_core::platform::Platform;
        let Some(platform) = self.platform else {
            return false;
        };
        let Some(report) = &self.report else {
            return false;
        };
        let compatible = match report.provider.description.query {
            Query::BrewFormulae => platform == Platform::Macos,
            Query::DpkgPackages => platform == Platform::Ubuntu,
            Query::MiseInstalled | Query::NpmGlobals => true,
        };
        compatible && self.browsing_inventory() && !self.matching.is_empty()
    }
    fn declare_selected(&mut self) {
        if !self.can_declare_selected() {
            return;
        }
        let Some(platform) = self.platform else {
            return;
        };
        let Some(report) = self.report.as_ref() else {
            return;
        };
        let Observation::Complete { installations } = &report.provider.observation else {
            return;
        };
        let Some(entry) = self
            .matching
            .get(self.selected)
            .and_then(|&i| installations.get(i))
        else {
            return;
        };
        let query = report.provider.description.query;
        let package = entry.package.clone();
        let version = entry.version.clone();
        let Some(snapshot) = self.snapshots.get(self.current) else {
            return;
        };
        let key = format!("{}\0{query:?}\0{package}\0{version}", snapshot.id);
        let new = self.declaration_key.as_ref() != Some(&key)
            && !self.saved_declarations.contains_key(&key);
        if new && self.saved_declarations.len() >= 128 {
            self.notice =
                Some("Declaration draft limit reached; retained drafts were not replaced.");
            return;
        }
        self.switch_declaration(Some(key));
        if new {
            self.declaration
                .seed_inspection(platform, query, package, version);
        }
    }
    pub fn shrink(&mut self) {
        self.installation.clear_approvals();
        if self.setup {
            self.defaults = None;
            self.setup = false;
        }
        if self.preview.take().is_some() {
            self.configuring = true;
        }
    }
    fn request(&self) -> Request {
        Self::request_for(self.query, &self.fields)
    }
    fn request_for(query: usize, fields: &[String; 6]) -> Request {
        let npm = query == 3;
        Request {
            query: QUERIES[query],
            executable: fields[0].clone().into(),
            search_dirs: fields[1].split(':').map(PathBuf::from).collect(),
            manifest: npm.then(|| PathBuf::from(&fields[2])),
            runtime_tool: npm.then(|| fields[3].clone()),
            npm_cli: npm.then(|| PathBuf::from(&fields[4])),
            prefix: npm.then(|| PathBuf::from(&fields[5])),
        }
    }
    pub fn prepare(&mut self) -> Task {
        self.drafts[self.query] = Some(self.fields.clone());
        self.defaults = None;
        self.setup = false;
        self.preview = None;
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
        self.configuring = false;
        self.scroll = 0;
    }
    pub fn completed(&mut self, report: tools::Report) -> bool {
        self.collection_result = false;
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
        if self.install_open {
            return Some(match self.installation.key(key, dry_run, &self.snapshots) {
                Some(super::tools_install::Outcome::Back) => {
                    self.install_open = false;
                    None
                }
                Some(super::tools_install::Outcome::FreshReview) => {
                    // Navigation/metadata discovery only. Never reuse historical
                    // install inputs as approvals or bind a new catalog here.
                    self.reconciliation.invalidate();
                    self.reconciliation
                        .stale_for_install
                        .extend(self.snapshots.iter().map(|s| s.id.clone()));
                    let task = self.configure();
                    self.fresh_review = true;
                    Some(task)
                }
                Some(super::tools_install::Outcome::Task(task)) => Some(*task),
                None => None,
            });
        }
        if self.declaration_open {
            if key.code == KeyCode::Esc && !self.declaration.details {
                self.declaration_open = false;
                return Some(None);
            }
            return Some(self.declaration.key(key));
        }
        if self.recon_open {
            let outcome = self.reconciliation.key(key, &self.snapshots);
            return Some(match outcome {
                Some(super::tools_reconcile::Outcome::FreshInventory) => Some(self.configure()),
                Some(super::tools_reconcile::Outcome::Install(tool)) => {
                    match self.reconciliation.result.as_ref().and_then(|r| {
                        crate::services::installation::Request::selected(r, &tool, &self.snapshots)
                            .ok()
                    }) {
                        Some(request) => {
                            self.install_open = true;
                            Some(self.installation.prepare(request))
                        }
                        None => {
                            self.notice = Some(
                                "Select an eligible missing package with explicit snapshot and prerequisite bindings.",
                            );
                            None
                        }
                    }
                }
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
        if self.setup && self.details {
            match key.code {
                KeyCode::Char('d') | KeyCode::F(3) | KeyCode::Esc => self.details = false,
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
                KeyCode::Home => self.scroll = 0,
                KeyCode::Char('q') => return None,
                _ => (),
            }
            return Some(None);
        }
        if self.setup {
            match key.code {
                KeyCode::Char('q')
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::F(10)
                | KeyCode::Char('?' | ';')
                | KeyCode::F(1) => return None,
                KeyCode::Char('i') => return Some(Some(self.open_attempts())),
                KeyCode::Char('a') => self.declare(),
                KeyCode::Char('s') => {
                    self.leave();
                    self.catalog_browser = true;
                }
                KeyCode::Char('p') => {
                    self.leave();
                    self.recon_open = true;
                }
                KeyCode::Esc => {
                    self.leave();
                    self.notice = None;
                }
                KeyCode::Tab | KeyCode::Down => {
                    self.default_focus = (self.default_focus + 1) % 4;
                    self.default_feedback = None;
                }
                KeyCode::BackTab | KeyCode::Up => {
                    self.default_focus = (self.default_focus + 3) % 4;
                    self.default_feedback = None;
                }
                KeyCode::Char(' ') => {
                    if let Some(choice) = self
                        .defaults
                        .as_mut()
                        .and_then(|d| d.choices.get_mut(self.default_focus))
                        && choice.preview.is_some()
                    {
                        choice.selected = !choice.selected;
                        self.default_feedback = None;
                    } else {
                        self.default_feedback = Some(
                            "Inventory unavailable. Advanced settings contain the missing trusted scope.",
                        );
                    }
                }
                KeyCode::Enter => {
                    return Some(self.approve_selected(dry_run));
                }
                KeyCode::Char('e') => self.select_advanced(),
                KeyCode::Char('r') => {
                    return Some(Some(self.configure()));
                }
                KeyCode::Char('d') | KeyCode::F(3) => self.details = !self.details,
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
                _ => (),
            }
            if self.default_focus < 4 {
                self.default_query = self.default_focus;
            }
            return Some(None);
        }
        if self.preview.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.preview = None;
                    self.configuring = true;
                }
                KeyCode::Home => self.scroll = 0,
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
                KeyCode::Enter if !dry_run => {
                    let preview = self.preview.take()?;
                    self.report = None;
                    return Some(Some(Task::ToolsInspect(preview.approve())));
                }
                _ => (),
            }
            return Some(None);
        }
        if self.configuring && self.config_details {
            match key.code {
                KeyCode::Esc | KeyCode::F(3) => self.config_details = false,
                KeyCode::Down | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
                KeyCode::Up | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
                KeyCode::Home => self.scroll = 0,
                _ => (),
            }
            return Some(None);
        }
        if self.configuring {
            if key.code == KeyCode::F(3) {
                self.config_details = true;
                self.scroll = 0;
                return Some(None);
            }
            let controls = self.controls();
            let pos = controls.iter().position(|i| *i == self.focus).unwrap_or(0);
            match key.code {
                KeyCode::Esc => {
                    self.drafts[self.query] = Some(self.fields.clone());
                    self.configuring = false;
                    self.setup = self.defaults.is_some();
                }
                KeyCode::Tab | KeyCode::Down => self.focus = controls[(pos + 1) % controls.len()],
                KeyCode::BackTab | KeyCode::Up => {
                    self.focus = controls[(pos + controls.len() - 1) % controls.len()]
                }
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.focus == 0 => {
                    self.default_query =
                        (self.query + if key.code == KeyCode::Left { 3 } else { 1 }) % 4;
                    self.default_focus = self.default_query;
                    self.select_advanced();
                    self.focus = 0;
                }
                KeyCode::Enter => {
                    if let Some(missing) = self
                        .fields
                        .iter()
                        .take(if self.query == 3 { 6 } else { 2 })
                        .position(|f| f.is_empty())
                    {
                        self.focus = missing + 1;
                    }
                    return Some(Some(self.prepare()));
                }
                _ if (1..=6).contains(&self.focus) => {
                    edit_text(&mut self.fields[self.focus - 1], key)
                }
                _ => (),
            }
            return Some(None);
        }
        if key.code == KeyCode::F(3) && self.report.is_some() {
            self.details = !self.details;
            self.searching = false;
            self.scroll = 0;
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
            KeyCode::Char('i') => return Some(Some(self.open_attempts())),
            KeyCode::Char('a') => self.declare(),
            KeyCode::Char('t') if self.can_declare_selected() => self.declare_selected(),
            KeyCode::Char('s') => self.catalog_browser = true,
            KeyCode::Char('p') => {
                self.recon_open = true;
            }
            KeyCode::Char('c') => return Some(Some(self.configure())),
            KeyCode::Char('e') => self.advanced(),
            KeyCode::Char('r') => {
                return Some(Some(if self.collection_result {
                    self.configure()
                } else {
                    self.prepare()
                }));
            }
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Enter if self.report.is_none() => return Some(Some(self.configure())),
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
    pub fn open_attempts(&mut self) -> Task {
        self.leave();
        self.install_open = true;
        self.installation.open_attempts(&self.snapshots)
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, dry_run: bool, hits: &super::input::Hits) {
        hits.wheel(area, super::input::Wheel::Tools);
        if self.install_open {
            self.installation.draw(frame, area, dry_run, hits);
            return;
        }
        if self.declaration_open {
            self.declaration.draw(frame, area, hits);
            return;
        }
        if self.recon_open {
            self.reconciliation.draw(frame, area, &self.snapshots, hits);
            return;
        }
        if self.catalog_browser {
            if !self.snapshots.is_empty() {
                hits.present(ui::ActionButton {
                    label: "Forget inspection metadata",
                    key: "x",
                    target: Target::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
                });
                hits.present(ui::ActionButton {
                    label: "Select inspection",
                    key: "Enter",
                    target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                });
            }
            let mut lines = vec![Line::styled("Previous inspections", ui::heading())];
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
                lines.push(Line::raw("No inspections collected."));
            }
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        if let Some(notice) = self.notice {
            ui::chrome_prose(frame, area, notice, ui::warning());
            return;
        }
        if self.setup {
            self.draw_defaults(frame, area, dry_run, hits);
            return;
        }
        if self.configuring {
            if self.config_details {
                let text = serde_json::to_string_pretty(&serde_json::json!({"provider":format!("{:?}", QUERIES[self.query]), "program":self.fields[0], "search_directories":self.fields[1], "manifest":self.fields[2], "runtime_tool":self.fields[3], "npm_cli":self.fields[4], "prefix":self.fields[5]})).expect("draft metadata serializes");
                ui::report(
                    frame,
                    area,
                    "Draft scope (not an approved review)",
                    ui::review_text(&text),
                    self.scroll,
                    false,
                );
                hits.present(ui::ActionButton {
                    label: "Fields",
                    key: "Esc",
                    target: Target::Back,
                });
                return;
            }
            self.form(frame, area, hits);
            return;
        }
        if self.preview.is_some() {
            let warning = if dry_run {
                "Preview only: execution disabled. Config/plugins may execute code; caches/logs may change. Not a sandbox."
            } else {
                "Risk: config/plugins may execute code; caches/logs may change. Not a sandbox."
            };
            let warning_height = if dry_run { 3 } else { 2 };
            ui::prose(
                frame,
                Rect::new(area.x, area.y, area.width, warning_height),
                warning,
                ui::warning(),
            );
            let height = area.height.saturating_sub(warning_height);
            ui::report(
                frame,
                Rect::new(area.x, area.y + warning_height, area.width, height),
                "Exact provider query · no installation",
                ui::review_text(&self.scope),
                self.scroll,
                false,
            );
            if !dry_run {
                hits.present(ui::ActionButton {
                    label: "Run query",
                    key: "Enter",
                    target: Target::ProviderConfirm,
                });
            }
            return;
        }
        let Some(report) = &self.report else {
            hits.present(ui::ActionButton {
                label: "Find inspection sources",
                key: "Enter",
                target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            });
            for (label, key, action) in [
                (
                    "Inspect installed tools",
                    "c",
                    super::shell::Action::ToolsConfigure,
                ),
                (
                    "Add declaration",
                    "a",
                    super::shell::Action::ToolDeclaration,
                ),
                (
                    "Compare declared tools",
                    "p",
                    super::shell::Action::ToolsReconcile,
                ),
                (
                    "Previous inspections",
                    "s",
                    super::shell::Action::ToolsSnapshots,
                ),
                (
                    "Installation attempts",
                    "i",
                    super::shell::Action::ToolAttempts,
                ),
            ] {
                hits.present(ui::ActionButton {
                    label,
                    key,
                    target: Target::Action(action),
                });
            }
            if self.next_id > 1 {
                ui::prose(
                    frame,
                    area,
                    "No inspections retained. Forgetting metadata does not undo earlier provider side effects.",
                    ui::muted(),
                );
                return;
            }
            ui::prose(
                frame,
                area,
                "No inspection collected yet. Review installed package inventories, declare a desired tool, or compare declarations with explicitly selected inspections. Nothing runs until you approve its review.",
                ui::muted(),
            );
            return;
        };
        hits.present(ui::ActionButton {
            label: if self.details {
                "Packages"
            } else {
                "Inspection details"
            },
            key: "F3",
            target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
        });
        if self.details {
            hits.present(ui::ActionButton {
                label: "Back",
                key: "Enter",
                target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            });
            let mut lines = vec![];
            if let Observation::Complete { installations } = &report.provider.observation
                && let Some(selected) = self
                    .matching
                    .get(self.selected)
                    .and_then(|i| installations.get(*i))
            {
                lines.push(Line::styled(
                    "Selected provider claim (not binary ownership)",
                    ui::heading(),
                ));
                lines.push(ui::metadata("Package", &selected.package));
                lines.push(ui::metadata("Version", &selected.version));
                lines.push(Line::default());
            }
            lines.extend(ui::review_text(&self.scope).lines);
            ui::report(
                frame,
                area,
                "Inspection scope",
                lines.into(),
                self.scroll,
                false,
            );
            return;
        }
        hits.present(ui::ActionButton {
            label: if self.searching { "Done" } else { "Details" },
            key: "Enter",
            target: Target::InventoryKey(KeyCode::Enter),
        });
        let provider = match report.provider.description.query {
            Query::BrewFormulae => "Homebrew",
            Query::DpkgPackages => "APT",
            Query::MiseInstalled => "mise",
            Query::NpmGlobals => "npm",
        };
        let snapshot = self
            .snapshots
            .get(self.current)
            .map(|s| ui::inline(&s.id))
            .unwrap_or_else(|| "unknown".into());
        let count = match &report.provider.observation {
            Observation::Complete { installations } => {
                format!("{}/{} reported", self.matching.len(), installations.len())
            }
            Observation::Unavailable { .. } => "Unavailable".into(),
        };
        ui::chrome_prose(
            frame,
            Rect::new(area.x, area.y, area.width, 1),
            &format!(
                "{provider} {} Snapshot {snapshot} {} {count}",
                ui::glyphs::SEPARATOR,
                ui::glyphs::SEPARATOR
            ),
            ui::design::Tone::Information.style(),
        );
        if area.height < 3 {
            return;
        }
        let filter = ui::design::filter(
            frame,
            Rect::new(area.x, area.y + 1, area.width, 1),
            "Filter packages",
            &self.search,
            self.searching && hits.input_focus_allowed(),
        );
        hits.click(filter, Target::InventoryKey(KeyCode::Char('/')));
        let rows = Rect::new(
            area.x,
            area.y + 2,
            area.width,
            area.height.saturating_sub(2),
        );
        match &report.provider.observation {
            Observation::Unavailable { reason } => ui::design::notice(
                frame,
                rows,
                "Inspection unavailable",
                &format!("{reason}\nNot a healthy empty inventory."),
                ui::design::Tone::Attention,
            ),
            Observation::Complete { installations } => {
                if installations.is_empty() {
                    ui::design::notice(
                        frame,
                        rows,
                        "0 packages reported",
                        "The provider reported an empty inventory in this scope. Not proof of binary absence or Dort ownership.",
                        ui::design::Tone::Neutral,
                    );
                    return;
                }
                if self.matching.is_empty() {
                    ui::design::notice(
                        frame,
                        rows,
                        "No matching packages",
                        "Change or clear the display filter. The inspection is retained.",
                        ui::design::Tone::Neutral,
                    );
                    return;
                }
                let capacity = ui::design::pair_capacity(rows).max(1);
                self.capacity.set(capacity);
                let offset = self.selected.saturating_sub(capacity - 1);
                let visible: Vec<_> = self
                    .matching
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(capacity)
                    .map(|(identity, index)| {
                        let entry = &installations[*index];
                        ui::design::PairRow {
                            identity,
                            values: [&entry.package, &entry.version],
                        }
                    })
                    .collect();
                ui::design::pairs(
                    frame,
                    rows,
                    ui::design::PairTable {
                        headers: ["PACKAGE", "VERSION"],
                        rows: &visible,
                        selected: self.selected,
                    },
                    hits,
                    Target::ToolsRow,
                );
            }
        }
    }
    pub fn browse_actions(&self) -> Vec<ui::ActionButton> {
        INVENTORY_ACTIONS
            .iter()
            .filter(|(_, code, _)| *code != KeyCode::Char('t') || self.can_declare_selected())
            .map(|(label, code, _)| ui::ActionButton {
                label,
                key: match code {
                    KeyCode::Char(_) if self.searching => "", // More owns this explicit action; printable keys still edit the filter.
                    KeyCode::Char('d') => "d",
                    KeyCode::Char('r') => "r",
                    KeyCode::Char('e') => "e",
                    KeyCode::Char('/') => "/",
                    KeyCode::Char('p') => "p",
                    KeyCode::Char('a') => "a",
                    KeyCode::Char('t') => "t",
                    KeyCode::Char('s') => "s",
                    _ => "i",
                },
                target: Target::InventoryKey(*code),
            })
            .collect()
    }
    fn form(&self, frame: &mut Frame, area: Rect, hits: &super::input::Hits) {
        let provider = ["Homebrew", "APT", "mise", "npm"][self.query].to_owned();
        let mut fields = vec![(LABELS[0], provider.as_str())];
        for (i, label) in LABELS
            .iter()
            .enumerate()
            .take(if self.query == 3 { 7 } else { 3 })
            .skip(1)
        {
            fields.push((*label, self.fields[i - 1].as_str()));
        }
        ui::field_page(
            frame,
            area,
            &fields,
            self.focus,
            hits,
            |i| Target::ToolsControl(i, false),
            ui::FormActions {
                choices: &[0],
                buttons: &[
                    ui::ActionButton {
                        label: "Review exact query",
                        key: "Enter",
                        target: Target::ToolsControl(7, true),
                    },
                    ui::ActionButton {
                        label: "Draft metadata",
                        key: "F3",
                        target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
                    },
                ],
                feedback: None,
            },
        );
    }
}
