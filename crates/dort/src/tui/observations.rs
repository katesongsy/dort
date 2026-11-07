//! Typed read-only dashboards. Presentation never derives identity or policy from prose.
use super::{app::App, components as ui};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
};

struct Metric {
    label: &'static str,
    value: String,
    style: Style,
}
fn metric(label: &'static str, value: impl ToString, style: Style) -> Metric {
    Metric {
        label,
        value: value.to_string(),
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
    // Short layouts retain a compact summary strip and a scrollable detail pane.
    let cards = area.height >= 11;
    let rows = Layout::vertical([
        Constraint::Length(if cards { 3 } else { 1 }),
        Constraint::Min(3),
    ])
    .split(area);
    let columns = Layout::horizontal(vec![
        Constraint::Ratio(1, metrics.len() as u32);
        metrics.len()
    ])
    .split(rows[0]);
    for (area, metric) in columns.iter().zip(metrics) {
        if cards {
            frame.render_widget(
                Paragraph::new(ui::clip(&metric.value, area.width.saturating_sub(2)))
                    .style(metric.style)
                    .block(ui::pane(metric.label, false)),
                *area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(ui::clip(
                    &match metric.label {
                        "Repository" | "Manifest" => metric.value,
                        "History blockers" | "Blockers" => format!("Blocks: {}", metric.value),
                        label => format!("{label} {}", metric.value),
                    },
                    area.width,
                ))
                .style(metric.style),
                *area,
            );
        }
    }
    ui::report(
        frame,
        rows[1],
        title,
        Text::from(lines),
        app.details_scroll,
        true,
    );
}
fn count(value: Option<usize>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Unknown".into())
}

pub fn overview(frame: &mut Frame, area: Rect, app: &App) {
    let Some(data) = &app.data else {
        ui::report(
            frame,
            area,
            "Overview",
            Text::raw("Loading read-only observations..."),
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
                ui::accent()
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
                ui::danger()
            },
        ),
    ];
    let mut lines = vec![];
    if let Some(result) = &app.result {
        section(&mut lines, "Last result");
        lines.extend(result.lines().map(|line| note(line, ui::accent())));
    }
    if root.is_none() {
        section(&mut lines, "Welcome | manage local files with previews");
        lines.push(note(
            "Session: Create/attach / Choose repository",
            ui::accent(),
        ));
        lines.push(note("Files: Discover | Checks: manifest", ui::accent()));
        lines.push(note(
            "Recovery and Diagnostics are available independently of repository selection.",
            Style::default(),
        ));
    }
    section(&mut lines, "Repository & scope");
    lines.push(field(
        "Repository",
        root.map(|p| format!("{p:?}"))
            .unwrap_or_else(|| "Not selected".into()),
    ));
    lines.push(field("Selection origin", app.origin));
    lines.push(field("Registration", registration));
    if observation.and_then(|o| o.registered) == Some(false) {
        lines.push(note(
            "Attach before writing. n: Create/attach this repository.",
            ui::warning(),
        ));
    }
    lines.push(field(
        "Registered repositories",
        count(observation.and_then(|o| o.registry_count)),
    ));
    if let Some(error) = observation.and_then(|o| o.configuration_error.as_ref()) {
        lines.push(field("Configuration unavailable", error));
        lines.push(note(
            "Registry metadata unavailable; counts and active selection unresolved.",
            ui::warning(),
        ));
    }
    if files.is_some() {
        lines.push(field("Effective profiles", format!("{:?}", data.profiles)));
    } else {
        lines.push(field(
            "Effective profiles",
            "Unknown (file observation unavailable)",
        ));
    }
    section(&mut lines, "Active file observations");
    if let Some(files) = files {
        lines.push(field("Linked", files.linked));
        lines.push(field("Needs apply", files.changes));
        lines.push(field("Blocked mappings", files.blocked));
        lines.push(field("Plan blockers", files.plan_blocked));
        lines.push(note(
            "2 Files: inspect full source/target metadata. Apply uses the full active scope.",
            Style::default(),
        ));
    } else {
        lines.push(note("File counts unavailable.", ui::warning()));
    }
    if let Some(error) = &data.files_error {
        lines.extend(error.lines().map(|line| note(line, ui::warning())));
    }
    section(&mut lines, "All-repository history");
    lines.push(field(
        "Transactions",
        count(data.history_error.is_none().then_some(data.history.len())),
    ));
    lines.push(field("Global write blockers", count(blockers)));
    if let Some(error) = &data.history_error {
        lines.push(field("History unavailable", error));
        lines.push(note(
            "Global write safety unknown. Recovery remains independently refreshable.",
            ui::warning(),
        ));
    }
    section(&mut lines, "Host & limits");
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
        "Use your preferred tools for providers, templates, secrets and Git writes.",
        ui::muted(),
    ));
    lines.push(note(
        "Execution requires explicit approval of a fresh preview. Failed writes may retain partial effects.",
        ui::warning(),
    ));
    dashboard(
        frame,
        area,
        app,
        if root.is_none() {
            "Welcome"
        } else {
            "Overview"
        },
        metrics,
        lines,
    );
}

pub fn checks(frame: &mut Frame, area: Rect, app: &App) {
    if app.showing_diagnostics {
        diagnostics(frame, area, app);
        return;
    }
    let mut lines = vec![];
    let metrics;
    if let Some((report, standalone)) = &app.check {
        metrics = vec![
            metric("Manifest", "Valid", ui::accent()),
            metric("Files", report.selected_files, ui::heading()),
            metric("Tools", report.selected_tools, ui::heading()),
        ];
        section(&mut lines, "Structural check only");
        lines.push(note(
            "Declarations validated; inspect Files for live state and verify installed tools separately.",
            ui::warning(),
        ));
        lines.push(field(
            "Scope",
            if *standalone {
                "standalone manifest (independent of file repository)"
            } else {
                "current repository"
            },
        ));
        lines.push(field("Valid manifest", format!("{:?}", report.manifest)));
        lines.push(field("Schema", report.schema));
        section(&mut lines, "Effective profiles | combined contributors");
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
        lines.push(field("Optional additions", format!("{additions:?}")));
        section(&mut lines, "Selected declarations");
        lines.push(field("Selected files", report.selected_files));
        lines.push(field("Selected tools", report.selected_tools));
        lines.push(field(
            "Tool dependency order",
            format!("{:?}", report.tool_dependency_order),
        ));
        section(&mut lines, "Capability limits");
        if report.unsupported_capabilities.is_empty() {
            lines.push(note(
                "Selected declarations are supported. Files shows live state.",
                Style::default(),
            ));
        }
        for capability in &report.unsupported_capabilities {
            lines.push(field("Not implemented", capability));
        }
        lines.push(note(
            "Verify tool readiness and prerequisites separately.",
            ui::muted(),
        ));
    } else {
        metrics = vec![
            metric("Manifest", "Not checked", ui::warning()),
            metric("Files", "Unknown", ui::muted()),
            metric("Tools", "Unknown", ui::muted()),
        ];
        section(&mut lines, "Check a manifest");
        lines.push(ui::keys(&[
            ("c", "Current repository"),
            ("v", "Standalone manifest"),
        ]));
        lines.push(note(
            "Standalone needs no repository, registration or Git.",
            Style::default(),
        ));
        lines.push(note(
            "Checks validate declarations. Files inspects live state; verify installed tools separately.",
            ui::warning(),
        ));
    }
    section(&mut lines, "Read-only actions");
    lines.push(ui::keys(&[("r", "Recheck this scope")]));
    dashboard(
        frame,
        area,
        app,
        match &app.check {
            Some((_, true)) => "Checks | Standalone manifest",
            Some((_, false)) => "Checks | Repository manifest",
            None => "Checks | Manifest result",
        },
        metrics,
        lines,
    );
}

fn diagnostics(frame: &mut Frame, area: Rect, app: &App) {
    let Some(report) = &app.diagnostics else {
        ui::report(
            frame,
            area,
            "Diagnostics",
            Text::raw("Diagnostics not loaded. j: read host, paths and registry metadata"),
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
    lines.push(field(
        "All-repository transactions",
        count(history.map(|h| h.0)),
    ));
    lines.push(field("Global write blockers", count(history.map(|h| h.1))));
    if let Some(registry) = registry {
        lines.push(field(
            "Registered active ID",
            format!("{:?}", registry.active_repository),
        ));
    }
    if let Err(error) = &report.registry {
        lines.push(field("Configuration unavailable", error));
        lines.push(note(
            "Registry metadata unavailable; counts and active selection unresolved.",
            ui::warning(),
        ));
    }
    if let Err(error) = &report.history {
        lines.push(field("History unavailable", error));
        lines.push(note("Global write safety unknown.", ui::warning()));
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
    lines.push(field("Selection origin", app.origin));
    lines.push(field("Git executable", format!("{:?}", app.git)));
    lines.push(field(
        "Git session root",
        format!("{:?}", app.git_view.root),
    ));
    lines.push(field("Git follows file root", app.git_view.follows_files));
    section(&mut lines, "Capability limits");
    lines.push(note(
        "Files inspects live state; verify installed tools separately.",
        ui::warning(),
    ));
    lines.push(note(
        "Diagnostics shows host and path metadata.",
        Style::default(),
    ));
    lines.push(note("Supported: guarded local file workflows and recovery; structural checks; hardened read-only Git.", Style::default()));
    lines.push(note(
        "Unavailable: providers, templates, secrets, cloning/sync, network and Git writes.",
        ui::muted(),
    ));
    lines.push(note("Git requires separately installed trusted Git; other workflows remain available when Git is missing.", Style::default()));
    lines.push(note("Configuration errors: correct configuration privately, then r to retry. Recovery remains independent.", Style::default()));
    lines.push(ui::keys(&[("r", "Refresh Diagnostics")]));
    dashboard(frame, area, app, "Checks | Diagnostics", metrics, lines);
}
