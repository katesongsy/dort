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
        Action::ToolsStandaloneCheck => "m",
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
        Action::Configure(super::configuration::Kind::Repair) => "e",
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
            label: "Close Help",
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
                    label: if choice.edits_draft() {
                        "Apply to draft"
                    } else {
                        "Choose"
                    },
                    key: "Enter",
                    target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                },
            );
        }
        if !choice.details && choice.can_toggle() {
            actions.push(ui::ActionButton {
                label: "Toggle",
                key: "Space",
                target: Target::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            });
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
    if let Some(field) = app.focused_reference() {
        for action in &mut result {
            if matches!(
                action.target,
                Target::FormAction(super::forms::Command::Submit) | Target::ToolDeclarationReview
            ) {
                action.key = "";
            }
        }
        result.insert(
            0,
            ui::ActionButton {
                label: field.action(),
                key: "Enter",
                target: Target::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            },
        );
    }
    if app.preview.is_some() {
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
            label: if app
                .form_page()
                .is_some_and(|owner| owner != "generated file")
                && app.form_has_focus()
            {
                "Stop editing"
            } else {
                "Back"
            },
            key: "Esc",
            target: Target::Back,
        });
    }
    if app
        .form_page()
        .is_some_and(|owner| owner != "generated file")
        && app.form_has_focus()
    {
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
    } else if label == "Create or attach" {
        "Create/attach"
    } else if label == Action::Clone.label() {
        "Clone"
    } else if label == Action::EnvironmentPlan.label() {
        "Env plan"
    } else if label == Action::PreviewGenerated.label() {
        "Preview"
    } else if label == Action::NewGenerated.label() {
        "New"
    } else if label == Action::EditGenerated.label() {
        "Edit"
    } else if label == "Open in editor" {
        "Editor"
    } else if label == "Add file mapping" {
        "Add"
    } else if label == "Edit file mapping" {
        "Edit mapping"
    } else if label == Action::SaveProfileChoice.label() {
        "Local choice"
    } else if label == "Review deployment" {
        label
    } else if label == "Review resume" {
        "Resume review"
    } else if label == "Edit profile defaults" {
        "Defaults"
    } else if label == "Rename profile" {
        "Rename"
    } else if label == "Delete profile" {
        "Delete"
    } else if matches!(label, "Choose profiles" | "Choose tools" | "Choose runtime") {
        "Choose"
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
    available
        .sort_by_key(|a| usize::from(!matches!(a.target, Target::Continue) && a.key != "Enter"));
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
            primary: a.key == "Enter" || matches!(a.target, Target::Continue),
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
        Tab::Files => app.files_page.location(),
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
    ui::reader::Reader::new(&app.hits)
        .with_wheel(Wheel::Preview)
        .review(
            frame,
            area,
            None,
            &ui::review_text(&preview.text),
            ui::reader::ReviewStage::Changes,
            &preview.reader,
        );
    app.preview_approval()
        .present_key(&app.hits, preview.pending.confirmation_label());
    if frame.area().width >= 64
        && matches!(
            preview.pending,
            super::actions::Pending::GeneratedDeployment(_)
        )
    {
        app.presented(
            "Last line",
            "End",
            Target::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        );
    }
    if app.environment_can_check() {
        app.presented(
            "Check environment",
            "c",
            Target::Action(Action::EnvironmentCheck),
        );
    }
    if app.file_review_needs_environment() {
        app.presented(
            Action::EnvironmentPlan.label(),
            "w",
            Target::Action(Action::EnvironmentPlan),
        );
    }
    app.presented("Back", "Esc", Target::Back);
}
fn overlay(frame: &mut Frame, inner: Rect, app: &App, overlay: &Overlay) {
    app.hits.wheel(inner, Wheel::Overlay);
    match overlay {
        Overlay::Generated(report) => {
            report.draw(frame, inner, app);
            if report.can_review_deployment() {
                app.presented(
                    "Review deployment",
                    "r",
                    Target::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
                );
            }
            if frame.area().width >= 64 {
                app.presented(
                    "Last line",
                    "End",
                    Target::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
                );
            }
        }
        Overlay::Ignored { paths, selected } => {
            super::ignored::draw(frame, inner, app, paths, *selected)
        }
        Overlay::Profiles(_) | Overlay::Repositories { .. } => {
            super::pickers::draw(frame, inner, app, overlay)
        }
        Overlay::Form(form) if app.shell.form_details => {
            let draft = views::overlay_text(app, overlay, inner.height, inner.width);
            let details = match &app.shell.form_feedback {
                Some((owner, text)) if owner == form => format!("{text}\n\n{draft}"),
                _ => draft,
            };
            ui::reader::Reader::new(&app.hits)
                .with_wheel(Wheel::Overlay)
                .report(
                    frame,
                    inner,
                    "Draft details",
                    document(details),
                    app.shell.scroll,
                    true,
                );
        }
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
fn scope_header(app: &App, width: u16) -> Option<(String, Option<String>)> {
    let short_path = |path: &std::path::Path| {
        ui::inline(
            &path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy(),
        )
    };
    let profile_label = |names: Vec<&str>| {
        let names = if names.is_empty() {
            "Unknown".into()
        } else if width < 105 && names.len() > 2 {
            format!("{}, +{}", names[0], names.len() - 1)
        } else {
            names.join(", ")
        };
        format!("Profiles: {names}")
    };
    if app.dialog_title().is_some() || app.error.is_some() || app.busy || app.show_result {
        return None;
    }
    if let Some(review) = &app.preview {
        use super::actions::Pending;
        let (kind, path, profiles) = match &review.pending {
            Pending::Environment(plan) => {
                let (path, profiles) = plan.scope();
                ("Repo", path, profiles)
            }
            Pending::Files(plan) => (
                "Repo",
                plan.inputs.repository.as_path(),
                &plan.inputs.active_profiles,
            ),
            Pending::FullCheck(check) => {
                let (path, profiles) = check.scope();
                ("Manifest", path, profiles)
            }
            _ => return None,
        };
        return Some((
            format!("{kind}: {}", short_path(path)),
            Some(profile_label(profiles.iter().map(String::as_str).collect())),
        ));
    }
    if app.tab == Tab::Git && !app.git_view.follows_files {
        return app
            .git_view
            .root
            .as_deref()
            .map(|root| (format!("Git: {}", short_path(root)), None));
    }
    if app.tab == Tab::Checks
        && let Some((report, true)) = &app.check
    {
        return Some((format!("Manifest: {}", short_path(&report.manifest)), None));
    }
    if app.tab == Tab::Tools
        && let Some(result) = &app.tools_view.declared.standalone_result
    {
        return Some((
            format!(
                "Manifest: {}",
                short_path(&result.reconciliation.scope.manifest)
            ),
            None,
        ));
    }
    if app.tab == Tab::Recovery {
        return Some(("History: all repositories".into(), None));
    }
    if app.tab == Tab::Tools && app.tools_view.installed_open {
        return Some(("Installed packages: this host".into(), None));
    }
    if app.tab == Tab::Tools && app.tools_view.install_open {
        return Some(("Installs: this host".into(), None));
    }
    let data = app.data.as_ref().filter(|d| d.overview.is_some())?;
    let repository = data.repository.as_deref().map_or_else(
        || "No repository".into(),
        |root| format!("Repo: {}", short_path(root)),
    );
    let profiles = data.repository.as_ref().map(|_| {
        let mut names = data
            .profile_rows
            .iter()
            .filter(|row| row.on)
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>();
        if names.is_empty() {
            names.extend(data.profiles.iter().map(String::as_str));
        }
        profile_label(names)
    });
    Some((repository, profiles))
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
    let scope = scope_header(app, area.width);
    let areas = ui::chrome::shell(
        frame,
        title(app),
        app.tab,
        app.dry_run,
        app.navigation_block().is_some(),
        &app.hits,
        scope
            .as_ref()
            .map(|(repository, profiles)| ui::chrome::Scope {
                repository,
                profiles: profiles.as_deref(),
                switch: app.can_switch_scope() && app.tab != Tab::Recovery,
                switch_profiles: app.can_switch_scope() && app.registered_repository(),
            }),
    );
    let mut body = super::navigation::local(frame, areas.body, app);
    let foreground = app.help.is_some() || app.more.is_some() || app.choice.is_some();
    if !foreground && !app.busy {
        let bottom_line = |body: &mut Rect| {
            body.height = body.height.saturating_sub(1);
            Rect::new(body.x, body.bottom(), body.width, 1)
        };
        if app.write_warning && body.height > 0 {
            frame.render_widget(
                Paragraph::new(ui::design::badge(
                    "Earlier write failed: Possible partial effects",
                    ui::design::Tone::Error,
                )),
                bottom_line(&mut body),
            );
        }
        if app.error.is_none() && !app.show_result && body.height > 0 {
            if let Some(notice) = app.interaction_notice {
                let height = (1 + notice.len().div_ceil(usize::from(body.width).max(1)) as u16)
                    .min(3)
                    .min(body.height);
                body.height -= height;
                ui::design::notice(
                    frame,
                    Rect::new(body.x, body.bottom(), body.width, height),
                    "Action unavailable",
                    notice,
                    ui::design::Tone::Attention,
                );
            } else if !app.status.as_str().is_empty() && app.form_page().is_none() {
                ui::chrome_prose(
                    frame,
                    bottom_line(&mut body),
                    app.status.as_str(),
                    ui::design::Tone::Neutral.style(),
                );
            }
        }
    }
    app.hits.wheel(body, Wheel::Details);
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
                Tab::Overview if app.profiles.open => super::profiles::draw(frame, body, app),
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
