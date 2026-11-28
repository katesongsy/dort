//! Review and outcome text for one request that creates a template file and its mapping.
use super::{
    presentation::{inline, write_generated_body},
    template_files::contents,
};
use dort_core::generated::compose::{Outcome, Review};
use std::{fmt::Write, path::PathBuf};

pub fn review(review: &Review<'_>) -> String {
    let mut text = format!(
        "Template file: {}\nFile permissions: {:04o}\n",
        inline(&review.source.display().to_string()),
        review.permissions
    );
    if let Some(folder) = review
        .template
        .as_ref()
        .and_then(|file| file.folder.as_ref())
    {
        let _ = writeln!(
            text,
            "Create folder: {} | permissions {:04o} | owned by you\nThe folder is new and stays if the file step fails.",
            inline(&folder.path.display().to_string()),
            folder.permissions
        );
    }
    if review.template.is_none() {
        text.push_str("This file is already in place. The approval writes the mapping only.\n");
    }
    if let Some(bytes) = review.template.as_ref().and_then(|file| file.after) {
        contents(&mut text, "File contents", bytes);
    }
    text.push('\n');
    match review.rendered {
        Some(rendered) => {
            let _ = writeln!(
                text,
                "Source: {} (new file)\nDestination: {}",
                inline(&review.source.display().to_string()),
                inline(&rendered.destination.display().to_string()),
            );
            write_generated_body(&mut text, rendered);
        }
        None => text.push_str(
            "No rendered output yet. Set the local input values, then review the mapping again.\n",
        ),
    }
    text.push_str(
        "\nSave the template file and its mapping in dort.toml. Home files stay unchanged.\n",
    );
    text
}

pub fn outcome(outcome: &Outcome) -> String {
    let mut text = String::new();
    match outcome {
        Outcome::Created {
            folder_created,
            backup,
        } => {
            folder(&mut text, *folder_created);
            text.push_str("Created template file.\nWrote the mapping in dort.toml.\n");
            previous_manifest(&mut text, backup);
        }
        Outcome::Mapped { backup: previous } => {
            text.push_str("Template file was already in place.\nWrote the mapping in dort.toml.\n");
            previous_manifest(&mut text, previous);
        }
        Outcome::MappingNotWritten {
            file,
            folder_created,
            reason,
            remedy,
        } => {
            folder(&mut text, *folder_created);
            let _ = writeln!(
                text,
                "Template file kept: {}\nMapping not written: {}\n{}",
                inline(&file.display().to_string()),
                inline(reason),
                remedy
            );
        }
    }
    text
}

fn folder(text: &mut String, created: bool) {
    if created {
        text.push_str("Created templates/ with permissions 0755.\n");
    }
}

fn previous_manifest(text: &mut String, previous: &Option<PathBuf>) {
    if let Some(path) = previous {
        let _ = writeln!(
            text,
            "Previous dort.toml: {}",
            inline(&path.display().to_string())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_outcome_keeps_the_file_path_and_remedy_when_the_mapping_fails() {
        let text = outcome(&Outcome::MappingNotWritten {
            file: PathBuf::from("templates/prefs.jinja"),
            folder_created: true,
            reason: "dort.toml is read only.".into(),
            remedy: "The template file stays in place.",
        });
        assert!(
            text.contains("Created templates/ with permissions 0755."),
            "{text}"
        );
        assert!(
            text.contains("Template file kept: templates/prefs.jinja"),
            "{text}"
        );
        assert!(
            text.contains("Mapping not written: dort.toml is read only."),
            "{text}"
        );
        assert!(text.contains("The template file stays in place."), "{text}");
        assert!(!text.contains("Wrote the mapping"), "{text}");
    }

    #[test]
    fn compose_outcome_of_a_repeated_approval_reports_the_mapping_and_the_backup() {
        let text = outcome(&Outcome::Mapped {
            backup: Some(PathBuf::from("/state/manifest-edits/prefs/dort.toml")),
        });
        assert!(
            text.contains("Template file was already in place."),
            "{text}"
        );
        assert!(text.contains("Wrote the mapping in dort.toml."), "{text}");
        assert!(
            text.contains("Previous dort.toml: /state/manifest-edits/prefs/dort.toml"),
            "{text}"
        );
    }
}
