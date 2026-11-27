use super::components as ui;
use dort_core::{Error, Result, generated::TEMPLATE_LIMIT};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::Line,
    widgets::Paragraph,
};
use std::{borrow::Cow, cell::Cell};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};
use unicode_width::UnicodeWidthStr;

#[derive(Default, Clone, Copy)]
struct Viewport {
    top: usize,
    left: usize,
    rows: usize,
}

pub(super) struct TemplateBuffer {
    text: String,
    cursor: usize,
    preferred_column: Option<usize>,
    view: Cell<Viewport>,
}

pub(super) fn plain_template(text: &str) -> Result<Cow<'_, str>> {
    if text.len() > TEMPLATE_LIMIT {
        return Err(Error::Invalid("Template exceeds 64 KiB.".into()));
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(Error::Invalid("Use plain text for the template.".into()));
    }
    Ok(if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    })
}

impl TemplateBuffer {
    pub fn new(text: String) -> Self {
        Self {
            cursor: text.len(),
            text,
            preferred_column: None,
            view: Cell::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn replace(&mut self, text: String) {
        if text != self.text {
            *self = Self::new(text);
        }
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        let text = plain_template(text)?;
        if text.len() > TEMPLATE_LIMIT.saturating_sub(self.text.len()) {
            return Err(Error::Invalid("Template exceeds 64 KiB.".into()));
        }
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.realign();
        Ok(())
    }

    fn realign(&mut self) {
        let mut cursor = GraphemeCursor::new(self.cursor, self.text.len(), true);
        if let Ok(false) = cursor.is_boundary(&self.text, 0)
            && let Ok(Some(next)) = cursor.next_boundary(&self.text, 0)
        {
            self.cursor = next;
        }
        self.preferred_column = None;
    }

    fn position(&self) -> (usize, usize) {
        let prefix = &self.text[..self.cursor];
        (
            prefix.bytes().filter(|&byte| byte == b'\n').count(),
            cells(prefix.rsplit('\n').next().unwrap_or_default()),
        )
    }

    fn vertical(&mut self, delta: isize) {
        let (row, column) = self.position();
        let column = *self.preferred_column.get_or_insert(column);
        let wanted = row.saturating_add_signed(delta);
        let mut index = 0;
        let mut lines = self.text.split_inclusive('\n');
        let mut line = lines.next().unwrap_or_default();
        for _ in 0..wanted {
            if !line.ends_with('\n') {
                break;
            }
            index += line.len();
            line = lines.next().unwrap_or_default();
        }
        let mut x = 0;
        let line = line
            .strip_suffix("\r\n")
            .or_else(|| line.strip_suffix('\n'))
            .unwrap_or(line);
        for g in line.graphemes(true) {
            x += display(g).width();
            if x > column {
                break;
            }
            index += g.len();
        }
        self.cursor = index;
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.view.get().rows.max(1) as isize;
        let vertical = match key.code {
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            KeyCode::PageUp => -page,
            KeyCode::PageDown => page,
            _ => 0,
        };
        if vertical != 0 {
            self.vertical(vertical);
            return Ok(());
        }
        self.preferred_column = None;
        let prev = || {
            self.text[..self.cursor]
                .grapheme_indices(true)
                .next_back()
                .map(|(i, _)| i)
        };
        let next = || {
            self.text[self.cursor..]
                .graphemes(true)
                .next()
                .map(|g| self.cursor + g.len())
        };
        match key.code {
            KeyCode::Left => self.cursor = prev().unwrap_or(0),
            KeyCode::Right => self.cursor = next().unwrap_or(self.text.len()),
            KeyCode::Home => {
                self.cursor = if ctrl {
                    0
                } else {
                    self.text[..self.cursor]
                        .rfind('\n')
                        .map_or(0, |index| index + 1)
                };
            }
            KeyCode::End => {
                self.cursor = if ctrl {
                    self.text.len()
                } else {
                    self.text[self.cursor..]
                        .find('\n')
                        .map_or(self.text.len(), |index| {
                            let end = self.cursor + index;
                            end - usize::from(
                                self.text.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r'),
                            )
                        })
                };
            }
            KeyCode::Backspace => {
                if let Some(start) = prev() {
                    self.text.drain(start..self.cursor);
                    self.cursor = start;
                    self.realign();
                }
            }
            KeyCode::Delete => {
                if let Some(end) = next() {
                    self.text.drain(self.cursor..end);
                    self.realign();
                }
            }
            KeyCode::Enter => self.paste("\n")?,
            KeyCode::Char('u') if ctrl => self.replace(String::new()),
            KeyCode::Char(c)
                if !c.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.paste(c.encode_utf8(&mut [0; 4]))?;
            }
            _ => (),
        }
        Ok(())
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, focused: bool) {
        if area.is_empty() {
            return;
        }
        let (row, column) = self.position();
        let width = usize::from(area.width);
        let rows = usize::from(area.height);
        let prev = self.view.get();
        let total_rows = self.text.split('\n').count();
        let top = keep_visible(prev.top, row, rows).min(total_rows.saturating_sub(rows));
        let left = keep_visible(prev.left, column, width);
        self.view.set(Viewport { top, left, rows });
        let lines: Vec<_> = self
            .text
            .split('\n')
            .enumerate()
            .skip(top)
            .take(rows)
            .map(|(row, line)| {
                let line = if row + 1 < total_rows {
                    line.strip_suffix('\r').unwrap_or(line)
                } else {
                    line
                };
                Line::raw(visible_cells(line, left, width))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).style(ui::muted()), area);
        if focused {
            frame.set_cursor_position((
                area.x + (column - left) as u16,
                area.y + (row - top) as u16,
            ));
        }
    }
}

fn keep_visible(origin: usize, index: usize, extent: usize) -> usize {
    origin.clamp(index.saturating_sub(extent - 1), index)
}

fn display(grapheme: &str) -> Cow<'_, str> {
    if grapheme == "\t" {
        "    ".into()
    } else if grapheme.chars().any(|c| {
        c.is_control() || matches!(c, '\u{061c}' | '\u{200e}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }) {
        ui::inline(grapheme).into()
    } else {
        grapheme.into()
    }
}

fn cells(line: &str) -> usize {
    line.graphemes(true).map(|g| display(g).width()).sum()
}

fn visible_cells(line: &str, left: usize, width: usize) -> String {
    let mut shown = String::new();
    let mut start = 0;
    for g in line.graphemes(true) {
        let g = display(g);
        let end = start + g.width();
        if end > left {
            if end > left + width {
                break;
            }
            if start >= left {
                shown.push_str(&g);
            } else {
                shown.push_str(&" ".repeat(end - left));
            }
        }
        start = end;
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        Terminal,
        backend::{Backend, TestBackend},
        layout::Position,
    };

    fn key(editor: &mut TemplateBuffer, key: KeyCode) {
        editor.key(KeyEvent::new(key, KeyModifiers::NONE)).unwrap();
    }

    #[test]
    fn edits_at_the_caret_without_splitting_graphemes_or_discarding_line_suffixes() {
        let mut editor = TemplateBuffer::new("αe\u{301}🍏suffix\nx\nab日🍏cd".into());
        editor
            .key(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL))
            .unwrap();
        key(&mut editor, KeyCode::Right);
        key(&mut editor, KeyCode::Delete);
        key(&mut editor, KeyCode::Right);
        key(&mut editor, KeyCode::Enter);
        editor.paste("M\r\nN").unwrap();
        assert_eq!(editor.text(), "α🍏\nM\nNsuffix\nx\nab日🍏cd");
        key(&mut editor, KeyCode::Home);
        key(&mut editor, KeyCode::Backspace);
        key(&mut editor, KeyCode::End);
        key(&mut editor, KeyCode::Left);
        key(&mut editor, KeyCode::Left);
        key(&mut editor, KeyCode::Down);
        key(&mut editor, KeyCode::Down);
        key(&mut editor, KeyCode::Backspace);
        assert_eq!(editor.text(), "α🍏\nMNsuffix\nx\nab日cd");
        let before = editor.text().to_owned();
        assert!(editor.paste("\x1b").is_err());
        assert!(editor.paste(&"x".repeat(TEMPLATE_LIMIT)).is_err());
        assert_eq!(editor.text(), before);
    }

    #[test]
    fn caret_and_text_stay_visible_in_both_directions_in_a_small_viewport() {
        let mut editor = TemplateBuffer::new("head\nsecond\nthird\nfourth\n0123456789".into());
        let mut terminal = Terminal::new(TestBackend::new(6, 2)).unwrap();
        fn draw(
            editor: &TemplateBuffer,
            terminal: &mut Terminal<TestBackend>,
        ) -> (String, Position) {
            terminal
                .draw(|frame| editor.draw(frame, frame.area(), true))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            (text, terminal.backend_mut().get_cursor_position().unwrap())
        }
        assert_eq!(
            draw(&editor, &mut terminal),
            ("h     56789 ".into(), Position::new(5, 1))
        );
        key(&mut editor, KeyCode::Home);
        key(&mut editor, KeyCode::PageUp);
        assert_eq!(
            draw(&editor, &mut terminal),
            ("third fourth".into(), Position::new(0, 0))
        );
        editor
            .key(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(
            draw(&editor, &mut terminal),
            ("head  second".into(), Position::new(0, 0))
        );
        key(&mut editor, KeyCode::End);
        key(&mut editor, KeyCode::Delete);
        assert!(editor.text().starts_with("headsecond\n"));
    }
}
