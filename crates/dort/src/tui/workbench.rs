//! Workbench shell and shared task surfaces. Core approval ownership stays in App.
use super::{
    app::{App, Tab},
    components as ui,
    shell::{Action, Overlay},
    views::{self, safe},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

fn short(action: Action) -> &'static str {
    match action {
        Action::Init => "Create/attach",
        Action::Choose => "Choose repository",
        Action::Discover => "Discover files",
        Action::Apply => "Review apply",
        Action::Inspect => "Inspect",
        Action::BatchAdopt => "Adopt marked",
        Action::ApplyMarked => "Apply marked",
        Action::KeepMarked => "Keep marked",
        Action::RemoveMarked => "Delete marked",
        Action::ToggleMark => "Mark/group",
        Action::IgnoreSelected => "Ignore selected",
        Action::ManageIgnored => "Ignored paths",
        Action::AdoptSelected => "Adopt selected",
        Action::Adopt => "Manual adoption",
        Action::Keep => "Keep local file",
        Action::Rollback => "Review rollback",
        Action::Resume => "Review resume",
        Action::Abandon => "Review abandon",
        Action::CheckCurrent => "Check repository",
        Action::CheckStandalone => "Standalone check",
        Action::Diagnostics => "Refresh diagnostics",
        Action::Profiles => "Profiles",
        Action::GitStatus => "Status",
        Action::GitUnstaged => "Unstaged",
        Action::GitStaged => "Staged",
        Action::GitPatch => "View patch",
        _ => action.label(),
    }
}
pub fn shortcut(action: Action) -> &'static str {
    match action {
        Action::Init => "n",
        Action::Choose => "p",
        Action::Discover => "f",
        Action::Apply => "a",
        Action::Inspect => "Enter",
        Action::BatchAdopt => "E",
        Action::ApplyMarked => "A",
        Action::KeepMarked => "K",
        Action::RemoveMarked => "D",
        Action::ToggleMark => "Space",
        Action::IgnoreSelected => "I",
        Action::ManageIgnored => "U",
        Action::AdoptSelected => "e",
        Action::Adopt => "o",
        Action::Keep => "u",
        Action::Remove => "d",
        Action::Rollback => "b",
        Action::Resume => "s",
        Action::Abandon => "x",
        Action::CheckCurrent => "c",
        Action::CheckStandalone => "v",
        Action::Diagnostics => "r",
        Action::Profiles => "t",
        Action::GitStatus => "z",
        Action::GitUnstaged => "u",
        Action::GitStaged => "s",
        Action::GitPatch => "w",
        _ => "",
    }
}
fn buttons(frame: &mut Frame, area: Rect, app: &App) {
    let mut x = area.x;
    let actions = app.page_actions();
    for action in &actions {
        let action = *action;
        let button = ui::button(short(action), shortcut(action), false);
        let width = button.width() as u16;
        if x + width > area.right() {
            break;
        }
        let r = Rect::new(x, area.y, width, 1);
        app.hits.click(r, super::input::Target::Action(action));
        frame.render_widget(
            Paragraph::new(Line::from(button)).style(if app.action_reason(action).is_some() {
                ui::muted()
            } else {
                ui::accent()
            }),
            r,
        );
        x += width;
    }
}
fn tabs(frame: &mut Frame, area: Rect, app: &App) {
    super::navigation::bar(frame, area, app);
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
pub fn wide_details(area: Rect, header_rows: u16) -> bool {
    area.width >= 110 && area.height.saturating_sub(header_rows) >= 14
}
fn inventory(frame: &mut Frame, app: &App) {
    use super::{
        components::design::{self, StripAction},
        input::Target,
    };
    use ratatui::crossterm::event::KeyCode;
    let area = frame.area();
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    let places = " [Places] ";
    let places_width = places.len() as u16;
    let title = if area.width < 64 {
        "Dort / Tools / Packages"
    } else {
        "Dort / Tools / Installed packages"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(title, ui::heading()),
            Span::styled(if app.dry_run { " [DRY RUN]" } else { "" }, ui::warning()),
        ])),
        Rect::new(area.x, area.y, area.width.saturating_sub(places_width), 1),
    );
    let place = Rect::new(
        area.right().saturating_sub(places_width),
        area.y,
        places_width,
        1,
    );
    frame.render_widget(Paragraph::new(places).style(ui::heading()), place);
    app.hits.click(place, Target::MenuOpen);
    let body = if app.write_warning {
        frame.render_widget(
            Paragraph::new(design::badge(
                "Earlier write failed: Possible partial effects",
                design::Tone::Error,
            )),
            Rect::new(sections[1].x, sections[1].y, sections[1].width, 1),
        );
        Rect::new(
            sections[1].x,
            sections[1].y + 1,
            sections[1].width,
            sections[1].height.saturating_sub(1),
        )
    } else {
        sections[1]
    };
    app.tools_view.draw(frame, body, app.dry_run, &app.hits);
    let notice = app.interaction_notice.or(app
        .write_warning
        .then_some("Earlier write failed: Possible partial effects"));
    app.tools_view.draw_inventory_more(frame, &app.hits, notice);
    let mut actions = Vec::new();
    if app.tools_view.inventory_more.is_some() || app.navigation.menu.is_some() {
        actions.push(StripAction {
            key: "Enter",
            label: "Open",
            target: Target::InventoryKey(KeyCode::Enter),
            primary: true,
        });
        actions.push(StripAction {
            key: "Esc",
            label: "Back",
            target: Target::Back,
            primary: false,
        });
    } else {
        actions.push(StripAction {
            key: "Enter",
            label: if app.tools_view.searching {
                "Done"
            } else {
                "Details"
            },
            target: Target::InventoryKey(KeyCode::Enter),
            primary: true,
        });
        if area.width >= 76 && !app.tools_view.searching {
            actions.push(StripAction {
                key: "r",
                label: "Inspect again",
                target: Target::InventoryKey(KeyCode::Char('r')),
                primary: false,
            });
        }
        actions.push(StripAction {
            key: "F2",
            label: "More",
            target: Target::InventoryKey(KeyCode::F(2)),
            primary: false,
        });
    }
    actions.push(StripAction {
        key: "F1",
        label: "Help",
        target: Target::Action(Action::Help),
        primary: false,
    });
    design::action_strip(frame, sections[2], &actions, &app.hits);
    if let Some(menu) = app.navigation.menu {
        super::navigation::popup(frame, app, menu);
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    app.hits.begin(area);
    app.hits
        .input_focus(app.dialog_title().is_none() && app.navigation.menu.is_none());
    if area.width < 48 || area.height < 14 {
        ui::prose(
            frame,
            area,
            "Dort: terminal too small\nResize to 48x14; approval disabled.\nq: exit (waits for worker)",
            ui::warning(),
        );
        if let Some(help) = &app.help {
            super::help::draw(frame, app, help);
        }
        return;
    }
    if app.tab == Tab::Tools
        && app.tools_view.browsing_inventory()
        && app.shell.overlay.is_none()
        && !app.busy
        && app.error.is_none()
        && !app.show_result
        && app.preview.is_none()
        && app.help.is_none()
        && !app.batch_editor.as_ref().is_some_and(|e| e.open)
    {
        inventory(frame, app);
        return;
    }
    let top = 2;
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
    } else if app.tab == Tab::Tools {
        "Explicit provider scope"
    } else {
        name
    };
    let header = Line::from(vec![
        Span::styled(" Dort  ", ui::heading()),
        Span::raw(ui::clip(context, area.width.saturating_sub(28))),
        Span::styled(if app.dry_run { "  [DRY RUN]" } else { "" }, ui::warning()),
        Span::styled(
            if app.menus_available() {
                format!("  {}", super::navigation::location(app))
            } else {
                String::new()
            },
            ui::selected(),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), sections[0]);
    tabs(frame, sections[1], app);
    let body = if let Some(title) = app.page_title() {
        super::surface::page(frame, sections[2], app, title)
    } else {
        sections[2]
    };
    let toolbar = app.preview.is_some()
        || app.error.is_some()
        || app.show_result
        || (!app.busy
            && app.error.is_none()
            && !app.show_result
            && app.shell.overlay.is_none()
            && !app.batch_editor.as_ref().is_some_and(|e| e.open)
            && !app.page_actions().is_empty());
    let content =
        Layout::vertical([Constraint::Min(3), Constraint::Length(u16::from(toolbar))]).split(body);
    app.hits.wheel(content[0], super::input::Wheel::Details);
    if !app.busy
        && !app.show_result
        && app.error.is_none()
        && app.preview.is_none()
        && app.shell.overlay.is_none()
        && !app.batch_editor.as_ref().is_some_and(|e| e.open)
        && app.help.is_none()
    {
        match app.tab {
            Tab::Files => super::files::draw(frame, content[0], app),
            Tab::Git if app.git_view.disclosure_pending() => (),
            Tab::Git => app.git_view.draw(frame, content[0], app.busy, &app.hits),
            Tab::Tools => app
                .tools_view
                .draw(frame, content[0], app.dry_run, &app.hits),
            Tab::Overview => super::observations::overview(frame, content[0], app),
            Tab::Checks => super::observations::checks(frame, content[0], app),
            Tab::Recovery => super::recovery::draw(frame, content[0], app),
        }
        buttons(frame, content[1], app);
    }
    if app.error.is_some() || app.busy || (app.show_result && app.preview.is_none()) {
        super::task_surfaces::draw(frame, content[0], content[1], app);
    }
    if app.error.is_some() {
        // Error controls above are exclusive; retained forms are not active underneath.
    } else if app.busy {
        // Task controls above are exclusive.
    } else if app.show_result && app.preview.is_none() {
        // A returned form stays retained but inactive until the result is dismissed.
    } else if app.help.is_some() {
        // Help owns the foreground. Withhold underlying forms and private
        // content, including their caret, rather than painting them underneath.
    } else if let Some(preview) = &app.preview {
        app.hits.wheel(content[0], super::input::Wheel::Preview);
        let continue_label = if preview.pending.adoption() {
            "Adopt all"
        } else {
            "Continue to confirmation"
        };
        let back_x = content[1].x
            + if preview.pending.blocked() || app.dry_run {
                " [Continue unavailable] ".len() as u16
            } else {
                ui::button(continue_label, "Enter", true).width() as u16
            };
        if !preview.pending.blocked() && !app.dry_run {
            app.hits.click(
                Rect::new(
                    content[1].x,
                    content[1].y,
                    (back_x - content[1].x).min(content[1].width),
                    1,
                ),
                super::input::Target::Continue,
            );
        }
        if back_x < content[1].right() {
            app.hits.click(
                Rect::new(
                    back_x,
                    content[1].y,
                    (content[1].right() - back_x)
                        .min(ui::button("Back", "Esc", false).width() as u16),
                    1,
                ),
                super::input::Target::Back,
            );
        }
        if let super::actions::Pending::Files(plan) = &preview.pending
            && !preview.full
        {
            let block = ui::pane("Review · Planned file changes", true);
            let inner = block.inner(content[0]);
            frame.render_widget(block, content[0]);
            super::review::draw(frame, inner, plan, preview, &app.hits);
        } else if let super::actions::Pending::Batch(plan) = &preview.pending
            && !preview.full
        {
            let block = ui::pane("Combined batch preview · Review exact marked set", true);
            let inner = block.inner(content[0]);
            frame.render_widget(block, content[0]);
            super::review::draw_batch(frame, inner, plan, preview, &app.hits);
        } else {
            frame.render_widget(
                Paragraph::new(ui::review_text(&preview.text))
                    .block(ui::pane("Review · Effects and safeguards", true))
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
                    ui::button(continue_label, "Enter", true)
                },
                ui::button("Back", "Esc", false),
            ])),
            content[1],
        );
    } else if let Some(editor) = app.batch_editor.as_ref().filter(|e| e.open) {
        super::batch_ui::draw(frame, body, app, editor);
    } else if let Some(overlay) = &app.shell.overlay {
        let modal = if matches!(overlay, Overlay::Form(_) | Overlay::Ignored { .. }) {
            body
        } else {
            super::surface::dialog(
                frame,
                app,
                app.dialog_title().unwrap_or("Session choice"),
                false,
            )
        };
        app.hits.wheel(modal, super::input::Wheel::Overlay);
        match overlay {
            Overlay::Ignored { paths, selected } => {
                super::ignored::draw(frame, modal, app, paths, *selected)
            }
            Overlay::Profiles(_) | Overlay::Repositories { .. } => {
                super::pickers::draw(frame, modal, app, overlay)
            }
            Overlay::Form(form) => {
                if app.shell.form_details {
                    ui::report(
                        frame,
                        modal,
                        "Draft details · F2/Esc fields",
                        legacy_document(views::overlay_text(
                            app,
                            overlay,
                            modal.height,
                            modal.width,
                        )),
                        app.shell.scroll,
                        true,
                    );
                } else if modal.height >= 18 {
                    let parts =
                        Layout::vertical([Constraint::Min(8), Constraint::Length(6)]).split(modal);
                    super::forms::draw(frame, parts[0], app, *form);
                    app.shell
                        .page_rows
                        .set(parts[1].height.saturating_sub(2).max(1));
                    ui::report(
                        frame,
                        parts[1],
                        "Draft details · F2 full details",
                        legacy_document(views::overlay_text(
                            app,
                            overlay,
                            modal.height,
                            modal.width,
                        )),
                        app.shell.scroll,
                        false,
                    );
                } else {
                    super::forms::draw(frame, modal, app, *form);
                }
            }
            Overlay::Actions {
                query,
                selected,
                searching,
                entries,
            } => super::palette::draw(frame, modal, app, entries, query, *selected, *searching),
            _ => ui::report(
                frame,
                modal,
                match overlay {
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
    let earlier_failure = app.write_warning && !app.busy && app.error.is_none();
    let status_style = if earlier_failure {
        ui::danger()
    } else if app.writing {
        ui::warning()
    } else {
        ui::muted()
    };
    let status = if earlier_failure {
        "Earlier write failed: Possible partial effects"
    } else {
        app.status.as_str()
    }; // Includes real metadata-only boundaries and deferred-exit feedback.
    frame.render_widget(
        Paragraph::new(ui::chrome_clip(status, sections[3].width)).style(status_style),
        sections[3],
    );
    let keys = super::hints::keys(app);
    frame.render_widget(Paragraph::new(ui::keys(&keys)), sections[4]);
    let mut x = sections[4].x;
    for (key, label) in &keys {
        let width = ui::keys(&[(key, label)]).width() as u16;
        let target = match *key {
            "F10" => Some(super::input::Target::MenuOpen),
            "F1" => Some(super::input::Target::Action(Action::Help)),
            ";" => Some(super::input::Target::PaletteSearch),
            _ => None,
        };
        if let Some(target) = target {
            app.hits.click(
                Rect::new(
                    x,
                    sections[4].y,
                    width.min(sections[4].right().saturating_sub(x)),
                    1,
                ),
                target,
            );
        }
        x += width + 2;
    }
    if let Some(menu) = app.navigation.menu {
        super::navigation::popup(frame, app, menu);
    }
    if let Some(confirmation) = &app.confirmation {
        let modal = super::surface::dialog(frame, app, "Confirm reviewed changes", true);
        app.hits.wheel(modal, super::input::Wheel::Confirmation);
        let removal = app.preview.as_ref().and_then(|p| p.pending.removal_id());
        let batch_ids = app.batch_removal_ids();
        let prompt = if !batch_ids.is_empty() {
            format!(
                "Delete managed links for these exact IDs:\n{}\nRepository sources are retained.",
                batch_ids
                    .iter()
                    .map(|id| safe(id))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else if let Some(id) = removal {
            format!(
                "Delete managed link for ID: {}\nRepository source is retained.",
                safe(id)
            )
        } else {
            "Proceed with the reviewed changes?".into()
        };
        // Approval uses the real preview/IDs, never reconstructed display text.
        let inner = modal;
        let prompt_height = inner.height.saturating_sub(1);
        ui::text_view(
            frame,
            Rect::new(inner.x, inner.y, inner.width, prompt_height),
            Text::from(prompt).style(ui::warning()),
            confirmation.prompt_scroll,
        );
        let button = Rect::new(inner.x, inner.y + prompt_height, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(ui::button("Confirm · Enter", "", true))),
            button,
        );
        app.hits.click(button, super::input::Target::ConfirmAction);
    }
    if app.tab == Tab::Git
        && app.git_view.disclosure_pending()
        && app.confirmation.is_none()
        && app.help.is_none()
    {
        let modal = super::surface::dialog(frame, app, "View Git patch content", true);
        app.git_view.draw(frame, modal, app.busy, &app.hits);
    }
    if let Some(help) = &app.help {
        super::help::draw(frame, app, help);
    }
}
