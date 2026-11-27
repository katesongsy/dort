use dort_core::{
    Error, Result,
    generated::{ExchangeStatus, UpdateLocation, UpdateOutcome, UpdateReview},
};
use std::fmt::Write;

const REVIEW_LIMIT: usize = 64 * 1024;

pub fn review(change: UpdateReview<'_>) -> Result<String> {
    let mut text = format!(
        "Review generated update: {}\nRepository: {:?}\nActive profiles: {:?}\nSource: {}\nDestination: {:?}\nPermissions: {} -> {}\nFinal newline: {} -> {}\n\nReplace the deployed file with the reviewed output.\nKeep the previous file in a private folder beside the destination.\nSave the new deployment record. If interrupted, open Recovery.\n\nContents:\n",
        change.id,
        change.repository,
        change.active_profiles,
        change.source.map_or_else(
            || "Inline template in dort.toml".into(),
            |path| format!("{path:?}")
        ),
        change.destination,
        change.deployed_permissions,
        change.proposed_permissions,
        newline(change.deployed),
        newline(change.proposed.as_bytes()),
    );
    let old = change.deployed;
    let new = change.proposed.as_bytes();
    if old == new {
        text.push_str("No content change.\n");
    } else {
        text.push_str("OLD and NEW show the changed block. Matching outer lines are omitted.\nSpaces are \\x20. Tabs are \\t. Line endings are \\r and \\n.\n");
        let old: Vec<_> = old.split_inclusive(|byte| *byte == b'\n').collect();
        let new: Vec<_> = new.split_inclusive(|byte| *byte == b'\n').collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let _ = writeln!(text, "First changed line: {}", prefix + 1);
        for (label, lines) in [("OLD", old), ("NEW", new)] {
            let changed = &lines[prefix..lines.len() - suffix];
            if changed.is_empty() {
                let _ = writeln!(text, "{label} | No lines");
            }
            for line in changed {
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
                check_size(&text)?;
            }
        }
    }
    text.push_str("End of update review\n");
    let text = super::presentation::safe(&text);
    check_size(&text)?;
    Ok(text)
}

fn newline(bytes: &[u8]) -> &'static str {
    if bytes.ends_with(b"\n") { "Yes" } else { "No" }
}

fn check_size(text: &str) -> Result<()> {
    if text.len() > REVIEW_LIMIT {
        return Err(Error::Invalid(
            "The full update review is too large to display. Split the template change into smaller steps, then review again.".into(),
        ));
    }
    Ok(())
}

fn locations(location: &UpdateLocation) -> String {
    format!(
        "Destination: {:?}\nPrivate file (unless its folder moved): {:?}\nPrivate folder device: {}\nPrivate folder inode: {}",
        location.destination, location.artifact, location.stage_device, location.stage_inode,
    )
}

pub fn recovery_review(location: &UpdateLocation) -> String {
    super::presentation::safe(&format!(
        "Recover generated update\n{}\n\nThe new file may already be at the destination.\nRecovery uses the current files and saved record to finish or clear the pending update. It does not replace either file. If recovery stops, keep both files and inspect the reported paths.\nEnd of recovery review\n",
        locations(location),
    ))
}

pub fn outcome(outcome: &UpdateOutcome) -> String {
    super::presentation::safe(&match outcome {
        UpdateOutcome::Updated { location } => format!(
            "Generated update recorded.\n{}\nThe previous file was retained in the private folder.\nNext: Preview the generated file for current status.",
            locations(location),
        ),
        UpdateOutcome::NotExchanged { location, reason } => format!(
            "Dort did not replace the deployed file.\n{}\nReason: {}\nNext: Check both file locations and open a new review to retry.",
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
                    "Dort exchanged the files.\nThe new file may already be visible at the destination."
                }
                ExchangeStatus::NotPerformed => "Dort did not replace the deployed file.",
                ExchangeStatus::Unknown => {
                    "Dort could not confirm which file is at the destination.\nThe new file may already be visible there."
                }
            };
            format!(
                "Generated update needs recovery.\n{effect}\n{}\nReason: {}\nNext: Keep both files. Open Recovery or run `dort recovery recover-update` after checking the paths. Dort blocks other writes while this update is unresolved.",
                locations(location),
                super::presentation::inline(reason),
            )
        }
    })
}
