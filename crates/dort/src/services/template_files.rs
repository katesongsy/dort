use super::presentation::inline;
use dort_core::template_files::{Action, Outcome, Removed, Retained, Review, Row, Source};
use dort_core::{Error, Result, template::SOURCE_LIMIT};
use std::{
    fmt::Write,
    fs::{File, OpenOptions},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub fn read_proposed(path: &Path) -> Result<String> {
    let unreadable = |_| Error::Invalid("Could not read the proposed template file.".into());
    let file: File = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(unreadable)?;
    if !file.metadata().map_err(unreadable)?.is_file() {
        return Err(Error::Invalid("Use a regular file for --from.".into()));
    }
    let mut bytes = Vec::new();
    file.take(SOURCE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > SOURCE_LIMIT {
        return Err(Error::Invalid("Template exceeds 64 KiB.".into()));
    }
    String::from_utf8(bytes).map_err(|_| Error::Invalid("Template must be UTF-8.".into()))
}

pub fn list(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "No template files found.\n".into();
    }
    let mut text = String::new();
    for row in rows {
        let _ = writeln!(text, "{}", summary(row));
    }
    text
}

fn path(path: &std::path::Path) -> String {
    inline(&path.display().to_string())
}

fn summary(row: &Row) -> String {
    let access = if row.editable {
        "Editable"
    } else {
        "Read only"
    };
    format!("{} | {} mappings | {access}", path(&row.path), row.mappings)
}

pub fn review(change: Review<'_>) -> String {
    let mut text = format!(
        "{} template file: {}\n",
        change.action.label(),
        path(change.path)
    );
    if let Some(folder) = &change.folder {
        let _ = writeln!(
            text,
            "Create folder: {} | permissions {:04o} | owned by you\nThe folder is new and stays if the file step fails.",
            path(&folder.path),
            folder.permissions
        );
    }
    let _ = writeln!(
        text,
        "Permissions: {:04o}\nMappings declaring this path: {}",
        change.permissions, change.mappings
    );
    match change.action {
        Action::Delete => {
            text.push_str(
                "The file moves to private Dort state. Mappings and Home files stay unchanged.\n",
            );
        }
        _ => text.push_str("Mappings and Home files stay unchanged.\n"),
    }
    if let Some(removed) = &change.removed {
        let _ = writeln!(text, "Retained copy: {}", path(removed));
    }
    if let Some(retained) = change.retained {
        backups(&mut text, &retained);
    }
    if let Some(before) = change.before {
        contents(&mut text, "Before", before);
    }
    if let Some(after) = change.after {
        contents(&mut text, "After", after);
    }
    text
}

pub fn show(source: &Source) -> String {
    let mut text = format!("{} | {:04o}\n", summary(source.row()), source.permissions());
    contents(&mut text, "Contents", source.text());
    text
}

pub(super) fn contents(text: &mut String, label: &str, body: &str) {
    let _ = writeln!(text, "\n{label} (escaped text)");
    if body.is_empty() {
        text.push_str("Empty file\n");
    }
    for line in body.split_inclusive('\n') {
        text.push('|');
        for ch in line.chars() {
            match ch {
                '|' => text.push_str("\\|"),
                '\\' => text.push_str("\\\\"),
                ' '..='~' => text.push(ch),
                _ => text.extend(ch.escape_default()),
            }
        }
        text.push_str("|\n");
    }
}

pub fn outcome(outcome: &Outcome) -> String {
    let mut text = String::new();
    match outcome {
        Outcome::Created { folder_created } => {
            if *folder_created {
                text.push_str("Created templates/ with permissions 0755.\n");
            }
            text.push_str("Created template file.\n");
        }
        Outcome::Edited(retained) => {
            text.push_str("Saved template file.\n");
            backups(&mut text, retained);
        }
        Outcome::Deleted(removed) => {
            text.push_str("Deleted template file.\n");
            kept(&mut text, removed);
        }
        Outcome::NeedsAttention {
            retained,
            removed,
            folder_created,
            competing_change,
            durability_unknown,
        } => {
            if *folder_created {
                text.push_str("Created templates/ with permissions 0755.\n");
            }
            text.push_str("The template change ran. Inspect the file before retrying.\n");
            if *competing_change {
                text.push_str("Another change overlapped the save. Dort did not undo the save.\n");
            }
            if *durability_unknown {
                text.push_str("Dort could not confirm the save reached disk.\n");
            }
            if let Some(retained) = retained {
                backups(&mut text, retained);
            }
            if let Some(removed) = removed {
                kept(&mut text, removed);
            }
        }
    }
    text
}

fn kept(text: &mut String, removed: &Removed) {
    let _ = writeln!(text, "Retained copy: {}", path(&removed.path));
}

fn backups(text: &mut String, retained: &Retained) {
    let _ = writeln!(
        text,
        "Reviewed copy: {}\nReplaced entry: {}",
        path(&retained.reviewed),
        path(&retained.displaced)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use dort_core::template_files::Folder;
    use std::path::Path;

    #[test]
    fn template_files_review_distinguishes_line_endings_whitespace_and_delimiters() {
        let report = review(Review {
            action: Action::Edit,
            path: Path::new("templates/file.jinja"),
            folder: None,
            before: Some(" \t|\\\r\nlast"),
            after: Some("\u{00a0}\n"),
            permissions: 0o640,
            mappings: 1,
            retained: None,
            removed: None,
        });
        assert!(report.contains("Edit template file: templates/file.jinja\nPermissions: 0640\n"));
        assert!(report.contains("\n| \\t\\|\\\\\\r\\n|\n|last|\n"));
        assert!(report.contains("\n|\\u{a0}\\n|\n"));
    }

    #[test]
    fn template_files_review_names_the_folder_before_the_file() {
        let report = review(Review {
            action: Action::Create,
            path: Path::new("templates/new.jinja"),
            folder: Some(Folder {
                path: "templates".into(),
                permissions: 0o755,
            }),
            before: None,
            after: Some("value"),
            permissions: 0o644,
            mappings: 0,
            retained: None,
            removed: None,
        });
        let folder = report
            .find("Create folder: templates | permissions 0755")
            .unwrap();
        let file = report
            .find("Create template file: templates/new.jinja")
            .unwrap();
        assert!(file < folder, "{report}");
        assert!(report.contains("\n|value|\n"), "{report}");
    }

    #[test]
    fn template_files_delete_review_names_the_retained_copy_and_no_after_text() {
        let report = review(Review {
            action: Action::Delete,
            path: Path::new("templates/old.jinja"),
            folder: None,
            before: Some("gone"),
            after: None,
            permissions: 0o600,
            mappings: 0,
            retained: None,
            removed: Some("/state/removed".into()),
        });
        assert!(report.starts_with("Delete template file: templates/old.jinja\n"));
        assert!(
            report.contains("The file moves to private Dort state."),
            "{report}"
        );
        assert!(
            report.contains("Retained copy: /state/removed\n"),
            "{report}"
        );
        assert!(
            report.contains("\n|gone|\n") && !report.contains("After"),
            "{report}"
        );
    }

    #[test]
    fn template_files_outcome_names_the_folder_and_the_removed_copy() {
        assert_eq!(
            outcome(&Outcome::Created {
                folder_created: true
            }),
            "Created templates/ with permissions 0755.\nCreated template file.\n"
        );
        assert_eq!(
            outcome(&Outcome::Created {
                folder_created: false
            }),
            "Created template file.\n"
        );
        assert_eq!(
            outcome(&Outcome::Deleted(Removed {
                path: "/state/removed".into()
            })),
            "Deleted template file.\nRetained copy: /state/removed\n"
        );
    }

    #[test]
    fn template_files_conflict_reports_the_write_and_both_retained_paths() {
        let report = outcome(&Outcome::NeedsAttention {
            retained: Some(Retained {
                reviewed: "/state/reviewed".into(),
                displaced: "/state/replaced".into(),
            }),
            removed: None,
            folder_created: false,
            competing_change: true,
            durability_unknown: false,
        });
        assert!(report.contains("Inspect the file before retrying."));
        assert!(report.contains("Dort did not undo the save."));
        assert!(
            report.contains("Reviewed copy: /state/reviewed\nReplaced entry: /state/replaced\n")
        );
    }
}
