//! Whole-app shell. App owns approvals.
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
        Action::ToolsCheck => "C",
        Action::ToolsInstallMissing => "i",
        Action::ToolsConfigure => "s",
        Action::ToolDeclaration => "a",
        Action::Init => "n",
        Action::Choose => "p",
        Action::Discover => "f",
        Action::Apply => "a",
        Action::Inspect => "Enter",
        Action::BatchAdopt => "A",
        Action::ApplyMarked => "A",
        Action::KeepMarked => "U",
        Action::RemoveMarked => "D",
        Action::ToggleMark => "Space",
        Action::IgnoreSelected => "i",
        Action::AdoptSelected => "a",
        Action::Adopt => "m",
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
        _ => "",
    }
}
fn short(action: Action) -> &'static str {
    match action {
        Action::Init => "Create/attach",
        Action::Choose => "Choose repository",
        Action::Discover => "Discover files",
        Action::Apply => "Apply",
        Action::Inspect => "Details",
        Action::BatchAdopt => "Adopt marked",
        Action::ToggleMark => "Mark/unmark",
        Action::ApplyMarked => "Apply marked",
        Action::KeepMarked => "Unmanage marked",
        Action::RemoveMarked => "Delete marked",
        Action::IgnoreSelected => "Ignore",
        Action::ManageIgnored => "Ignored paths",
        Action::AdoptSelected => "Adopt",
        Action::Adopt => "Manual adopt",
        Action::Keep => "Unmanage",
        Action::Remove => "Delete link",
        Action::Rollback => "Rollback",
        Action::Resume => "Resume",
        Action::Abandon => "Abandon",
        Action::CheckCurrent => "Check repository",
        Action::CheckStandalone => "Standalone check",
        Action::Diagnostics => "Diagnostics",
        Action::Profiles => "Profiles",
        Action::GitStatus => "Status",
        Action::GitUnstaged => "Unstaged",
        Action::GitStaged => "Staged",
        _ => action.label(),
    }
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
    let mut result = app.hits.actions();
    if app.confirmation.is_some() || app.preview.is_some() {
        return result;
    }
    if !app.busy
        && (app.page_title().is_some() || app.dialog_title().is_some())
        && !(app.tab == Tab::Tools
            && app.tools_view.at_root()
            && app.error.is_none()
            && !app.show_result)
        && !result.iter().any(|a| a.key == "Esc")
    {
        result.push(ui::ActionButton {
            label: if app.form_page().is_some() && app.form_has_focus() {
                "Stop editing"
            } else {
                "Back"
            },
            key: "Esc",
            target: Target::Back,
        });
    }
    if app.form_page().is_some() && app.form_has_focus() {
        for action in &mut result {
            if action.key == "Esc" {
                action.label = "Stop editing";
            }
        }
    }
    if app.form_page().is_some() && !app.form_has_focus() {
        result.push(ui::ActionButton {
            label: "Edit fields",
            key: "Tab",
            target: Target::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        });
    }
    if app.shell.overlay.is_none()
        && app.batch_editor.as_ref().is_none_or(|editor| !editor.open)
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
    for action in &mut result {
        action.key = super::navigation::context_key(action.key);
    }
    result
}
fn compact(label: &'static str) -> &'static str {
    if matches!(
        label,
        "First tool details" | "Tool details" | "Installation details" | "Package details"
    ) {
        "Details"
    } else if label == "Run inspection" {
        "Inspect"
    } else if label == "Installation history" {
        "History"
    } else if matches!(label, "Edit first declaration" | "Edit declaration") {
        "Edit"
    } else if label == "Add declaration" {
        "Add"
    } else if label == "Remove declaration" {
        "Remove"
    } else if label == "Tool settings" {
        "Settings"
    } else if label.starts_with("Review") {
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
    } else if label.starts_with("Done") {
        "Done"
    } else if label == "Stop editing" {
        "Unfocus"
    } else if label.starts_with("Confirm") {
        "Confirm"
    } else if label.starts_with("Continue") {
        "Continue"
    } else if label == "Run checks" || label.starts_with("Check") {
        "Check"
    } else if label.starts_with("Apply") && label != "Apply marked" {
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
/// The same width-aware partition drives the strip and the More dialog.
pub(super) fn strip_actions(
    app: &App,
    width: u16,
) -> (Vec<ui::design::StripAction>, Vec<ui::ActionButton>) {
    use ui::design::StripAction;
    let footer_width = if app.dialog_title().is_some() {
        ui::design::dialog_inner_width(width)
    } else {
        width
    };
    let mut available = actions(app);
    if app.help.is_none() && !available.iter().any(|a| a.key == "F1") {
        available.push(ui::ActionButton {
            key: "F1",
            label: "Help",
            target: Target::Action(Action::Help),
        });
    }
    // Controllers supply relevance order; navigation keeps reserved trailing positions.
    available.sort_by_key(|a| {
        usize::from(
            !matches!(a.target, Target::Continue | Target::ConfirmAction) && a.key != "Enter",
        )
    });
    let mut anchors: Vec<_> = available
        .iter()
        .copied()
        .filter(|a| matches!(a.key, "Esc" | "F1"))
        .collect();
    anchors.sort_by_key(|a| usize::from(a.key == "F1"));
    available.retain(|a| !matches!(a.key, "Esc" | "F1"));
    let reserved: usize = anchors
        .iter()
        .map(|a| {
            let label = if width < 80 {
                compact(a.label)
            } else {
                a.label
            };
            ratatui::text::Line::raw(format!(" {} {} ", a.key, label)).width() + 1
        })
        .sum();
    let more_allowed =
        app.more.is_none() && app.choice.is_none() && app.help.is_none() && !app.busy;
    let mut shown: Vec<ui::ActionButton> = Vec::new();
    let mut hidden: Vec<ui::ActionButton> = Vec::new();
    let mut used = 0usize;
    let mut overflowed = false;
    for action in available {
        if shown.iter().chain(&hidden).any(|a| a.label == action.label) {
            continue;
        }
        let label = if width < 80 {
            compact(action.label)
        } else {
            action.label
        };
        let cells = ratatui::text::Line::raw(format!(" {} {} ", action.key, label)).width()
            + usize::from(!shown.is_empty());
        let enabled =
            !matches!(action.target, Target::Action(value) if app.action_reason(value).is_some());
        if !action.key.is_empty()
            && !(app.key_text_entry() && (action.key.chars().count() == 1 || action.key == "Space"))
            && enabled
            && !overflowed
            && shown.len() + anchors.len() < 8
            && !shown.iter().any(|a| a.key == action.key)
            && used + cells + reserved <= usize::from(footer_width)
        {
            used += cells;
            shown.push(action);
        } else if more_allowed && super::more::eligible(app, &action) {
            overflowed |= enabled && !action.key.is_empty();
            hidden.push(action);
        }
    }
    if !hidden.is_empty() {
        let more_width = " F2 More ".len();
        while !shown.is_empty()
            && (shown.len() + anchors.len() >= 8
                || used + reserved + 1 + more_width > usize::from(footer_width))
        {
            let action = shown.pop().expect("nonempty");
            let label = if width < 80 {
                compact(action.label)
            } else {
                action.label
            };
            used = used.saturating_sub(
                ratatui::text::Line::raw(format!(" {} {} ", action.key, label)).width()
                    + usize::from(!shown.is_empty()),
            );
            if super::more::eligible(app, &action) {
                hidden.insert(0, action);
            }
        }
    }
    let mut output: Vec<_> = shown
        .into_iter()
        .map(|a| StripAction {
            key: a.key,
            label: if width < 80 {
                compact(a.label)
            } else {
                a.label
            },
            target: a.target,
            primary: a.key == "Enter"
                || matches!(a.target, Target::Continue | Target::ConfirmAction),
        })
        .collect();
    if !hidden.is_empty() {
        output.push(StripAction {
            key: "F2",
            label: "More",
            target: Target::More,
            primary: false,
        });
    }
    output.extend(anchors.into_iter().map(|a| StripAction {
        key: a.key,
        label: if width < 80 {
            compact(a.label)
        } else {
            a.label
        },
        target: a.target,
        primary: false,
    }));
    (output, hidden)
}
fn strip(frame: &mut Frame, area: Rect, app: &App) {
    let (output, _) = strip_actions(app, frame.area().width);
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
        let text = ui::review_text(&preview.text);
        let end = text.height().saturating_sub(usize::from(area.height));
        let offset = preview.observe_report(app.hits.size(), end.min(u16::MAX as usize) as u16);
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Preview)
            .pannable(frame, area, text, (offset, preview.horizontal));
    }
    let approval = app.preview_approval(app.hits.size());
    if approval.allowed() {
        approval.present_key(
            &app.hits,
            if preview.pending.adoption() {
                "Adopt all"
            } else {
                "Confirm changes"
            },
        );
    } else if !app.dry_run && !preview.pending.blocked() {
        app.presented(
            "Read remaining effects",
            "End",
            Target::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        );
    }
    app.presented("Back", "Esc", Target::Back);
    app.presented(
        "Full report",
        "F3",
        Target::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
    );
}
fn overlay(frame: &mut Frame, inner: Rect, app: &App, overlay: &Overlay) {
    app.hits.wheel(inner, Wheel::Overlay);
    match overlay {
        Overlay::Ignored { paths, selected } => {
            super::ignored::draw(frame, inner, app, paths, *selected)
        }
        Overlay::Profiles(_) | Overlay::Repositories { .. } => {
            super::pickers::draw(frame, inner, app, overlay)
        }
        Overlay::Form(_) if app.shell.form_details => ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .report(
                frame,
                inner,
                "Draft details",
                document(views::overlay_text(app, overlay, inner.height, inner.width)),
                app.shell.scroll,
                true,
            ),
        Overlay::Form(form) => super::forms::draw(frame, inner, app, *form),
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
        .input_focus(app.dialog_title().is_none() && app.form_has_focus());
    if area.width < 48 || area.height < 14 {
        ui::prose(
            frame,
            area,
            "Dort: terminal too small\nResize to 48x14; approval disabled.\nq: exit (waits for worker)",
            ui::warning(),
        );
        if let Some(help) = &app.help {
            let dialog = ui::design::choice_dialog(frame, "Help", &app.hits);
            super::help::draw(frame, dialog.body, app, help);
            strip(frame, dialog.footer, app);
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
    let mut body = super::navigation::local(frame, areas.body, app);
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
    let foreground = app.help.is_some() || app.more.is_some() || app.choice.is_some();
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
    let (body, footer) = if let Some(title) = app.dialog_title() {
        let dialog = ui::design::choice_dialog(frame, title, &app.hits);
        (dialog.body, dialog.footer)
    } else {
        (body, areas.footer)
    };
    if !foreground {
        if app.error.is_some() || app.busy || (app.show_result && app.preview.is_none()) {
            super::task_surfaces::draw(
                frame,
                body,
                Rect::new(body.x, body.bottom(), body.width, 0),
                app,
            );
        } else if app.confirmation.is_some() {
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
            ui::reader::Reader::new(&app.hits)
                .with_wheel(Wheel::Confirmation)
                .wrapped(
                    frame,
                    body,
                    Text::from(prompt),
                    app.confirmation.as_ref().unwrap().prompt_scroll,
                );
            app.preview_approval(app.hits.size()).present(
                &app.hits,
                "Confirm",
                Target::ConfirmAction,
            );
            app.presented("Cancel", "Esc", Target::CancelConfirmation);
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
                Tab::Tools if app.tools_view.landing() => {
                    super::declared_tools::draw(frame, body, app)
                }
                Tab::Tools => app.tools_view.draw(frame, body, app.dry_run, &app.hits),
                Tab::Overview => super::observations::overview(frame, body, app),
                Tab::Checks => super::observations::checks(frame, body, app),
                Tab::Recovery => super::recovery::draw(frame, body, app),
            }
        }
    }
    if let Some(help) = &app.help {
        super::help::draw(frame, body, app, help);
    } else if let Some(choice) = &app.choice {
        choice.draw(
            frame,
            body,
            &app.hits,
            app.interaction_notice.or(app
                .write_warning
                .then_some("Earlier write failed: Possible partial effects")),
        );
    } else if let Some(menu) = &app.more {
        ui::menu::draw(
            frame,
            body,
            &menu.items,
            menu.selected,
            menu.details,
            menu.feedback.or(app.interaction_notice).or(app
                .write_warning
                .then_some("Earlier write failed: Possible partial effects")),
            &app.hits,
        );
    }
    strip(frame, footer, app);
}
