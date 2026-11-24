//! Read-only dashboards built from typed data.
use super::{app::App, components as ui, input::Target, shell::Action};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
};

use ui::dashboard::Metric;
fn metric(label: &'static str, value: impl ToString, style: Style) -> Metric {
    let value = value.to_string();
    let compact = match label {
        "Repository" | "Manifest" => value.clone(),
        "History blockers" | "Blockers" => format!("Blocked writes: {value}"),
        _ => format!("{label} {value}"),
    };
    Metric {
        label,
        value,
        compact,
        style,
    }
}
fn section(lines: &mut Vec<Line<'static>>, title: &'static str) {
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled(title, ui::heading())));
}
fn field(label: &str, value: impl ToString) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), ui::muted()),
        Span::raw(ui::inline(&value.to_string())),
    ])
}
fn note(text: &str, style: Style) -> Line<'static> {
    Line::from(Span::styled(ui::inline(text), style))
}
fn dashboard(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    title: &'static str,
    metrics: Vec<Metric>,
    lines: Vec<Line<'static>>,
) {
    ui::dashboard::draw(
        frame,
        area,
        title,
        &metrics,
        Text::from(lines),
        app.details_scroll,
    );
}
fn count(value: Option<usize>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Unknown".into())
}

pub fn overview(frame: &mut Frame, area: Rect, app: &App) {
    if !app.overview_details {
        let action = app.overview_primary();
        app.presented(
            if action == Action::Apply {
                "Review changes"
            } else {
                "Create or attach"
            },
            "Enter",
            Target::Action(action),
        );
    }
    app.presented(
        if app.overview_details {
            "Summary"
        } else {
            "Overview details"
        },
        "F3",
        Target::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
    );
    if app.overview_details {
        app.presented("Back", "Esc", Target::Back);
    }
    let Some(data) = &app.data else {
        app.presented("Choose repository", "p", Target::Action(Action::Choose));
        ui::reader::Reader::new(&app.hits).report(
            frame,
            area,
            "Repository details",
            Text::raw("Loading read-only data..."),
            0,
            true,
        );
        return;
    };
    let root = data.repository.as_ref().or(app.repository.as_ref());
    let observation = data.overview.as_ref();
    let files = observation.and_then(|o| o.files.as_ref());
    let blockers = data
        .history_error
        .is_none()
        .then(|| data.history.iter().filter(|d| d.blocks_writes()).count());
    let registration = match observation.and_then(|o| o.registered) {
        Some(true) => "Registered",
        Some(false) => "Unregistered",
        None if root.is_none() => "Not selected",
        None => "Unknown",
    };
    let metrics = vec![
        metric(
            "Repository",
            registration,
            if observation.and_then(|o| o.registered) == Some(true) {
                ui::design::Tone::Information.style()
            } else {
                ui::warning()
            },
        ),
        metric(
            "Files",
            count(files.map(|f| f.linked + f.changes + f.blocked)),
            if files.is_some() {
                ui::heading()
            } else {
                ui::warning()
            },
        ),
        metric(
            "History blockers",
            count(blockers),
            if blockers == Some(0) {
                ui::heading()
            } else {
                ui::warning()
            },
        ),
    ];
    let mut lines = vec![];
    if let Some(result) = &app.result {
        section(&mut lines, "Last result");
        lines.extend(
            result
                .lines()
                .map(|line| note(line, ui::design::Tone::Information.style())),
        );
    }
    if root.is_none() {
        app.presented("Choose repository", "p", Target::Action(Action::Choose));
        app.presented("Discover files", "f", Target::Action(Action::Discover));
        app.presented(
            "Standalone check",
            "v",
            Target::Action(Action::CheckStandalone),
        );
        section(&mut lines, "Welcome");
        lines.push(note(
            "Choose a repository or create a local one to manage files.",
            ui::muted(),
        ));
        lines.push(note(
            "Discovery and standalone checks need no repository.",
            ui::muted(),
        ));
        lines.push(note(
            "Recovery and Diagnostics work without a repository.",
            Style::default(),
        ));
    }
    section(&mut lines, "Repository and profiles");
    lines.push(field(
        "Repository",
        root.map(|p| format!("{p:?}"))
            .unwrap_or_else(|| "Not selected".into()),
    ));
    lines.push(field("Selected by", app.origin));
    lines.push(field("Registration", registration));
    if observation.and_then(|o| o.registered) == Some(false) {
        lines.push(note(
            "Attach this repository before writing.",
            ui::warning(),
        ));
    }
    lines.push(field(
        "Registered repositories",
        count(observation.and_then(|o| o.registry_count)),
    ));
    if let Some(error) = observation.and_then(|o| o.configuration_error.as_ref()) {
        lines.push(field("Config unavailable", error));
        lines.push(note(
            "Repository list unavailable. Counts and the active repository are unknown.",
            ui::warning(),
        ));
    }
    if files.is_some() {
        lines.push(field("Effective profiles", format!("{:?}", data.profiles)));
    } else {
        lines.push(field("Effective profiles", "Unknown (files unavailable)"));
    }
    section(&mut lines, "Active files");
    if let Some(files) = files {
        lines.push(field("Linked", files.linked));
        lines.push(field("Needs apply", files.changes));
        lines.push(field("Blocked mappings", files.blocked));
        lines.push(field("Plan blocks", files.plan_blocked));
        lines.push(note(
            "Files shows source and target. Apply covers every active file.",
            Style::default(),
        ));
    } else {
        lines.push(note("File counts unavailable.", ui::warning()));
    }
    if let Some(error) = &data.files_error {
        lines.extend(error.lines().map(|line| note(line, ui::warning())));
    }
    section(&mut lines, "History");
    lines.push(field(
        "Operations",
        count(data.history_error.is_none().then_some(data.history.len())),
    ));
    lines.push(field("Blocked writes", count(blockers)));
    if let Some(error) = &data.history_error {
        lines.push(field("History unavailable", error));
        lines.push(note(
            "Write status unknown. Recovery can still refresh.",
            ui::warning(),
        ));
    }
    section(&mut lines, "Host and limits");
    if let Some(observation) = observation {
        lines.push(field(
            "Host",
            format!(
                "{:?} / {:?}",
                observation.host.platform, observation.host.architecture
            ),
        ));
    }
    lines.push(note(
        "Templates, secrets, sync and Git writes need other tools.",
        ui::muted(),
    ));
    lines.push(note(
        "Writes need a fresh review. A failed write may leave partial effects.",
        ui::warning(),
    ));
    if !app.overview_details {
        let mut summary = Vec::new();
        if let Some(root) = root {
            summary.push(Line::styled(
                ui::short_field(
                    "Repository",
                    root.to_str().unwrap_or("Unavailable non-UTF-8 root"),
                    area.width,
                ),
                ui::heading(),
            ));
            summary.push(field("Profiles", data.profiles.join(", ")));
            if let Some(files) = files {
                summary.push(field(
                    "Files",
                    format!(
                        "{} linked / {} changes / {} blocked",
                        files.linked, files.changes, files.blocked
                    ),
                ));
            } else {
                summary.push(field("Files", "Unknown"));
            }
            if observation.and_then(|o| o.registered) == Some(false) {
                summary.push(note(
                    "Registration is required before writing.",
                    ui::warning(),
                ));
            }
        } else {
            summary.push(Line::styled("Welcome", ui::heading()));
            summary.push(note(
                "Choose a repository or create a local one to manage files.",
                ui::muted(),
            ));
            summary.push(note(
                "Discovery, standalone checks and Recovery are independently available.",
                ui::muted(),
            ));
        }
        if let Some(error) = observation.and_then(|o| o.configuration_error.as_ref()) {
            summary.push(field("Registry unavailable", error));
        }
        if let Some(error) = &data.history_error {
            summary.push(field("History unavailable", error));
        }
        if let Some(result) = &app.result {
            summary.push(Line::styled("Last result", ui::heading()));
            summary.extend(
                result
                    .lines()
                    .take(2)
                    .map(|line| note(line, ui::design::Tone::Information.style())),
            );
        }
        lines = summary;
    }
    dashboard(frame, area, app, "Repository details", metrics, lines);
}

pub fn checks(frame: &mut Frame, area: Rect, app: &App) {
    let action = app.checks_primary();
    app.presented(
        match action {
            Action::Diagnostics => "Refresh diagnostics",
            Action::Refresh => "Check again",
            Action::CheckCurrent => "Check repository",
            _ => "Choose manifest",
        },
        "Enter",
        Target::Action(action),
    );
    if app.showing_diagnostics {
        diagnostics(frame, area, app);
        return;
    }
    let mut lines = vec![];
    let metrics;
    if let Some((report, standalone)) = &app.check {
        metrics = vec![
            metric("Manifest", "Valid", ui::success()),
            metric("Files", report.selected_files, ui::heading()),
            metric("Tools", report.selected_tools, ui::heading()),
        ];
        section(&mut lines, "Manifest check");
        lines.push(note(
            "Declarations are valid. Files shows live state; check tools separately.",
            ui::warning(),
        ));
        lines.push(field(
            "Scope",
            if *standalone {
                "standalone manifest"
            } else {
                "current repository"
            },
        ));
        lines.push(field("Valid manifest", format!("{:?}", report.manifest)));
        lines.push(field("Format", report.schema));
        section(&mut lines, "Active profiles");
        lines.push(field(
            "Effective profiles",
            format!("{:?}", report.active_profiles),
        ));
        lines.push(field("Base", "always active"));
        lines.push(field("Defaults", format!("{:?}", report.choices.defaults)));
        lines.push(field("Platform", format!("{:?}", report.choices.platform)));
        lines.push(field("Local", format!("{:?}", report.choices.local)));
        // These are result-bound additions, not the current mutable session draft.
        let additions: Vec<_> = report
            .active_profiles
            .difference(&report.choices.mandatory)
            .collect();
        lines.push(field("Extra profiles", format!("{additions:?}")));
        section(&mut lines, "Selected");
        lines.push(field("Selected files", report.selected_files));
        lines.push(field("Selected tools", report.selected_tools));
        lines.push(field(
            "Tool dependency order",
            format!("{:?}", report.tool_dependency_order),
        ));
        section(&mut lines, "Limits");
        if report.unsupported_capabilities.is_empty() {
            lines.push(note(
                "These declarations are supported. Files shows live state.",
                Style::default(),
            ));
        }
        for capability in &report.unsupported_capabilities {
            lines.push(field("Unavailable", capability));
        }
        lines.push(note(
            "Check tools and prerequisites separately.",
            ui::muted(),
        ));
    } else {
        metrics = vec![
            metric("Manifest", "Not checked", ui::warning()),
            metric("Files", "Unknown", ui::muted()),
            metric("Tools", "Unknown", ui::muted()),
        ];
        section(&mut lines, "Check a manifest");
        app.presented(
            "Check repository",
            "c",
            Target::Action(Action::CheckCurrent),
        );
        app.presented(
            "Standalone check",
            "v",
            Target::Action(Action::CheckStandalone),
        );
        lines.push(note(
            "A standalone manifest needs no repository or Git.",
            Style::default(),
        ));
        lines.push(note(
            "Checks validate declarations. Files shows live state; check tools separately.",
            ui::warning(),
        ));
    }
    app.presented(
        "Recheck scope",
        "r",
        Target::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
    );
    dashboard(
        frame,
        area,
        app,
        match &app.check {
            Some((_, true)) => "Standalone manifest",
            Some((_, false)) => "Repository manifest",
            None => "Manifest check",
        },
        metrics,
        lines,
    );
}

fn diagnostics(frame: &mut Frame, area: Rect, app: &App) {
    let Some(report) = &app.diagnostics else {
        ui::reader::Reader::new(&app.hits).report(
            frame,
            area,
            "Diagnostics",
            Text::raw("Diagnostics not loaded."),
            0,
            true,
        );
        return;
    };
    let registry = report.registry.as_ref().ok();
    let history = report.history.as_ref().ok();
    let metrics = vec![
        metric(
            "Registry",
            count(registry.map(|r| r.registered_repositories)),
            if registry.is_some() {
                ui::heading()
            } else {
                ui::warning()
            },
        ),
        metric(
            "History",
            count(history.map(|h| h.0)),
            if history.is_some() {
                ui::heading()
            } else {
                ui::warning()
            },
        ),
        metric(
            "Blockers",
            count(history.map(|h| h.1)),
            if history.is_some_and(|h| h.1 == 0) {
                ui::heading()
            } else {
                ui::danger()
            },
        ),
    ];
    let mut lines = vec![note(
        &format!(
            "Diagnostics | Dort {} | read-only",
            env!("CARGO_PKG_VERSION")
        ),
        ui::heading(),
    )];
    lines.push(field(
        "Registered repositories",
        count(registry.map(|r| r.registered_repositories)),
    ));
    lines.push(field("Operations", count(history.map(|h| h.0))));
    lines.push(field("Blocked writes", count(history.map(|h| h.1))));
    if let Some(registry) = registry {
        lines.push(field(
            "Registered active ID",
            format!("{:?}", registry.active_repository),
        ));
    }
    if let Err(error) = &report.registry {
        lines.push(field("Config unavailable", error));
        lines.push(note(
            "Repository list unavailable. Counts and the active repository are unknown.",
            ui::warning(),
        ));
    }
    if let Err(error) = &report.history {
        lines.push(field("History unavailable", error));
        lines.push(note("Write status unknown.", ui::warning()));
    }
    section(&mut lines, "Host & paths");
    let host = &report.host;
    lines.push(field(
        "Platform",
        format!("{:?} / {:?}", host.platform, host.architecture),
    ));
    lines.push(field("HOME", format!("{:?}", host.home)));
    lines.push(field("Config", format!("{:?}", host.config_dir)));
    lines.push(field("State", format!("{:?}", host.state_dir)));
    lines.push(field(
        "Session file root",
        format!(
            "{:?}",
            app.data
                .as_ref()
                .and_then(|d| d.repository.as_ref())
                .or(app.repository.as_ref())
        ),
    ));
    lines.push(field("Selected by", app.origin));
    lines.push(field("Git executable", format!("{:?}", app.git)));
    lines.push(field(
        "Git session root",
        format!("{:?}", app.git_view.root),
    ));
    lines.push(field("Uses file repository", app.git_view.follows_files));
    section(&mut lines, "Limits");
    lines.push(note(
        "Files shows live state; check tools separately.",
        ui::warning(),
    ));
    lines.push(note(
        "Diagnostics shows host and path details.",
        Style::default(),
    ));
    lines.push(note(
        "Supported: local files, recovery, manifest checks and read-only Git.",
        Style::default(),
    ));
    lines.push(note(
        "Not supported: templates, secrets, cloning/sync, Git writes and other package managers.",
        ui::muted(),
    ));
    lines.push(note(
        "Git needs a trusted Git executable. Other workflows still work.",
        Style::default(),
    ));
    lines.push(note(
        "Fix configuration errors, then refresh. Recovery still works.",
        Style::default(),
    ));
    app.presented(
        "Refresh Diagnostics",
        "r",
        Target::Action(Action::Diagnostics),
    );
    dashboard(frame, area, app, "Host details", metrics, lines);
}
