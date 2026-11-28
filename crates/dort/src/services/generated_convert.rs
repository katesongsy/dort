//! Review and outcome text for converting one link mapping into a generated file.
use dort_core::{
    Error, Result,
    generated::{ConversionLocation, ConversionStep, ConvertOutcome, ConvertReview},
};
use std::fmt::Write;

const REVIEW_LIMIT: usize = 64 * 1024;

pub const REVIEW_LABEL: &str = "Review conversion";
pub const CONFIRMATION_LABEL: &str = "Convert to generated file";
pub const RECOVERY_LABEL: &str = "Finish conversion";

pub fn review(change: &ConvertReview<'_>) -> Result<String> {
    let effects = [
        format!(
            "1. Create template file {:?} with permissions {}.",
            change.template, change.template_permissions
        ),
        format!(
            "2. Replace the destination symlink with the rendered file, permissions {}.",
            change.permissions
        ),
        "3. Change the mapping to mode = \"template\" with the template file as its source."
            .to_string(),
    ];
    let mut text = format!(
        "Review conversion: {}\nRepository: {:?}\nActive profiles: {:?}\nSource: {:?}\nTemplate file: {:?}\nDestination: {:?}\nRetained link target: {:?}\nPermissions: {}\nTemplate permissions: {}\n",
        change.id,
        change.repository,
        change.active_profiles,
        change.source,
        change.template,
        change.destination,
        change.retained,
        change.permissions,
        change.template_permissions,
    );
    if let Some(folder) = change.folder {
        let _ = writeln!(text, "Creates folder first: {folder:?}");
    }
    text.push_str("\nEffects, in order\n");
    for effect in &effects {
        let _ = writeln!(text, "{effect}");
    }
    text.push_str(
        "\nThe old symlink is removed after step 2. Its target file stays and the deployment record keeps that path.\nIf a step fails, Recovery shows the finished steps and finishes the rest. Cancel writes nothing.\n\n",
    );
    check_size(&text)?;
    text.push_str("Contents\nRendering must reproduce these bytes.\nSpaces are \\x20. Tabs are \\t. Line endings are \\r and \\n.\n");
    if change.before != change.after {
        let _ = writeln!(text, "First changed line: 1");
        write_lines(&mut text, "OLD", change.before)?;
        write_lines(&mut text, "NEW", change.after)?;
    } else {
        text.push_str("Bytes: unchanged by rendering\n");
        write_lines(&mut text, "SAME", change.before)?;
    }
    let _ = writeln!(text, "End of conversion review");
    let text = super::presentation::safe(&text);
    check_size(&text)?;
    Ok(text)
}

fn write_lines(text: &mut String, label: &str, body: &str) -> Result<()> {
    if body.is_empty() {
        let _ = writeln!(text, "{label} | No lines");
        return Ok(());
    }
    for line in body.split_inclusive('\n') {
        let _ = write!(text, "{label} | ");
        for byte in line.bytes() {
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
        check_size(text)?;
    }
    Ok(())
}

fn check_size(text: &str) -> Result<()> {
    if text.len() > REVIEW_LIMIT {
        return Err(Error::Invalid(
            "The full conversion review is too large to display. Keep the link as is or reduce the source size and try again."
                .into(),
        ));
    }
    Ok(())
}

fn step(step: ConversionStep) -> &'static str {
    match step {
        ConversionStep::TemplateFile => "1. Template file created",
        ConversionStep::Destination => "2. Destination replaced",
        ConversionStep::Mapping => "3. Mapping changed",
    }
}

pub fn steps(completed: &[ConversionStep]) -> String {
    if completed.is_empty() {
        return "No step finished.".into();
    }
    completed
        .iter()
        .map(|s| step(*s))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn recovery_review(location: &ConversionLocation) -> String {
    super::presentation::safe(&format!(
        "{RECOVERY_LABEL}\nSource: {source:?}\nTemplate file: {template:?}\nDestination: {destination:?}\nRetained link target: {retained:?}\n\nFinished steps\n{steps}\n\nThe conversion writes only the steps that are not finished.\nThe destination may already hold the rendered file and the mapping may still be a link.\nIf it stops again, Recovery keeps every finished step.\nEnd of conversion recovery review",
        source = location.retained,
        template = location.template,
        destination = location.destination,
        retained = location.retained,
        steps = steps(&location.completed),
    ))
}

pub fn recovery_status() -> &'static str {
    "Conversion needs recovery."
}

pub fn status(outcome: &ConvertOutcome) -> &'static str {
    match outcome {
        ConvertOutcome::Converted(_) => "Converted",
        ConvertOutcome::NeedsRecovery { .. } => recovery_status(),
    }
}

pub fn outcome(outcome: &ConvertOutcome) -> String {
    let status = status(outcome);
    super::presentation::safe(&match outcome {
        ConvertOutcome::Converted(done) => format!(
            "{status}\nDestination: {:?}\nTemplate file: {:?}\nRetained link target: {:?}\nPermissions: {}\nNext: Preview the generated file for current status.",
            done.destination, done.template, done.retained, done.permissions,
        ),
        ConvertOutcome::NeedsRecovery {
            location,
            completed,
            reason,
        } => format!(
            "{status}\nFinished steps\n{}\nDestination: {:?}\nTemplate file: {:?}\nRetained link target: {:?}\nReason: {}\nNext: Keep any files. Open Recovery and run Finish conversion, or run `dort recovery recover-conversion`. Dort blocks other writes until recovery finishes.",
            steps(completed),
            location.destination,
            location.template,
            location.retained,
            super::presentation::inline(reason),
        ),
    })
}
