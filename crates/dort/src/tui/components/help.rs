use super::{accent, heading, note, reader};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
};

pub struct Section {
    pub label: &'static str,
    pub text: &'static str,
}

pub struct Guide {
    pub title: &'static str,
    pub intro: &'static str,
    pub sections: &'static [Section],
}

fn paragraph(text: &str, width: u16, lines: &mut Vec<Line<'static>>) {
    let mut row = String::new();
    for word in text.split_whitespace() {
        if !row.is_empty() && row.len() + 1 + word.len() > usize::from(width) {
            lines.push(Line::raw(std::mem::take(&mut row)));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    lines.push(Line::raw(row));
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    guide: &'static Guide,
    offset: usize,
) -> reader::VisibleRows {
    let inset = u16::from(area.width >= 4);
    let mut body = Rect::new(area.x + inset, area.y, area.width - 2 * inset, area.height);
    let mut lines = vec![Line::styled(guide.title, accent())];
    paragraph(guide.intro, body.width, &mut lines);
    for section in guide.sections {
        lines.push(Line::default());
        lines.push(Line::styled(section.label, heading()));
        paragraph(section.text, body.width, &mut lines);
    }
    let has_progress = body.height > 1 && lines.len() > usize::from(body.height);
    if has_progress {
        body.height -= 1;
    }
    let rows = reader::text_view_at(frame, body, Text::from(lines), offset);
    if has_progress {
        let height = usize::from(body.height);
        let progress = format!(
            "Lines {}-{} of {}  PgUp/PgDn",
            rows.start + 1,
            rows.start + height,
            rows.end + height
        );
        frame.render_widget(
            Line::styled(progress, note()),
            Rect::new(body.x, body.bottom(), body.width, 1),
        );
    }
    rows
}
