//! Workbench shell and shared task surfaces. Core approval ownership stays in App.
use super::{
    app::{App, Tab},
    components as ui,
    shell::{self, Action, Focus, Overlay},
    views::{self, safe},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Clear, List, ListItem, ListState, Paragraph},
};

fn short(action: Action) -> &'static str {
    match action {
        Action::Init => "Create/attach",
        Action::Choose => "Choose repository",
        Action::Discover => "Discover files",
        Action::Apply => "Review apply",
        Action::Inspect => "Inspect",
        Action::AdoptSelected => "Adopt selected",
        Action::Adopt => "Manual adoption",
        Action::Keep => "Keep local file",
        Action::Rollback => "Review rollback",
        Action::Resume => "Review resume",
        Action::Abandon => "Review abandon",
        Action::CheckCurrent => "Check repository",
        Action::CheckStandalone => "Standalone check",
        Action::Diagnostics => "Diagnostics",
        Action::Profiles => "Profiles",
        Action::GitStatus => "Status",
        Action::GitUnstaged => "Unstaged",
        Action::GitStaged => "Staged",
        Action::GitPatch => "View patch",
        _ => action.label(),
    }
}
fn buttons(frame: &mut Frame, area: Rect, app: &App) {
    let actions = app.page_actions();
    let selected = app.action_selected.min(actions.len().saturating_sub(1));
    let focused = app.shell.focus == Focus::Actions;
    let count = if area.width < 80 { 2 } else { 3 };
    let start = if focused {
        selected.saturating_sub(count - 1)
    } else {
        0
    };
    let mut spans = Vec::new();
    for (i, action) in actions.iter().enumerate().skip(start).take(count) {
        let disabled = app.action_reason(*action).is_some();
        let mut button = ui::button(short(*action), "", focused && selected == i);
        if disabled && !(focused && selected == i) {
            button.style = ui::muted();
        }
        spans.push(button);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let note = if focused {
        app.action_reason(actions[selected])
            .unwrap_or("Left/Right choose action · Enter activate · Space all actions")
    } else {
        "Tab focuses actions · Space opens all actions · Search never changes execution scope"
    };
    ui::prose(
        frame,
        Rect::new(area.x, area.y + 1, area.width, 1),
        note,
        ui::muted(),
    );
}
fn tabs(frame: &mut Frame, area: Rect, app: &App) {
    let items = [
        (Tab::Overview, "1 Overview"),
        (Tab::Files, "2 Files"),
        (Tab::Recovery, "3 Recovery"),
        (Tab::Checks, "4 Checks"),
        (Tab::Git, "5 Git"),
    ];
    let mut lines = vec![Line::default()];
    for (i, (tab, label)) in items.iter().enumerate() {
        if area.width < 80 && i == 3 {
            lines.push(Line::default());
        }
        lines.last_mut().unwrap().spans.push(Span::styled(
            format!(" {} ", label),
            if *tab == app.tab {
                ui::selected()
            } else {
                ui::muted()
            },
        ));
    }
    if app.shell.focus == Focus::Destinations {
        lines[0].spans.push(Span::styled(" · Focus", ui::accent()));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
fn palette(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    query: &str,
    selected: usize,
    searching: bool,
) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(area);
    ui::input(
        frame,
        sections[0],
        if searching {
            "Actions · Search by label"
        } else {
            "Actions · / search"
        },
        query,
        searching,
    );
    let actions = shell::actions(query);
    let rows: Vec<_> = actions
        .iter()
        .map(|a| {
            let reason = app.action_reason(*a);
            let group = match a {
                Action::Destination(_) => "Navigate",
                Action::Choose | Action::Init | Action::Profiles => "Repository",
                Action::About | Action::Help | Action::Quit => "Help",
                _ => "Workflow",
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{group:<11}"), ui::muted()),
                Span::styled(
                    safe(a.label()),
                    if reason.is_some() {
                        ui::muted()
                    } else {
                        ratatui::style::Style::default()
                    },
                ),
            ]))
        })
        .collect();
    let list = List::new(rows)
        .block(ui::pane("Actions · Enter choose", !searching))
        .highlight_style(ui::selected())
        .highlight_symbol("> ");
    let mut state = ListState::default().with_selected((!actions.is_empty()).then_some(selected));
    frame.render_stateful_widget(list, sections[1], &mut state);
    if let Some(action) = actions.get(selected) {
        ui::prose(
            frame,
            sections[2],
            app.action_reason(*action).unwrap_or(action.label()),
            if app.action_reason(*action).is_some() {
                ui::warning()
            } else {
                ui::muted()
            },
        );
    } else {
        ui::prose(
            frame,
            sections[2],
            "No matching actions. Ctrl-U clears search.",
            ui::muted(),
        );
    }
}
fn legacy_document(text: String) -> Text<'static> {
    // Transitional report styling only: never derive identity, eligibility or requests from prose.
    Text::from(
        safe(&text)
            .lines()
            .map(|line| {
                if let Some((label, value)) = line.split_once(':') {
                    Line::from(vec![
                        Span::styled(format!("{label}:"), ui::heading()),
                        Span::raw(value.to_owned()),
                    ])
                } else {
                    Line::raw(line.to_owned())
                }
            })
            .collect::<Vec<_>>(),
    )
}
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 48 || area.height < 14 {
        ui::prose(
            frame,
            area,
            "Dort: terminal too small\nResize to 48x14; approval disabled.\nq/Ctrl-C: exit (waits for worker)",
            ui::warning(),
        );
        return;
    }
    let top = if area.width < 80 { 3 } else { 2 };
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(top - 1),
        Constraint::Min(6),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let root = app
        .data
        .as_ref()
        .and_then(|d| d.repository.as_ref())
        .or(app.repository.as_ref());
    let name = root
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("No repository");
    let context = if app.tab == Tab::Recovery {
        "All repositories"
    } else if app.tab == Tab::Git {
        "Independent Git scope"
    } else {
        name
    };
    let header = Line::from(vec![
        Span::styled(" Dort  ", ui::heading()),
        Span::raw(ui::clip(context, area.width.saturating_sub(28))),
        Span::styled(if app.dry_run { "  [DRY RUN]" } else { "" }, ui::warning()),
        Span::styled("  [p Scope]", ui::accent()),
    ]);
    frame.render_widget(Paragraph::new(header), sections[0]);
    tabs(frame, sections[1], app);
    let body = sections[2];
    let content = Layout::vertical([Constraint::Min(3), Constraint::Length(2)]).split(body);
    if app.error.is_none() && app.preview.is_none() && app.shell.overlay.is_none() {
        match app.tab {
            Tab::Files => super::files::draw(frame, content[0], app),
            Tab::Git => app.git_view.draw(
                frame,
                content[0],
                app.shell.focus == Focus::Content,
                app.busy,
            ),
            Tab::Overview => super::observations::overview(frame, content[0], app),
            Tab::Checks => super::observations::checks(frame, content[0], app),
            Tab::Recovery => super::recovery::draw(frame, content[0], app),
        }
        buttons(frame, content[1], app);
    }
    if let Some(error) = &app.error {
        ui::report(
            frame,
            content[0],
            if app.failed_write {
                "Write failed · Possible partial effects"
            } else {
                "Read failed · Input retained"
            },
            legacy_document(error.clone()),
            app.details_scroll,
            true,
        );
        frame.render_widget(
            Paragraph::new(ui::keys(&[
                (
                    "r",
                    if app.failed_write {
                        "Refresh only"
                    } else {
                        "Retry read"
                    },
                ),
                ("h", "Recovery"),
                ("Esc", "Back"),
            ])),
            content[1],
        );
    } else if let Some(preview) = &app.preview {
        if let super::actions::Pending::Files(plan) = &preview.pending
            && !preview.full
        {
            let block = ui::pane("Preview (metadata only) · Review effects", true);
            let inner = block.inner(content[0]);
            frame.render_widget(block, content[0]);
            super::review::draw(frame, inner, plan, preview);
        } else {
            frame.render_widget(
                Paragraph::new(safe(&preview.text))
                    .block(ui::pane("Preview (metadata only) · Full report", true))
                    .scroll((preview.scroll, preview.horizontal)),
                content[0],
            );
        }
        let disabled = preview.pending.blocked() || app.dry_run;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                if disabled {
                    Span::styled(" [Continue unavailable] ", ui::warning())
                } else {
                    ui::button("Continue to confirmation", "Enter", true)
                },
                ui::button("Back", "Esc", false),
            ])),
            content[1],
        );
    } else if let Some(overlay) = &app.shell.overlay {
        // Bounded dialogs; do not stretch long prose across a very wide terminal.
        let width = body.width.min(88);
        let modal = Rect::new(
            body.x + (body.width - width) / 2,
            body.y,
            width,
            body.height,
        );
        frame.render_widget(Clear, modal);
        match overlay {
            Overlay::Form(form) if app.shell.scroll == 0 => {
                super::forms::draw(frame, modal, app, *form)
            }
            Overlay::Actions {
                query,
                selected,
                searching,
            } if app.shell.scroll == 0 => palette(frame, modal, app, query, *selected, *searching),
            _ => ui::report(
                frame,
                modal,
                match overlay {
                    Overlay::Help => "Help",
                    Overlay::About => "About",
                    Overlay::Profiles(_) => "Profiles",
                    Overlay::Repositories { .. } => "Repository selector",
                    _ => "Full draft details",
                },
                legacy_document(views::overlay_text(app, overlay, modal.height, modal.width)),
                app.shell.scroll,
                true,
            ),
        }
    }
    let status_style = if app.writing {
        ui::warning()
    } else {
        ui::muted()
    };
    let status = &app.status; // Includes real metadata-only boundaries and deferred-exit feedback.
    frame.render_widget(
        Paragraph::new(ui::clip(status, sections[3].width)).style(status_style),
        sections[3],
    );
    let keys = if app.confirmation.is_some() {
        vec![("Enter", "Submit"), ("Esc", "Back")]
    } else if app.preview.is_some() {
        vec![
            ("Enter", "Continue"),
            ("i", "Full report"),
            ("PgDn", "Effects"),
            ("Esc", "Back"),
        ]
    } else if app.text_entry() {
        vec![
            ("Enter", "Next"),
            ("Tab", "Field"),
            ("Esc", "Back"),
            ("Ctrl-C", "Quit"),
        ]
    } else {
        vec![
            ("Enter", "Activate"),
            ("Tab", "Focus"),
            ("Space", "Actions"),
            ("?", "Help"),
        ]
    };
    frame.render_widget(Paragraph::new(ui::keys(&keys)), sections[4]);
    if let Some(confirmation) = &app.confirmation {
        let width = area.width.saturating_sub(4).min(76);
        let height = area.height.saturating_sub(4).min(13);
        let modal = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, modal);
        let removal = app.preview.as_ref().and_then(|p| p.pending.removal_id());
        let prompt = if let Some(id) = removal.filter(|_| confirmation.removal_ack.is_none()) {
            format!(
                "Separate deletion acknowledgement\nType exact file ID: {}\nRepository source is retained.",
                safe(id)
            )
        } else {
            "Approve reviewed effects\nType literal yes, then Enter.\nExecution independently revalidates all guards.".into()
        };
        // Prompt scroll is independent of input; full IDs are never reconstructed from display.
        frame.render_widget(
            ui::pane("Confirm · No writes before submission", true),
            modal,
        );
        let inner = Rect::new(
            modal.x + 1,
            modal.y + 1,
            modal.width.saturating_sub(2),
            modal.height.saturating_sub(2),
        );
        let prompt_height = inner.height.saturating_sub(4);
        ui::text_view(
            frame,
            Rect::new(inner.x, inner.y, inner.width, prompt_height),
            Text::from(prompt).style(ui::warning()),
            confirmation.prompt_scroll,
        );
        ui::input(
            frame,
            Rect::new(inner.x, inner.y + prompt_height, inner.width, 3),
            "Confirmation",
            &confirmation.input,
            true,
        );
        frame.render_widget(
            Paragraph::new(ui::keys(&[
                ("Enter", "Submit"),
                ("PgDn", "Details"),
                ("Esc", "Cancel"),
            ])),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}
