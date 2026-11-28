use super::presentation::inline;
use dort_core::template_files::{Outcome, Retained, Review, Row, Source};
use std::fmt::Write;

pub fn list(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "No template files. Add a .jinja file under templates/.\n".into();
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
    let action = if change.before.is_some() {
        "Edit"
    } else {
        "Create"
    };
    let mut text = format!(
        "{action} template file: {}\nPermissions: {:04o}\nMappings declaring this path: {}\nMappings and Home files stay unchanged.\n",
        path(change.path),
        change.permissions,
        change.mappings,
    );
    if let Some(retained) = change.retained {
        backups(&mut text, &retained);
    }
    if let Some(before) = change.before {
        contents(&mut text, "Before", before);
    }
    contents(&mut text, "After", change.after);
    text
}

pub fn show(source: &Source) -> String {
    let mut text = format!("{} | {:04o}\n", summary(source.row()), source.permissions());
    contents(&mut text, "Contents", source.text());
    text
}

fn contents(text: &mut String, label: &str, body: &str) {
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
        Outcome::Created => text.push_str("Created template file.\n"),
        Outcome::Edited(retained) => {
            text.push_str("Saved template file.\n");
            backups(&mut text, retained);
        }
        Outcome::NeedsAttention {
            retained,
            competing_change,
            durability_unknown,
        } => {
            text.push_str("The template write ran. Inspect the file before retrying.\n");
            if *competing_change {
                text.push_str("Another change overlapped the save. Dort did not undo the save.\n");
            }
            if *durability_unknown {
                text.push_str("Dort could not confirm the save reached disk.\n");
            }
            if let Some(retained) = retained {
                backups(&mut text, retained);
            }
        }
    }
    text
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
    use std::path::Path;

    #[test]
    fn template_files_review_distinguishes_line_endings_whitespace_and_delimiters() {
        let report = review(Review {
            path: Path::new("templates/file.jinja"),
            before: Some(" \t|\\\r\nlast"),
            after: "\u{00a0}\n",
            permissions: 0o640,
            mappings: 1,
            retained: None,
        });
        assert!(report.contains("Edit template file: templates/file.jinja\nPermissions: 0640\n"));
        assert!(report.contains("\n| \\t\\|\\\\\\r\\n|\n|last|\n"));
        assert!(report.contains("\n|\\u{a0}\\n|\n"));
    }

    #[test]
    fn template_files_conflict_reports_the_write_and_both_retained_paths() {
        let report = outcome(&Outcome::NeedsAttention {
            retained: Some(Retained {
                reviewed: "/state/reviewed".into(),
                displaced: "/state/replaced".into(),
            }),
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
