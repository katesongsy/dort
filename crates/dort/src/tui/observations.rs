//! Read-only dashboards built from typed data.
use super::{app::App, components as ui, input::Target, shell::Action};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Text},
};

use ui::dashboard::Metric;
use ui::design::Tone;
use ui::reader::{field, section};
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
fn start_section(lines: &mut Vec<Line<'static>>, title: &str) {
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(section(title));
}
fn fact(label: &str, value: impl ToString) -> Line<'static> {
    field(label, &value.to_string(), Tone::Neutral)
}
fn note(text: &str, tone: Tone) -> Line<'static> {
    Line::styled(ui::inline(text), tone.style())
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
    let mut lines = Vec::new();
    if root.is_none() {
        app.presented("Choose repository", "p", Target::Action(Action::Choose));
        app.presented("Discover files", "f", Target::Action(Action::Discover));
        app.presented(
            "Standalone check",
            "v",
            Target::Action(Action::CheckStandalone),
        );
        lines.push(section("Welcome"));
        lines.push(Line::raw(
            "Choose a repository or create a local one to manage files.",
        ));
        lines.push(Line::raw(
            "Discovery, standalone checks and Recovery need no repository.",
        ));
    } else if let Some(root) = root {
        let path = root.to_str().unwrap_or("Unavailable non-UTF-8 root");
        let path = if app.overview_details {
            path.to_owned()
        } else {
            ui::clip(path, area.width.saturating_sub(14))
        };
        lines.push(field("Repository", &path, Tone::Neutral));
        lines.push(field(
            "Registration",
            registration,
            if registration == "Registered" {
                Tone::Neutral
            } else {
                Tone::Attention
            },
        ));
        if observation.and_then(|o| o.registered) == Some(false) {
            lines.push(field(
                "Blocked",
                "Writing requires registration.",
                Tone::Error,
            ));
            lines.push(field("Next", "Attach this repository.", Tone::Neutral));
        }
    }
    if blockers.is_none_or(|n| n > 0) {
        lines.push(field(
            "Blocked writes",
            &count(blockers),
            if blockers.is_some() {
                Tone::Error
            } else {
                Tone::Attention
            },
        ));
        lines.push(field("Next", "Open Recovery.", Tone::Neutral));
    }
    if let Some(result) = &app.result {
        start_section(&mut lines, "Last result");
        let limit = if app.overview_details { usize::MAX } else { 2 };
        lines.extend(
            result
                .lines()
                .take(limit)
                .map(|line| note(line, Tone::Neutral)),
        );
    }
    if app.overview_details {
        start_section(&mut lines, "Active files");
        if let Some(files) = files {
            lines.push(fact("Linked", files.linked));
            lines.push(field(
                "Needs apply",
                &files.changes.to_string(),
                if files.changes > 0 {
                    Tone::Attention
                } else {
                    Tone::Neutral
                },
            ));
            lines.push(field(
                "Blocked mappings",
                &files.blocked.to_string(),
                if files.blocked > 0 {
                    Tone::Error
                } else {
                    Tone::Neutral
                },
            ));
            if files.plan_blocked {
                lines.push(field("Apply plan", "Blocked", Tone::Error));
            }
            lines.push(Line::raw(
                "Files shows source and target. Apply covers every active file.",
            ));
        } else {
            lines.push(note("File counts unavailable.", Tone::Attention));
        }
        lines.push(note(
            "Writes need a fresh review. A failed write may leave partial effects.",
            Tone::Attention,
        ));
        start_section(&mut lines, "Repository and history");
        lines.push(fact(
            "Effective profiles",
            if files.is_some() {
                data.profiles.join(", ")
            } else {
                "Unknown (files unavailable)".into()
            },
        ));
        lines.push(fact("Selected by", app.origin));
        lines.push(fact(
            "Registered repositories",
            count(observation.and_then(|o| o.registry_count)),
        ));
        lines.push(fact(
            "Operations",
            count(data.history_error.is_none().then_some(data.history.len())),
        ));
        if blockers == Some(0) {
            lines.push(fact("Blocked writes", 0));
        }
        lines.push(Line::raw(
            "Templates, secrets, sync and Git writes need other tools.",
        ));
        if let Some(observation) = observation {
            lines.push(fact(
                "Host",
                format!(
                    "{:?} / {:?}",
                    observation.host.platform, observation.host.architecture
                ),
            ));
            if let Some(error) = &observation.configuration_error {
                lines.push(field("Config unavailable", error, Tone::Attention));
            }
        }
        if let Some(error) = &data.history_error {
            lines.push(field("History unavailable", error, Tone::Attention));
        }
        if let Some(error) = &data.files_error {
            lines.extend(error.lines().map(|line| note(line, Tone::Attention)));
        }
    } else if root.is_some() {
        lines.push(fact("Profiles", data.profiles.join(", ")));
        lines.push(fact(
            "Files",
            files.map_or_else(
                || "Unknown".into(),
                |files| {
                    format!(
                        "{} linked / {} changes / {} blocked",
                        files.linked, files.changes, files.blocked
                    )
                },
            ),
        ));
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
        lines.push(section("Manifest check"));
        lines.push(field("Outcome", "Declarations are valid", Tone::Success));
        lines.push(fact(
            "Scope",
            if *standalone {
                "standalone manifest"
            } else {
                "current repository"
            },
        ));
        lines.push(fact("Selected files", report.selected_files));
        lines.push(fact("Selected tools", report.selected_tools));
        lines.push(Line::raw(
            "Files shows live state. Check tools and prerequisites separately.",
        ));
        for capability in &report.unsupported_capabilities {
            lines.push(field("Unavailable", capability, Tone::Attention));
        }
        start_section(&mut lines, "Manifest and profiles");
        lines.push(fact("Manifest", format!("{:?}", report.manifest)));
        lines.push(fact(
            "Effective profiles",
            report
                .active_profiles
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ));
        lines.push(fact("Base", "always active"));
        lines.push(fact("Defaults", format!("{:?}", report.choices.defaults)));
        lines.push(fact("Platform", format!("{:?}", report.choices.platform)));
        lines.push(fact("Local", format!("{:?}", report.choices.local)));
        let additions: Vec<_> = report
            .active_profiles
            .difference(&report.choices.mandatory)
            .collect();
        lines.push(fact("Extra profiles", format!("{additions:?}")));
        lines.push(fact(
            "Tool dependency order",
            format!("{:?}", report.tool_dependency_order),
        ));
        lines.push(fact("Format", report.schema));
    } else {
        metrics = vec![
            metric("Manifest", "Not checked", ui::warning()),
            metric("Files", "Unknown", ui::muted()),
            metric("Tools", "Unknown", ui::muted()),
        ];
        lines.push(section("Check a manifest"));
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
            Tone::Neutral,
        ));
        lines.push(note(
            "Checks validate declarations. Files shows live state; check tools separately.",
            Tone::Neutral,
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
    let mut lines = vec![section(&format!(
        "Diagnostics | Dort {} | read-only",
        env!("CARGO_PKG_VERSION")
    ))];
    lines.push(fact(
        "Registered repositories",
        count(registry.map(|r| r.registered_repositories)),
    ));
    lines.push(fact("Operations", count(history.map(|h| h.0))));
    lines.push(fact("Blocked writes", count(history.map(|h| h.1))));
    if let Some(registry) = registry {
        lines.push(fact(
            "Registered active ID",
            format!("{:?}", registry.active_repository),
        ));
    }
    if let Err(error) = &report.registry {
        lines.push(fact("Config unavailable", error));
        lines.push(note(
            "Repository list unavailable. Counts and the active repository are unknown.",
            Tone::Attention,
        ));
    }
    if let Err(error) = &report.history {
        lines.push(fact("History unavailable", error));
        lines.push(note("Write status unknown.", Tone::Attention));
    }
    start_section(&mut lines, "Host & paths");
    let host = &report.host;
    lines.push(fact(
        "Platform",
        format!("{:?} / {:?}", host.platform, host.architecture),
    ));
    lines.push(fact("HOME", format!("{:?}", host.home)));
    lines.push(fact("Config", format!("{:?}", host.config_dir)));
    lines.push(fact("State", format!("{:?}", host.state_dir)));
    lines.push(fact(
        "Session file root",
        format!(
            "{:?}",
            app.data
                .as_ref()
                .and_then(|d| d.repository.as_ref())
                .or(app.repository.as_ref())
        ),
    ));
    lines.push(fact("Selected by", app.origin));
    lines.push(fact("Git executable", format!("{:?}", app.git)));
    lines.push(fact("Git session root", format!("{:?}", app.git_view.root)));
    lines.push(fact("Uses file repository", app.git_view.follows_files));
    start_section(&mut lines, "Limits");
    lines.push(note(
        "Files shows live state; check tools separately.",
        Tone::Attention,
    ));
    lines.push(note(
        "Diagnostics shows host and path details.",
        Tone::Neutral,
    ));
    lines.push(note(
        "Supported: local files, recovery, manifest checks and read-only Git.",
        Tone::Neutral,
    ));
    lines.push(note(
        "Not supported: templates, secrets, cloning/sync, Git writes and other package managers.",
        Tone::Neutral,
    ));
    lines.push(note(
        "Git needs a trusted Git executable. Other workflows still work.",
        Tone::Neutral,
    ));
    lines.push(note(
        "Fix configuration errors, then refresh. Recovery still works.",
        Tone::Neutral,
    ));
    app.presented(
        "Refresh Diagnostics",
        "r",
        Target::Action(Action::Diagnostics),
    );
    dashboard(frame, area, app, "Host details", metrics, lines);
}
