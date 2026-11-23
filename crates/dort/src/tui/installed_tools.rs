//! One current installed-package inventory and an independently reviewed declaration draft.
use super::{actions::Task, components as ui, input::Target, shell::edit_text};
use crate::services::installed_tools as discovery;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::Line,
};
#[derive(Default)]
pub struct State {
    pub waiting: bool,
    pub preview: Option<discovery::Preview>,
    pub current: Option<discovery::Current>,
    pub selected: usize,
    viewport: super::navigation::ListViewport,
    pub details: bool,
    pub searching: bool,
    pub filter: String,
    pub error: Option<String>,
    scroll: u16,
}
impl State {
    pub fn clear_review(&mut self) {
        self.waiting = false;
        self.preview = None;
    }
    pub fn completed(&mut self, current: discovery::Current) {
        self.current = Some(current);
        self.viewport = Default::default();
        self.selected = 0;
        self.details = false;
        self.error = None;
        self.scroll = 0;
    }
    pub fn rows(&self) -> Vec<(usize, usize)> {
        self.current
            .as_ref()
            .map(|c| {
                c.rows()
                    .into_iter()
                    .filter(|row| {
                        c.package(*row).is_some_and(|(_, p)| {
                            p.package
                                .to_lowercase()
                                .contains(&self.filter.to_lowercase())
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn seed(&self) -> dort_core::Result<discovery::Seed> {
        let row = self.rows().get(self.selected).copied().ok_or_else(|| {
            dort_core::Error::Invalid("Select an installed package first.".into())
        })?;
        self.current.as_ref().unwrap().seed(row)
    }
    pub fn key(&mut self, key: KeyEvent, dry_run: bool) -> Option<Task> {
        if self.error.is_some() {
            if key.code == KeyCode::Esc {
                self.error = None;
            }
            return None;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.searching = false,
                _ => {
                    edit_text(&mut self.filter, key);
                    self.selected = 0;
                }
            }
            return None;
        }
        if self.preview.is_some() {
            match key.code {
                KeyCode::Esc => self.clear_review(),
                KeyCode::Enter
                    if !dry_run && self.preview.as_ref().is_some_and(|p| !p.is_empty()) =>
                {
                    self.current = None;
                    return self
                        .preview
                        .take()
                        .map(|p| Task::ToolsInstalledInspect(p.approve()));
                }
                _ => self.scroll(key.code),
            }
            return None;
        }
        if self.details {
            match key.code {
                KeyCode::Esc => self.close_details(),
                _ => self.scroll(key.code),
            }
            return None;
        }
        let n = self.rows().len();
        if n == 0 {
            self.scroll(key.code);
        }
        self.selected = self.selected.min(n.saturating_sub(1));
        match key.code {
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Enter if n > 0 => {
                self.details = true;
                self.scroll = 0;
            }
            KeyCode::Down | KeyCode::PageDown => {
                self.selected = (self.selected
                    + if key.code == KeyCode::Down {
                        1
                    } else {
                        self.viewport.page()
                    })
                .min(n.saturating_sub(1))
            }
            KeyCode::Up | KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(if key.code == KeyCode::Up {
                    1
                } else {
                    self.viewport.page()
                })
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = n.saturating_sub(1),
            _ => (),
        }
        None
    }
    pub(super) fn close_details(&mut self) {
        self.details = false;
        self.scroll = 0;
    }
    fn scroll(&mut self, code: KeyCode) {
        match code {
            KeyCode::Down | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(2),
            KeyCode::Up | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(2),
            KeyCode::Home => self.scroll = 0,
            _ => (),
        }
    }
    fn draw_package(&self, frame: &mut Frame, area: Rect, row: (usize, usize)) {
        let Some((scope, package)) = self.current.as_ref().and_then(|c| c.package(row)) else {
            return;
        };
        let mut lines = vec![
            ui::metadata("Package", &package.package),
            ui::metadata("Version", &package.version),
            ui::metadata("Package manager", discovery::label(scope.query)),
            ui::metadata("Reported by", &scope.executable.display().to_string()),
            Line::raw(
                "From the last check. Packages may change; a package does not prove its command works.",
            ),
        ];
        if let Some(prefix) = &scope.npm_global_prefix {
            lines.push(ui::metadata("Global prefix", &prefix.display().to_string()));
        }
        if let Some(npm) = &scope.npm_binding {
            lines.push(ui::metadata("Node", &npm.node.display().to_string()));
            lines.push(ui::metadata(
                "Global prefix",
                &npm.prefix.display().to_string(),
            ));
        }
        ui::report(
            frame,
            area,
            "Installed package",
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
        if let Some(preview) = &self.preview {
            let review = preview.review();
            let mut lines = vec![Line::raw("Check packages with these package managers:")];
            for query in &review.inspections {
                lines.push(ui::metadata(
                    "Package manager",
                    discovery::label(query.provider.query),
                ));
                lines.push(ui::metadata(
                    "Program",
                    &query.provider.executable.display().to_string(),
                ));
                if let Some(probe) = query.prefix_probe {
                    lines.push(ui::metadata(
                        "Global location query",
                        &probe.arguments.join(" "),
                    ));
                    lines.push(Line::raw("The list uses the prefix reported by this check. Node comes from the displayed PATH."));
                }
                lines.push(ui::metadata(
                    "Arguments",
                    &query.provider.arguments.join(" "),
                ));
                lines.push(ui::metadata(
                    "HOME",
                    &query.provider.home.display().to_string(),
                ));
                lines.push(ui::metadata(
                    "Command paths",
                    &query
                        .provider
                        .trusted_search_path
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(":"),
                ));
                if let Some(npm) = &query.provider.npm_binding {
                    lines.push(ui::metadata("npm CLI", &npm.npm_cli.display().to_string()));
                    lines.push(ui::metadata(
                        "Global prefix",
                        &npm.prefix.display().to_string(),
                    ));
                }
            }
            for issue in review.unavailable {
                lines.push(ui::metadata(
                    discovery::label(issue.provider),
                    &issue.reason,
                ));
            }
            let top = if preview.is_empty() {
                0
            } else {
                3.min(area.height)
            };
            if top > 0 {
                ui::chrome_prose(
                    frame,
                    Rect::new(area.x, area.y, area.width, top),
                    "Checks can run package-manager configuration and plugins, and change caches. No packages are installed.",
                    ui::warning(),
                );
            }
            ui::report(
                frame,
                Rect::new(
                    area.x,
                    area.y + top,
                    area.width,
                    area.height.saturating_sub(top),
                ),
                "Inspect installed tools",
                lines.into(),
                self.scroll,
                false,
            );
            if !preview.is_empty() && !dry_run {
                action("Check installed", "Enter", KeyCode::Enter);
            }
            action("Back", "Esc", KeyCode::Esc);
            return;
        }
        if let Some(error) = &self.error {
            ui::design::notice(
                frame,
                area,
                "Declaration unavailable",
                error,
                ui::design::Tone::Attention,
            );
            action("Back", "Esc", KeyCode::Esc);
            return;
        }
        let rows = self.rows();
        let selected = rows.get(self.selected).copied();
        if self.details {
            if let Some(row) = selected {
                self.draw_package(frame, area, row);
                action("Add declaration", "a", KeyCode::Char('a'));
            }
            action("Back", "Esc", KeyCode::Esc);
            return;
        }
        let status = u16::from(self.current.is_some()).min(area.height);
        if let Some(current) = &self.current {
            let unavailable = current.reports.iter().filter(|r| !r.available()).count();
            let count = current.rows().len();
            let note = format!(
                "Last check: {count} package{}.{}",
                if count == 1 { "" } else { "s" },
                if unavailable == 0 {
                    String::new()
                } else {
                    format!(" {unavailable} checks unavailable.")
                }
            );
            ui::chrome_prose(
                frame,
                Rect::new(area.x, area.y, area.width, status),
                &note,
                ui::muted(),
            );
        }
        let filter_height = area.height.saturating_sub(status).min(1);
        ui::design::filter(
            frame,
            Rect::new(area.x, area.y + status, area.width, filter_height),
            "Filter",
            &self.filter,
            self.searching,
        );
        let body = Rect::new(
            area.x,
            area.y + status + filter_height,
            area.width,
            area.height.saturating_sub(status + filter_height),
        );
        if rows.is_empty() {
            let mut lines = vec![Line::raw(if !self.filter.is_empty() {
                "No packages match this filter."
            } else if self.current.as_ref().is_some_and(|c| c.available()) {
                "The checked package managers reported no installed packages."
            } else {
                "Check unavailable. Review the reasons, then check again."
            })];
            if let Some(current) = &self.current {
                for report in &current.reports {
                    if let dort_core::tools::inspection::Observation::Unavailable { reason } =
                        &report.provider.observation
                    {
                        lines.push(ui::metadata(
                            discovery::label(report.provider.description.query),
                            &reason.to_string(),
                        ));
                    }
                }
                if current.reports.is_empty() {
                    for issue in &current.unavailable {
                        lines.push(ui::metadata(
                            discovery::label(issue.provider),
                            &issue.reason,
                        ));
                    }
                }
            }
            ui::report(
                frame,
                body,
                "Installed packages",
                lines.into(),
                self.scroll,
                false,
            );
        } else {
            let (list, details) = super::workbench::detail_panes(body);
            let capacity = ui::design::pair_capacity(list);
            let first = self
                .viewport
                .start(rows.len(), capacity, selected.map(|_| self.selected));
            let values: Vec<_> = rows
                .iter()
                .enumerate()
                .skip(first)
                .take(capacity)
                .filter_map(|(identity, row)| {
                    self.current.as_ref()?.package(*row).map(|(scope, p)| {
                        (
                            identity,
                            format!(
                                "{} ({})",
                                p.package,
                                scope
                                    .npm_binding
                                    .as_ref()
                                    .map(|b| format!("npm: {}", b.runtime_tool))
                                    .unwrap_or_else(|| discovery::label(scope.query).into())
                            ),
                            p.version.clone(),
                        )
                    })
                })
                .collect();
            let pairs = values
                .iter()
                .map(|(identity, name, version)| ui::design::PairRow {
                    identity: *identity,
                    values: [name, version],
                })
                .collect::<Vec<_>>();
            ui::design::pairs(
                frame,
                list,
                ui::design::PairTable {
                    headers: ["PACKAGE / MANAGER", "VERSION"],
                    rows: &pairs,
                    selected: self.selected,
                },
                hits,
                Target::ToolsRow,
            );
            if let (Some(area), Some(row)) = (details, selected) {
                self.draw_package(frame, area, row);
            }
            action("Package details", "Enter", KeyCode::Enter);
            action("Add declaration", "a", KeyCode::Char('a'));
        }
        action("Check installed", "R", KeyCode::Char('R'));
        action("Filter packages", "/", KeyCode::Char('/'));
        action("Back", "Esc", KeyCode::Esc);
    }
}
