//! The default inspection screen: metadata suggestions, selection, one approval.
use super::{components as ui, input::Target, tools::State};
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

impl State {
    pub(super) fn draw_defaults(
        &self,
        frame: &mut Frame,
        area: Rect,
        dry_run: bool,
        hits: &super::input::Hits,
    ) {
        let Some(defaults) = &self.defaults else {
            ui::prose(
                frame,
                area,
                "Finding installed package managers… No commands are running.",
                ui::muted(),
            );
            return;
        };
        if self.details {
            let scopes: Vec<_> = defaults
                .choices
                .iter()
                .filter_map(|c| c.preview.as_ref())
                .map(|p| p.review())
                .collect();
            let scope =
                serde_json::to_string_pretty(&scopes).expect("inspection metadata serializes");
            ui::report(
                frame,
                area,
                "Exact inspection scopes · d back · PgUp/PgDn scroll",
                ui::review_text(&scope),
                self.scroll,
                false,
            );
            return;
        }
        let mut lines = vec![Line::styled(
            "Inspect installed tools · inventory, no installs",
            ui::heading(),
        )];
        for (i, choice) in defaults.choices.iter().enumerate() {
            let scope = choice.request.as_ref().map(|r| {
                if let Some(prefix) = &r.prefix {
                    format!("prefix {}", prefix.display())
                } else {
                    r.executable.display().to_string()
                }
            });
            let text = if choice.preview.is_some() {
                format!(
                    "[{}] {} · {}",
                    if choice.selected { "x" } else { " " },
                    choice.label,
                    scope.unwrap_or_default()
                )
            } else {
                format!("[-] {} · {}", choice.label, choice.reason)
            };
            lines.push(Line::styled(
                ui::clip(&super::views::safe(&text), area.width),
                if self.default_focus == i {
                    ui::selected()
                } else {
                    ui::muted()
                },
            ));
            hits.click(
                Rect::new(area.x, area.y + i as u16 + 1, area.width, 1),
                Target::ToolsDefault(i, true),
            );
        }
        let selected = defaults
            .choices
            .iter()
            .filter(|c| c.selected && c.preview.is_some())
            .count();
        lines.push(Line::raw("Config/plugins may run code; caches may change."));
        let label = if dry_run {
            "Preview only · disabled"
        } else if selected == 0 {
            "No inventories selected"
        } else {
            "Inspect installed tools"
        };
        let mut buttons = vec![
            ui::button(
                label,
                "",
                self.default_focus == 4 && selected > 0 && !dry_run,
            ),
            ui::button("Advanced", "e", self.default_focus == 5),
        ];
        if area.width >= 80 {
            buttons.push(ui::button("Refresh", "r", self.default_focus == 6));
        }
        let widths: Vec<_> = buttons.iter().map(|s| s.width() as u16).collect();
        lines.push(Line::from(buttons));
        // Keep the primary action visible on compact terminals; long metadata is
        // available through d rather than silently losing path identity.
        let footer = area.bottom().saturating_sub(1);
        frame.render_widget(
            Paragraph::new(lines.iter().take(6).cloned().collect::<Vec<_>>()),
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
        );
        let primary = Rect::new(area.x, footer, area.width.min(widths[0]), 1);
        frame.render_widget(
            Paragraph::new(lines.pop().unwrap()),
            Rect::new(area.x, footer, area.width, 1),
        );
        if selected > 0 && !dry_run {
            hits.click(primary, Target::ToolsDefault(4, true));
        }
        let mut x = area.x + widths[0];
        for (i, width) in widths.iter().enumerate().skip(1) {
            if x < area.right() {
                hits.click(
                    Rect::new(x, footer, (*width).min(area.right() - x), 1),
                    Target::ToolsDefault(4 + i, true),
                );
            }
            x += width;
        }
    }
}
