//! Read-only Help document.
use super::{design, heading, inline};
use ratatui::text::{Line, Span, Text};

fn wrap(text: &str, width: usize) -> impl Iterator<Item = String> + use<> {
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        if !row.is_empty() && row.len() + 1 + word.len() > width {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    rows.push(row);
    rows.into_iter()
}

pub fn document(
    width: u16,
    title: &'static str,
    purpose: &'static str,
    keys: &[(&str, &str)],
    sections: &[(&str, String)],
) -> Text<'static> {
    let width = usize::from(width);
    let mut lines = vec![Line::styled(title, heading())];
    lines.extend(wrap(purpose, width).map(Line::raw));
    lines.push(Line::default());
    let section = design::table_heading();
    lines.push(Line::styled("Keys in this context", section));
    lines.push(Line::default());
    let key_width = keys.iter().map(|(key, _)| key.len()).max().unwrap_or(0) + 4;
    let key_style = design::primary();
    for (key, description) in keys {
        let keycap = format!(" {} ", inline(key));
        let indent = if width.saturating_sub(key_width) >= 20 {
            key_width
        } else {
            2
        };
        let mut rows = wrap(&inline(description), width.saturating_sub(indent));
        if indent == key_width {
            let gap = " ".repeat(indent.saturating_sub(keycap.len()));
            lines.push(Line::from(vec![
                Span::styled(keycap, key_style),
                Span::raw(gap),
                Span::raw(rows.next().unwrap_or_default()),
            ]));
        } else {
            lines.push(Line::styled(keycap, key_style));
        }
        lines.extend(rows.map(|row| Line::raw(format!("{:indent$}{row}", ""))));
    }
    for (title, body) in sections {
        lines.push(Line::default());
        lines.push(Line::styled(inline(title), section));
        lines.push(Line::default());
        lines.extend(
            body.lines()
                .flat_map(|line| wrap(&inline(line), width))
                .map(Line::raw),
        );
    }
    Text::from(lines)
}
