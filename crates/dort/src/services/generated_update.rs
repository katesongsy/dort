use dort_core::{
    Error, Result,
    generated::{ExchangeStatus, UpdateAction, UpdateLocation, UpdateOutcome, UpdateReview},
};
use std::fmt::Write;

const REVIEW_LIMIT: usize = 64 * 1024;

pub fn review_label(action: UpdateAction) -> &'static str {
    match action {
        UpdateAction::Update => "Review update",
        UpdateAction::Replace => "Review replacement",
        UpdateAction::Recreate => "Review recreation",
    }
}

pub fn confirmation_label(action: UpdateAction) -> &'static str {
    match action {
        UpdateAction::Update => "Update generated file",
        UpdateAction::Replace => "Replace generated file",
        UpdateAction::Recreate => "Recreate generated file",
    }
}

pub fn recovery_label(action: UpdateAction) -> &'static str {
    match action {
        UpdateAction::Update => "Recover generated update",
        UpdateAction::Replace => "Recover generated replacement",
        UpdateAction::Recreate => "Recover generated recreation",
    }
}

pub fn review(change: &UpdateReview<'_>) -> Result<String> {
    let (action, title, effect) = match change.action {
        UpdateAction::Update => (
            "update",
            "Review generated update",
            "Replace the deployed file with the reviewed output.\nKeep the previous file in a private folder beside the destination.",
        ),
        UpdateAction::Replace => (
            "replacement",
            "Review generated replacement",
            "Replace the local file with the reviewed output.\nKeep the exact local file in a private folder beside the destination.",
        ),
        UpdateAction::Recreate => (
            "recreation",
            "Review generated recreation",
            "Create the reviewed file only if the destination is still missing.\nLeave any file that appears there untouched.",
        ),
    };
    let old = change.deployed;
    let new = change.proposed.as_bytes();
    let source = change.source.map_or_else(
        || "Inline template in dort.toml".into(),
        |path| format!("{path:?}"),
    );
    let mut text = format!(
        "{title}: {}\nRepository: {:?}\nActive profiles: {:?}\nSource: {}\nDestination: {:?}\nDetected: {}\nPermissions: {} -> {}\nFinal newline: {} -> {}\n",
        change.id,
        change.repository,
        change.active_profiles,
        source,
        change.destination,
        change.detected.label(),
        change.deployed_permissions.as_deref().unwrap_or("Missing"),
        change.proposed_permissions,
        change
            .deployed_permissions
            .as_ref()
            .map_or("Missing", |_| newline(old)),
        newline(new),
    );
    if change.action != UpdateAction::Update {
        let _ = writeln!(
            text,
            "Last deployed permissions: {}",
            change.recorded_permissions
        );
        if let Some(path) = &change.retained {
            let _ = writeln!(text, "Retain local file at: {path:?}");
        }
    }
    let _ = write!(
        text,
        "\n{effect}\nSave the new deployment record. If interrupted, open Recovery.\n\nContents:\n"
    );
    if change.action == UpdateAction::Update && old == new {
        text.push_str("No content change.\n");
    } else {
        text.push_str("Spaces are \\x20. Tabs are \\t. Line endings are \\r and \\n.\n");
        if change.action == UpdateAction::Recreate {
            write_lines(&mut text, action, "NEW", &lines(new))?;
        } else {
            if old == new {
                text.push_str("No content change.\n");
            }
            let old = lines(old);
            let new = lines(new);
            let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let suffix = old[prefix..]
                .iter()
                .rev()
                .zip(new[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            if change.action == UpdateAction::Replace {
                text.push_str("SAME shows matching lines. OLD and NEW show the changed block.\n");
                if prefix != 0 {
                    write_lines(&mut text, action, "SAME", &new[..prefix])?;
                }
            } else {
                text.push_str(
                    "OLD and NEW show the changed block. Matching outer lines are omitted.\n",
                );
            }
            if old != new {
                let _ = writeln!(text, "First changed line: {}", prefix + 1);
                write_lines(&mut text, action, "OLD", &old[prefix..old.len() - suffix])?;
                write_lines(&mut text, action, "NEW", &new[prefix..new.len() - suffix])?;
            } else if new.is_empty() {
                write_lines(&mut text, action, "SAME", &[])?;
            }
            if change.action == UpdateAction::Replace && suffix != 0 {
                write_lines(&mut text, action, "SAME", &new[new.len() - suffix..])?;
            }
        }
    }
    let _ = writeln!(text, "End of {action} review");
    let text = super::presentation::safe(&text);
    check_size(&text, action)?;
    Ok(text)
}

fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == b'\n').collect()
}

fn write_lines(text: &mut String, action: &str, label: &str, lines: &[&[u8]]) -> Result<()> {
    if lines.is_empty() {
        let _ = writeln!(text, "{label} | No lines");
    }
    for line in lines {
        let _ = write!(text, "{label} | ");
        for byte in line.iter().copied() {
            match byte {
                b'\t' => text.push_str("\\t"),
                b'\r' => text.push_str("\\r"),
                b'\n' => text.push_str("\\n"),
                b'\\' => text.push_str("\\\\"),
                b'!'..=b'~' => text.push(char::from(byte)),
                _ => {
                    let _ = write!(text, "\\x{byte:02x}");
                }
            }
        }
        text.push('\n');
        check_size(text, action)?;
    }
    Ok(())
}

fn newline(bytes: &[u8]) -> &'static str {
    if bytes.ends_with(b"\n") { "Yes" } else { "No" }
}

fn check_size(text: &str, action: &str) -> Result<()> {
    if text.len() > REVIEW_LIMIT {
        return Err(Error::Invalid(format!(
            "The full {action} review is too large to display. Keep the file as is or reduce the review size and try again."
        )));
    }
    Ok(())
}

fn locations(location: &UpdateLocation) -> String {
    format!(
        "Destination: {:?}\nPrivate file (if present and its folder has not moved): {:?}\nPrivate folder device: {}\nPrivate folder inode: {}",
        location.destination, location.artifact, location.stage_device, location.stage_inode,
    )
}

pub fn recovery_review(location: &UpdateLocation) -> String {
    let effect = if location.action == UpdateAction::Recreate {
        "The private path holds the staged file before recreation.\nIf Dort already created the file, the private path should be empty.\nRecovery never creates the file or overwrites the destination."
    } else {
        "The private path holds the staged file before replacement or the previous file afterward.\nRecovery checks both locations. It does not replace either file."
    };
    super::presentation::safe(&format!(
        "{}\n{}\n\nThe new file may already be at the destination.\n{effect}\nRecovery checks the files and deployment record before closing the action. If it stops, keep any files and inspect the reported paths.\nEnd of recovery review\n",
        recovery_label(location.action),
        locations(location),
    ))
}

pub fn recovery_status(action: UpdateAction) -> &'static str {
    match action {
        UpdateAction::Update => "Generated update needs recovery.",
        UpdateAction::Replace => "Generated replacement needs recovery.",
        UpdateAction::Recreate => "Generated recreation needs recovery.",
    }
}

pub fn status(outcome: &UpdateOutcome) -> &'static str {
    match outcome {
        UpdateOutcome::Updated { location } => match location.action {
            UpdateAction::Update => "Generated update recorded.",
            UpdateAction::Replace => "Generated replacement recorded.",
            UpdateAction::Recreate => "Generated recreation recorded.",
        },
        UpdateOutcome::NotExchanged { location, .. } => match location.action {
            UpdateAction::Recreate => "Dort did not recreate the file.",
            UpdateAction::Replace => "Dort did not replace the local file.",
            UpdateAction::Update => "Dort did not replace the deployed file.",
        },
        UpdateOutcome::NeedsRecovery { location, .. } => recovery_status(location.action),
    }
}

pub fn outcome(outcome: &UpdateOutcome) -> String {
    let status = status(outcome);
    super::presentation::safe(&match outcome {
        UpdateOutcome::Updated { location } if location.action == UpdateAction::Recreate => {
            format!(
                "{status}\nDestination: {:?}\nNext: Preview the generated file for current status.",
                location.destination,
            )
        }
        UpdateOutcome::Updated { location } => format!(
            "{status}\n{}\nThe previous file was retained in the private folder.\nNext: Preview the generated file for current status.",
            locations(location),
        ),
        UpdateOutcome::NotExchanged { location, reason } => format!(
            "{status}\n{}\nReason: {}\nNext: Check both locations and open a new review to retry.",
            locations(location),
            super::presentation::inline(reason),
        ),
        UpdateOutcome::NeedsRecovery {
            location,
            exchange,
            reason,
        } => {
            let effect = match exchange {
                ExchangeStatus::Completed => {
                    "Dort placed the new file at the destination. It may still be visible there."
                }
                ExchangeStatus::NotPerformed => "Dort did not change the destination.",
                ExchangeStatus::Unknown => {
                    "Dort could not confirm which file is at the destination.\nThe new file may already be visible there."
                }
            };
            format!(
                "{status}\n{effect}\n{}\nReason: {}\nNext: Keep any files. Open Recovery or run `dort recovery recover-update` after checking the paths. Dort blocks other writes until recovery finishes.",
                locations(location),
                super::presentation::inline(reason),
            )
        }
    })
}
