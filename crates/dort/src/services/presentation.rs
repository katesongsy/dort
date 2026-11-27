//! Terminal-safe application reports shared by CLI and TUI.
use dort_core::tools::{inspection, install};
pub fn safe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\n' || c.is_ascii_graphic() || c == ' ' {
            escaped.push(c);
        } else {
            escaped.extend(c.escape_default());
        }
    }
    escaped
}
pub fn inline(text: &str) -> String {
    safe(text).replace('\n', "\\n")
}
pub fn generated_inputs(preview: &dort_core::generated::Preview) -> String {
    if preview.inputs.is_empty() {
        "None".into()
    } else {
        preview
            .inputs
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn generated_source(preview: &dort_core::generated::Preview) -> String {
    preview.source.as_ref().map_or_else(
        || "Inline template in dort.toml".into(),
        |source| source.display().to_string(),
    )
}
pub fn generated(preview: &dort_core::generated::Preview) -> String {
    format!(
        "Generated file preview: {}\nNo files written.\n{}",
        inline(&preview.id),
        generated_details(preview)
    )
}
pub fn generated_details(preview: &dort_core::generated::Preview) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "Source: {}\nDestination: {}\n",
        inline(&generated_source(preview)),
        inline(&preview.destination.display().to_string()),
    );
    if let Some(status) = preview.drift_status {
        let _ = writeln!(text, "Status: {}\n{}", status.label(), status.guidance());
    }
    let _ = writeln!(
        text,
        "Inputs from dort.toml: {}\nProposed permissions: {}",
        generated_inputs(preview),
        preview.proposed_permissions(),
    );
    let _ = writeln!(
        text,
        "Final newline: {}",
        if preview.rendered.ends_with('\n') {
            "Yes"
        } else {
            "No"
        }
    );
    text.push_str("\nRendered output\n");
    text.push_str(&safe(&preview.rendered));
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
pub fn generated_deployment(review: dort_core::generated::DeploymentReview<'_>) -> String {
    use std::fmt::Write;
    let mut report = format!(
        "Review first deployment: {}\nRepository: {}\nActive profiles: {}\nSource: {}\nCreate: {}\nPermissions: {}\n\nRendered output (spaces as \\x20, newlines as \\n)\n",
        review.id,
        inline(&review.repository.display().to_string()),
        review
            .active_profiles
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        review.source.map_or_else(
            || "Inline template in dort.toml".into(),
            |path| inline(&path.display().to_string()),
        ),
        inline(&review.destination.display().to_string()),
        review.permissions,
    );
    if review.rendered.is_empty() {
        report.push_str("Empty output\n");
    }
    for chunk in review.rendered.split_inclusive('\n') {
        for byte in chunk.bytes() {
            match byte {
                b' ' => report.push_str("\\x20"),
                b'\t' => report.push_str("\\t"),
                b'\r' => report.push_str("\\r"),
                b'\n' => report.push_str("\\n"),
                b'\\' => report.push_str("\\\\"),
                b'!'..=b'~' => report.push(char::from(byte)),
                _ => {
                    let _ = write!(report, "\\x{byte:02x}");
                }
            }
        }
        report.push('\n');
    }
    report.push_str("End of rendered output\n");
    report
}
pub fn generated_published(result: &dort_core::generated::Published) -> String {
    use std::fmt::Write;
    let mut written = format!(
        "Deployed generated file: {}\nPermissions: {}",
        inline(&result.destination.display().to_string()),
        result.permissions,
    );
    if !result.directory_synced {
        written.push_str("\nWarning: Dort could not sync the destination folder. The file was created, but may not survive a restart.");
    }
    if let Some(warning) = result.record_warning {
        let _ = write!(written, "\nWarning: {warning}");
    }
    written
}
fn paths(paths: &[std::path::PathBuf]) -> String {
    paths
        .iter()
        .map(|p| inline(&p.display().to_string()))
        .collect::<Vec<_>>()
        .join(":")
}
pub fn inspection(scope: &inspection::Description) -> String {
    let mut text = format!(
        "{}\n  Program: {}\n  Arguments: {}\n  HOME: {}\n  Command paths: {}\n",
        super::installed_tools::label(scope.query),
        inline(&scope.executable.display().to_string()),
        scope
            .arguments
            .iter()
            .map(|a| inline(a))
            .collect::<Vec<_>>()
            .join(" "),
        inline(&scope.home.display().to_string()),
        paths(&scope.trusted_search_path)
    );
    if let Some(prefix) = &scope.npm_global_prefix {
        text.push_str(&format!(
            "  Global prefix: {}\n",
            inline(&prefix.display().to_string())
        ));
    }
    if let Some(npm) = &scope.npm_binding {
        text.push_str(&format!(
            "  Node: {}\n  npm CLI: {}\n  Global prefix: {}\n",
            inline(&npm.node.display().to_string()),
            inline(&npm.npm_cli.display().to_string()),
            inline(&npm.prefix.display().to_string())
        ));
    }
    text
}
pub fn installed_review(review: &super::installed_tools::Review<'_>) -> String {
    let mut text = String::from("Inspect installed tools\n");
    if !review.inspections.is_empty() {
        text.push_str("Package-manager configuration/plugins may run; caches may change. No packages are installed.\n");
    }
    for query in &review.inspections {
        if let Some(probe) = query.prefix_probe {
            text.push_str("Global location query:\n");
            text.push_str(&inspection(probe));
            text.push_str("Package listing uses the reported prefix. Node is selected from the displayed PATH.\n");
            text.push_str(query.provider.warning);
            text.push('\n');
        }
        text.push_str(&inspection(query.provider));
    }
    for issue in review.unavailable {
        text.push_str(&format!(
            "{}: {}\n",
            super::installed_tools::label(issue.provider),
            inline(&issue.reason)
        ));
    }
    text
}
pub fn installed(current: &super::installed_tools::Current) -> String {
    let mut text = String::from("Packages reported by this check:\n");
    for report in &current.reports {
        text.push_str(&format!(
            "{}\n",
            super::installed_tools::label(report.provider.description.query)
        ));
        if let Some(prefix) = &report.provider.description.npm_global_prefix {
            text.push_str(&format!(
                "  Global prefix: {}\n",
                inline(&prefix.display().to_string())
            ));
        }
        match &report.provider.observation {
            inspection::Observation::Complete { installations } if installations.is_empty() => {
                text.push_str("  No installed packages reported.\n")
            }
            inspection::Observation::Complete { installations } => {
                for p in installations {
                    text.push_str(&format!(
                        "  {}  {}\n",
                        inline(&p.package),
                        inline(&p.version)
                    ));
                }
            }
            inspection::Observation::Unavailable { reason } => {
                text.push_str(&format!("  Check unavailable: {reason}\n"))
            }
        }
    }
    for issue in &current.unavailable {
        text.push_str(&format!(
            "{}: {}\n",
            super::installed_tools::label(issue.provider),
            inline(&issue.reason)
        ));
    }
    text.push_str(
        "From this check. Packages may change; a package does not prove its command works.\n",
    );
    text
}
pub fn check_review(review: &super::declared_tools::CheckReview<'_>) -> String {
    format!("Check installed\n{}", check_scope(review))
}

pub fn check_scope(review: &super::declared_tools::CheckReview<'_>) -> String {
    let mut text = format!(
        "Manifest: {}\nProfiles: {}\n{}\n",
        inline(&review.scope.manifest.display().to_string()),
        review
            .scope
            .active_profiles
            .iter()
            .map(|name| inline(name))
            .collect::<Vec<_>>()
            .join(", "),
        review.warning
    );
    for scope in &review.inspections {
        text.push_str(&format!(
            "Tools: {}\n",
            scope
                .tools
                .iter()
                .map(|t| inline(t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        text.push_str(&inspection(scope.query.provider));
    }
    for issue in review.needs_setup {
        text.push_str(&format!(
            "{}: {}\nNext: {}\n",
            inline(&issue.tool),
            inline(&issue.reason),
            inline(&issue.next_step)
        ));
    }
    text
}
pub fn install_review(review: &super::declared_tools::InstallReview<'_>) -> String {
    let mut text = format!(
        "{} ready to install\n{}\n",
        review.installations.len(),
        review.warning
    );
    for (tool, install) in &review.installations {
        let s = install.scope;
        text.push_str(&format!("{}: {} {}\n  Installer: {}\n  Arguments: {}\n  Repository: {}\n  HOME: {}\n  Package manager paths: {}\n  Tool command paths: {}\n{}\n", inline(tool), inline(&s.package), inline(&s.version), inline(&install.installer.display().to_string()), install.arguments.iter().map(|a| inline(a)).collect::<Vec<_>>().join(" "), inline(&s.repository.display().to_string()), inline(&s.home.display().to_string()), paths(&s.search_dirs), paths(&s.tool_search_dirs), install.warning));
        if let Some(brew) = &s.brew {
            text.push_str(&format!(
                "  Homebrew prefix: {}\n  Homebrew repository: {}\n  Cellar: {}\n",
                inline(&brew.prefix.display().to_string()),
                inline(&brew.repository.display().to_string()),
                inline(&brew.cellar.display().to_string())
            ));
        }
        for query in &install.fresh_inspections {
            text.push_str(&inspection(query));
        }
        for probe in &install.fresh_probes {
            text.push_str(&format!(
                "  Fresh probe: {}\n",
                probe
                    .iter()
                    .map(|a| inline(a))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    for issue in review.deferred {
        text.push_str(&format!(
            "Not included: {}\nWhy: {}\nNext: {}\n",
            inline(&issue.tool),
            inline(&issue.reason),
            inline(&issue.next_step)
        ));
    }
    text
}
pub fn installed_result(result: &super::declared_tools::Installed) -> String {
    let mut text = String::from("Installation results\n");
    for row in &result.results {
        if let Some(report) = &row.report {
            text.push_str(&format!(
                "{}: {}\n{}\n",
                inline(&row.tool),
                report.summary().status.label(),
                outcome(report.summary())
            ));
        }
        if let Some(error) = &row.error {
            text.push_str(&format!("{}: {}\n", inline(&row.tool), inline(error)));
        }
    }
    if !result.not_started.is_empty() {
        text.push_str(&format!(
            "Not started: {}\n",
            result
                .not_started
                .iter()
                .map(|t| inline(t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    text
}
fn outcome(summary: install::Summary) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        summary.explanation,
        summary.package_note(),
        summary.binary_note,
        summary.next_step_hint
    )
}
pub fn close_review(review: &install::CloseReview<'_>) -> String {
    let mut text = format!(
        "Close installation record {}\n{}\n{}\n",
        inline(review.attempt_id),
        review.explanation,
        review.query.warning
    );
    text.push_str(&inspection(review.query));
    if let Some(prefix) = review.brew_prefix {
        text.push_str(&format!(
            "Homebrew prefix: {}\n",
            inline(&prefix.display().to_string())
        ));
    }
    for probe in &review.fresh_probes {
        text.push_str(&format!(
            "Fresh probe: {}\n",
            probe
                .iter()
                .map(|p| inline(p))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    text
}
pub fn attempt(attempt: &install::Attempt) -> String {
    let location = attempt
        .scope
        .as_ref()
        .map(|s| {
            format!(
                "Tool: {}\nPackage: {} {}\nRepository: {}\nProgram: {}\n",
                inline(&s.tool),
                inline(&s.package),
                inline(&s.version),
                inline(&s.repository.display().to_string()),
                inline(&s.executable.display().to_string())
            )
        })
        .unwrap_or_else(|| "Original scope unavailable.\n".into());
    format!(
        "Installation {}: {}\n{}{}\n",
        inline(&attempt.id),
        attempt.summary().status.label(),
        location,
        outcome(attempt.summary())
    )
}
