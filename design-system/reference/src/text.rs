use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn visible(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{061c}'
            | '\u{200e}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}' => {
                escaped.extend(c.escape_unicode());
            }
            _ if c.is_control() => escaped.extend(c.escape_unicode()),
            _ => escaped.push(c),
        }
    }
    escaped
        .graphemes(true)
        .fold(String::new(), |mut out, grapheme| {
            if grapheme.cell_width() == 0 {
                for c in grapheme.chars() {
                    out.extend(c.escape_unicode());
                }
            } else {
                out.push_str(grapheme);
            }
            out
        })
}

pub(crate) fn width(text: &str) -> usize {
    text.graphemes(true)
        .map(|part| usize::from(part.cell_width()))
        .sum()
}

pub(crate) fn wrap(mut text: &str, limit: usize) -> Vec<&str> {
    let mut lines = Vec::new();
    while !text.is_empty() {
        let mut cells = 0;
        let mut fit = 0;
        let mut boundary = 0;
        for (index, part) in text.grapheme_indices(true) {
            cells += usize::from(part.cell_width());
            if cells > limit && fit != 0 {
                break;
            }
            fit = index + part.len();
            if matches!(part, " " | "/") {
                boundary = fit;
            }
        }
        let end = if fit == text.len() || boundary == 0 {
            fit
        } else {
            boundary
        };
        let (line, rest) = text.split_at(end);
        lines.push(line);
        text = rest;
    }
    lines
}

pub(crate) fn clip(text: &str, limit: usize) -> String {
    if width(text) <= limit {
        return text.to_owned();
    }
    let mut remaining = limit.saturating_sub(1);
    let mut clipped = String::new();
    for part in text.graphemes(true) {
        if let Some(left) = remaining.checked_sub(usize::from(part.cell_width())) {
            remaining = left;
            clipped.push_str(part);
        } else {
            break;
        }
    }
    if limit > 0 {
        clipped.push('…');
    }
    clipped
}
