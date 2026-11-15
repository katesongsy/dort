//! A metadata-only reading document, not another set of active controls.
use super::{heading, metadata};
use ratatui::text::{Line, Text};
pub fn document(
    title: &'static str,
    purpose: &'static str,
    keys: &[(&str, &str)],
    sections: &[(&str, String)],
) -> Text<'static> {
    let mut lines = vec![
        Line::styled(title, heading()),
        Line::raw(purpose),
        Line::default(),
        Line::styled("Keys in this context", heading()),
    ];
    lines.extend(
        keys.iter()
            .map(|(key, description)| metadata(key, description)),
    );
    for (title, body) in sections {
        lines.push(Line::default());
        lines.push(Line::styled(super::inline(title), heading()));
        lines.extend(body.lines().map(|line| Line::raw(super::inline(line))));
    }
    Text::from(lines)
}
