//! Whole-app shell. Shared components own appearance; App retains real approval ownership.
use super::{
    app::{App, Tab},
    components as ui,
    input::{Target, Wheel},
    shell::{Action, Overlay},
    views,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect, text::Text, widgets::Paragraph};

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
fn short(action: Action) -> &'static str {
    match action {
        Action::Init => "Create/attach",
        Action::Choose => "Choose repository",
        Action::Discover => "Discover files",
        Action::Apply => "Review apply",
        Action::Inspect => "Details",
        Action::BatchAdopt => "Adopt marked",
        Action::ApplyMarked => "Apply marked",
        Action::KeepMarked => "Keep marked",
        Action::RemoveMarked => "Delete marked",
        Action::IgnoreSelected => "Ignore selected",
        Action::ManageIgnored => "Ignored paths",
        Action::AdoptSelected => "Adopt selected",
        Action::Adopt => "Manual adoption",
        Action::Keep => "Keep local file",
        Action::Remove => "Delete link",
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
pub fn wide_details(area: Rect, header_rows: u16) -> bool {
    area.width >= 110 && area.height.saturating_sub(header_rows) >= 14
}
/// Foreground-only metadata; no contents, serialized plans or approvals enter this model.
pub fn actions(app: &App) -> Vec<ui::ActionButton> {
    if app.help.is_some() {
        return vec![ui::ActionButton {
            label: "Back",
            key: "Esc",
            target: Target::Back,
        }];
    }
    if let Some(choice) = &app.choice {
        let mut actions = vec![ui::ActionButton {
            label: if choice.details { "Choices" } else { "Back" },
            key: "Esc",
            target: Target::Back,
        }];
        if !choice.details {
            actions.insert(
                0,
                ui::ActionButton {
                    label: "Choose",
                    key: "Enter",
                    target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                },
            );
        }
        actions.push(ui::ActionButton {
            label: if choice.details { "Choices" } else { "Details" },
            key: "F3",
            target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
        });
        return actions;
    }
    if let Some(menu) = &app.more {
        let mut actions = vec![ui::ActionButton {
            label: "Back",
            key: "Esc",
            target: Target::Back,
        }];
        if menu.details {
            actions.push(ui::ActionButton {
                label: "Actions",
                key: "F3",
                target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            });
        } else {
            if menu
                .items
                .get(menu.selected)
                .is_some_and(|item| item.reason.is_none())
            {
                actions.insert(
                    0,
                    ui::ActionButton {
                        label: "Open",
                        key: "Enter",
                        target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                    },
                );
            }
            actions.push(ui::ActionButton {
                label: "Details",
                key: "F3",
                target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            });
        }
        return actions;
    }
    if app.navigation.menu.is_some() {
        return vec![
            ui::ActionButton {
                label: "Open",
                key: "Enter",
                target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            },
            ui::ActionButton {
                label: "Back",
                key: "Esc",
                target: Target::Back,
            },
        ];
    }
    let mut result = app.hits.actions();
    if app.confirmation.is_some() || app.preview.is_some() {
        return result;
    }
    if !app.busy
        && (app.page_title().is_some() || app.dialog_title().is_some())
        && !result.iter().any(|a| a.key == "Esc")
    {
        result.push(ui::ActionButton {
            label: "Back",
            key: "Esc",
            target: Target::Back,
        });
    }
    if app.shell.overlay.is_none()
        && app.dialog_title().is_none()
        && (app.page_title().is_none()
            || matches!(app.tab, Tab::Files | Tab::Recovery)
            || app.tab == Tab::Git && !app.git_view.editing_root)
        && !app.busy
        && app.preview.is_none()
        && app.error.is_none()
        && !app.show_result
    {
        for entry in super::context::entries(app) {
            if !result
                .iter()
                .any(|action| matches!(action.target, Target::Action(a) if a == entry.action))
            {
                result.push(ui::ActionButton {
                    label: short(entry.action),
                    key: shortcut(entry.action),
                    target: Target::Action(entry.action),
                });
            }
        }
    }
    if app.tab == Tab::Tools && app.tools_view.browsing_inventory() {
        for action in app.tools_view.browse_actions() {
            if !result.iter().any(|a| a.label == action.label) {
                result.push(action);
            }
        }
    }
    result
}
fn compact(label: &'static str) -> &'static str {
    if label.starts_with("Review") {
        "Review"
    } else if label.starts_with("Install") {
        "Install"
    } else if label.starts_with("Inspect") {
        "Inspect"
    } else if label.starts_with("Reconcile") || label.starts_with("Compare") {
        "Compare"
    } else if label.starts_with("Find inspection") {
        "Find sources"
    } else if label.starts_with("Load") {
        "Load"
    } else if label.starts_with("Continue") {
        "Continue"
    } else if label.starts_with("Check") {
        "Check"
    } else if label.starts_with("Apply") {
        "Apply"
    } else if label.starts_with("Read") {
        "Read"
    } else if label.starts_with("Refresh") {
        "Refresh"
    } else if label.starts_with("Start fresh") {
        "Fresh review"
    } else {
        label
    }
}
fn strip(frame: &mut Frame, area: Rect, app: &App) {
    use ui::design::StripAction;
    let available = actions(app);
    let enabled = |action: &&ui::ActionButton| match action.target {
        Target::Action(value) => app.action_reason(value).is_none(),
        _ => true,
    };
    let primary = available.iter().filter(enabled).find(|a| a.key == "Enter");
    let back = available.iter().find(|a| a.key == "Esc");
    let mut output = Vec::new();
    if let Some(action) = primary {
        output.push(StripAction {
            key: action.key,
            label: if area.width < 80 {
                compact(action.label)
            } else {
                action.label
            },
            target: action.target,
            primary: true,
        });
    }
    if app.busy
        && let Some(action) = available.iter().find(|a| a.key == "q")
    {
        output.push(StripAction {
            key: action.key,
            label: action.label,
            target: action.target,
            primary: false,
        });
    }
    if let Some(action) = back {
        output.push(StripAction {
            key: action.key,
            label: action.label,
            target: action.target,
            primary: false,
        });
    }
    if app.more.is_none()
        && app.choice.is_none()
        && app.help.is_none()
        && app.navigation.menu.is_none()
        && !app.busy
    {
        output.push(StripAction {
            key: "F2",
            label: "More",
            target: Target::More,
            primary: false,
        });
    } else if app.more.as_ref().is_some_and(|menu| !menu.details)
        || app.choice.as_ref().is_some_and(|choice| !choice.details)
    {
        output.push(StripAction {
            key: "F3",
            label: "Details",
            target: Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            primary: false,
        });
    }
    if area.width >= 100 && app.dialog_title().is_none() && !app.busy && output.len() < 3 {
        output.push(StripAction {
            key: "Ctrl-K",
            label: "Commands",
            target: Target::Commands,
            primary: false,
        });
    }
    if app.help.is_none() {
        output.push(StripAction {
            key: "F1",
            label: "Help",
            target: Target::Action(Action::Help),
            primary: false,
        });
    }
    let width = output
        .iter()
        .map(|a| a.key.len() + a.label.len() + 4)
        .sum::<usize>()
        .saturating_sub(1);
    if width > usize::from(area.width) && app.more.is_none() && back.is_some() && output.len() > 3 {
        output.retain(|a| a.key != "F2");
    }
    ui::design::action_strip(frame, area, &output, &app.hits);
}
fn title(app: &App) -> &'static str {
    if let Some(title) = app.page_title() {
        return title;
    }
    match app.tab {
        Tab::Overview => "Overview",
        Tab::Files if app.discovering => "Files / Discover",
        Tab::Files => "Files / Managed",
        Tab::Recovery => "Recovery / All repositories",
        Tab::Checks if app.showing_diagnostics => "Checks / Diagnostics",
        Tab::Checks => "Checks / Manifest",
        Tab::Git => super::navigation::location(app),
        Tab::Tools if app.tools_view.browsing_inventory() => "Tools / Installed packages",
        Tab::Tools => "Tools",
    }
}
fn document(text: String) -> Text<'static> {
    ui::review_text(&text)
}
fn preview(frame: &mut Frame, area: Rect, app: &App) {
    let Some(preview) = &app.preview else {
        return;
    };
    app.hits.wheel(area, Wheel::Preview);
    if let super::actions::Pending::Files(plan) = &preview.pending
        && !preview.full
    {
        super::review::draw(frame, area, plan, preview, &app.hits);
    } else if let super::actions::Pending::Batch(plan) = &preview.pending
        && !preview.full
    {
        super::review::draw_batch(frame, area, plan, preview, &app.hits);
    } else {
        frame.render_widget(
            Paragraph::new(ui::review_text(&preview.text))
                .scroll((preview.scroll, preview.horizontal)),
            area,
        );
    }
    if !preview.pending.blocked() && !app.dry_run {
        app.presented(
            if preview.pending.adoption() {
                "Adopt all"
            } else {
                "Continue"
            },
            "Enter",
            Target::Continue,
        );
    }
    app.presented("Back", "Esc", Target::Back);
    app.presented(
        "Full report",
        "F3",
        Target::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
    );
}
fn overlay(frame: &mut Frame, area: Rect, app: &App, overlay: &Overlay) {
    let inner = if matches!(overlay, Overlay::Form(_) | Overlay::Ignored { .. }) {
        area
    } else {
        super::surface::dialog(
            frame,
            app,
            app.dialog_title().unwrap_or("Session choice"),
            false,
        )
    };
    app.hits.wheel(inner, Wheel::Overlay);
    match overlay {
        Overlay::Ignored { paths, selected } => {
            super::ignored::draw(frame, inner, app, paths, *selected)
        }
        Overlay::Profiles(_) | Overlay::Repositories { .. } => {
            super::pickers::draw(frame, inner, app, overlay)
        }
        Overlay::Form(_) if app.shell.form_details => ui::report(
            frame,
            inner,
            "Draft details",
            document(views::overlay_text(app, overlay, inner.height, inner.width)),
            app.shell.scroll,
            true,
        ),
        Overlay::Form(form) => super::forms::draw(frame, inner, app, *form),
        Overlay::Actions {
            query,
            selected,
            searching,
            entries,
        } => super::palette::draw(frame, inner, app, entries, query, *selected, *searching),
        _ => ui::text_view(
            frame,
            inner,
            document(views::overlay_text(app, overlay, inner.height, inner.width)),
            app.shell.scroll,
        ),
    }
    if app.shell.form_details {
        app.presented(
            "Fields",
            "F3",
            Target::FormAction(super::forms::Command::Details),
        );
    }
    app.presented("Back", "Esc", Target::Back);
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
    let areas = ui::chrome::shell(
        frame,
        title(app),
        app.tab,
        app.dry_run,
        app.navigation_block().is_some(),
        &app.hits,
    );
    let mut body = areas.body;
    if app.write_warning && !app.busy {
        frame.render_widget(
            Paragraph::new(ui::design::badge(
                "Earlier write failed: Possible partial effects",
                ui::design::Tone::Error,
            )),
            Rect::new(body.x, body.y, body.width, 1),
        );
        body.y += 1;
        body.height = body.height.saturating_sub(1);
    }
    app.hits.wheel(body, Wheel::Details);
    let foreground = app.help.is_some()
        || app.more.is_some()
        || app.choice.is_some()
        || app.navigation.menu.is_some();
    if !foreground && !app.busy && app.error.is_none() {
        if let Some(notice) = app.interaction_notice {
            let height = (1 + notice.len().div_ceil(usize::from(body.width).max(1)) as u16)
                .min(3)
                .min(body.height);
            ui::design::notice(
                frame,
                Rect::new(body.x, body.y, body.width, height),
                "Action unavailable",
                notice,
                ui::design::Tone::Attention,
            );
            body.y += height;
            body.height = body.height.saturating_sub(height);
        } else if app.tab == Tab::Overview && !app.status.as_str().is_empty() && body.height > 0 {
            ui::chrome_prose(
                frame,
                Rect::new(body.x, body.y, body.width, 1),
                app.status.as_str(),
                ui::design::Tone::Information.style(),
            );
            body.y += 1;
            body.height = body.height.saturating_sub(1);
        }
    }
    if !foreground {
        if app.error.is_some() || app.busy || (app.show_result && app.preview.is_none()) {
            super::task_surfaces::draw(
                frame,
                body,
                Rect::new(body.x, body.bottom(), body.width, 0),
                app,
            );
        } else if app.confirmation.is_some() {
            let inner = super::surface::dialog(frame, app, "Confirm reviewed changes", true);
            app.hits.wheel(inner, Wheel::Confirmation);
            let ids = app.batch_removal_ids();
            let removal = app.preview.as_ref().and_then(|p| p.pending.removal_id());
            let prompt = if !ids.is_empty() {
                format!(
                    "Delete managed links for these exact IDs:\n{}\nRepository sources are retained.",
                    ids.iter()
                        .map(|id| views::safe(id))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            } else if let Some(id) = removal {
                format!(
                    "Delete managed link for ID: {}\nRepository source is retained.",
                    views::safe(id)
                )
            } else {
                "Proceed with the reviewed changes?".into()
            };
            ui::text_view(
                frame,
                inner,
                Text::from(prompt),
                app.confirmation.as_ref().unwrap().prompt_scroll,
            );
            if !app.dry_run
                && app
                    .preview
                    .as_ref()
                    .is_some_and(|preview| !preview.pending.blocked())
            {
                app.presented("Confirm", "Enter", Target::ConfirmAction);
            }
            app.presented("Cancel", "Esc", Target::CancelConfirmation);
        } else if app.tab == Tab::Git && app.git_view.disclosure_pending() {
            let inner = super::surface::dialog(frame, app, "View Git patch content", true);
            app.git_view.draw(frame, inner, app.busy, &app.hits);
        } else if app.preview.is_some() {
            preview(frame, body, app);
        } else if let Some(editor) = app.batch_editor.as_ref().filter(|e| e.open) {
            super::batch_ui::draw(frame, body, app, editor);
        } else if let Some(current) = &app.shell.overlay {
            overlay(frame, body, app, current);
        } else {
            match app.tab {
                Tab::Files => super::files::draw(frame, body, app),
                Tab::Git => app.git_view.draw(frame, body, app.busy, &app.hits),
                Tab::Tools => app.tools_view.draw(frame, body, app.dry_run, &app.hits),
                Tab::Overview => super::observations::overview(frame, body, app),
                Tab::Checks => super::observations::checks(frame, body, app),
                Tab::Recovery => super::recovery::draw(frame, body, app),
            }
        }
    }
    if let Some(help) = &app.help {
        super::help::draw(frame, app, help);
    } else if let Some(choice) = &app.choice {
        choice.draw(
            frame,
            &app.hits,
            app.interaction_notice.or(app
                .write_warning
                .then_some("Earlier write failed: Possible partial effects")),
        );
    } else if let Some(menu) = &app.more {
        ui::menu::draw(
            frame,
            menu.title,
            &menu.items,
            menu.selected,
            menu.details,
            menu.feedback.or(app.interaction_notice).or(app
                .write_warning
                .then_some("Earlier write failed: Possible partial effects")),
            &app.hits,
        );
    } else if let Some(menu) = app.navigation.menu {
        super::navigation::popup(frame, app, menu);
    }
    strip(frame, areas.footer, app);
}
