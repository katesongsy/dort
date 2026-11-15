//! Metadata suggestions are selectable rows, not a traversal through action buttons.
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
            ui::chrome_prose(
                frame,
                area,
                "Finding installed package managers… No commands are running.",
                ui::muted(),
            );
            return;
        };
        hits.present(ui::ActionButton {
            label: if self.details {
                "Sources"
            } else {
                "Exact inspection scopes"
            },
            key: "F3",
            target: Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::F(3),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        });
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
                "Exact inspection scopes",
                ui::review_text(&scope),
                self.scroll,
                false,
            );
            return;
        }
        let selected = defaults
            .choices
            .iter()
            .filter(|c| c.selected && c.preview.is_some())
            .count();
        let mut buttons = vec![
            ui::ActionButton {
                label: "Advanced",
                key: "e",
                target: Target::ToolsDefault(5, true),
            },
            ui::ActionButton {
                label: "Refresh",
                key: "r",
                target: Target::ToolsDefault(6, true),
            },
        ];
        if !dry_run && selected > 0 {
            buttons.insert(
                0,
                ui::ActionButton {
                    label: "Inspect installed tools",
                    key: "Enter",
                    target: Target::ToolsDefault(4, true),
                },
            );
        }
        let content = ui::action_bar(
            frame,
            area,
            hits,
            ui::FormActions {
                choices: &[],
                buttons: &buttons,
                feedback: self.default_feedback.map(|text| ui::Feedback {
                    text,
                    tone: ui::design::Tone::Attention,
                }),
            },
        );
        let mut lines = vec![Line::styled(
            if self.fresh_review {
                "Fresh review · no replay or automatic binding"
            } else {
                "Inspect installed tools · inventory, no installs"
            },
            ui::heading(),
        )];
        for (i, choice) in defaults.choices.iter().enumerate() {
            let scope = choice.request.as_ref().map(|r| {
                if let Some(prefix) = &r.prefix {
                    format!("prefix {}", ui::inline(&prefix.display().to_string()))
                } else {
                    ui::inline(&r.executable.display().to_string())
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
                format!("[-] {} · {}", choice.label, ui::inline(choice.reason))
            };
            lines.push(Line::styled(
                ui::chrome_clip(&text, content.width),
                if self.default_focus == i {
                    ui::selected()
                } else {
                    ui::muted()
                },
            ));
            if content.y + i as u16 + 1 < content.bottom() {
                hits.click(
                    Rect::new(content.x, content.y + i as u16 + 1, content.width, 1),
                    Target::ToolsDefault(i, true),
                );
            }
        }
        lines.push(Line::raw("Config/plugins may run code; caches may change."));
        if self.fresh_review {
            lines.push(Line::raw(
                "Fresh inventories and explicit bindings are required.",
            ));
        }
        frame.render_widget(Paragraph::new(lines), content);
    }
}
