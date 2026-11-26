//! Pure terminal capabilities and a geometry-preserving rendering adaptation.
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier, Style},
};

#[derive(Clone, Copy)]
pub(super) enum TextRole {
    Heading,
    Label,
    Value,
    Note,
    Success,
    Warning,
    Blocker,
}

impl TextRole {
    pub(super) fn style(self) -> Style {
        let (color, emphasis) = match self {
            Self::Heading => (Some(Color::Cyan), Modifier::BOLD),
            Self::Label => (None, Modifier::BOLD),
            Self::Value => (None, Modifier::empty()),
            Self::Note => (Some(Color::DarkGray), Modifier::empty()),
            Self::Success => (Some(Color::Green), Modifier::empty()),
            Self::Warning => (Some(Color::Yellow), Modifier::BOLD),
            Self::Blocker => (Some(Color::Red), Modifier::BOLD),
        };
        let style = Style::default().add_modifier(emphasis);
        match color {
            Some(color) => style.fg(color),
            None => style,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
enum ColorDepth {
    #[default]
    Direct,
    Indexed,
    Basic,
}

impl ColorDepth {
    fn map(self, color: Color) -> Color {
        match (self, color) {
            (Self::Indexed, Color::Rgb(r, g, b)) => indexed([r, g, b]),
            (Self::Basic, Color::Rgb(..)) => Color::Reset,
            _ => color,
        }
    }
}

fn indexed(rgb: [u8; 3]) -> Color {
    let levels: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let cube = rgb.map(|channel| {
        (0..levels.len())
            .min_by_key(|&index| channel.abs_diff(levels[index]))
            .unwrap()
    });
    let average = (rgb.into_iter().map(u16::from).sum::<u16>() + 1) / 3;
    let gray = ((average.saturating_sub(8) + 5) / 10).min(23) as u8;
    let distance = |candidate: [u8; 3]| {
        rgb.into_iter()
            .zip(candidate)
            .map(|(a, b)| (i32::from(a) - i32::from(b)).pow(2))
            .sum::<i32>()
    };
    if distance([8 + 10 * gray; 3]) < distance(cube.map(|index| levels[index])) {
        Color::Indexed(232 + gray)
    } else {
        let [r, g, b] = cube;
        Color::Indexed((16 + 36 * r + 6 * g + b) as u8)
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Appearance {
    ascii: bool,
    monochrome: bool,
    color_depth: ColorDepth,
}

impl Appearance {
    pub(crate) fn classify(
        term: Option<&str>,
        lc_all: Option<&str>,
        lc_ctype: Option<&str>,
        lang: Option<&str>,
        no_color: Option<&str>,
    ) -> Self {
        let term = term.unwrap_or_default().to_ascii_lowercase();
        let locale = [lc_all, lc_ctype, lang]
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
            .unwrap_or_default()
            .to_ascii_uppercase();
        Self {
            ascii: !(locale.contains("UTF-8") || locale.contains("UTF8"))
                || matches!(term.as_str(), "vt100" | "vt102" | "ansi" | "dumb"),
            monochrome: no_color.is_some_and(|value| !value.is_empty())
                || matches!(term.as_str(), "vt100" | "vt102" | "dumb" | "xterm-mono"),
            ..Self::default()
        }
    }

    pub(crate) fn with_color_depth(mut self, term: Option<&str>, colorterm: Option<&str>) -> Self {
        let term = term.unwrap_or_default().to_ascii_lowercase();
        let colorterm = colorterm.unwrap_or_default().to_ascii_lowercase();
        self.color_depth = if matches!(colorterm.as_str(), "truecolor" | "24bit")
            || term.contains("-direct")
            || term.contains("truecolor")
        {
            ColorDepth::Direct
        } else if term.contains("256color") {
            ColorDepth::Indexed
        } else {
            ColorDepth::Basic
        };
        self
    }

    pub(super) fn ascii(self) -> bool {
        self.ascii
    }

    pub(super) fn apply(self, buffer: &mut Buffer) {
        let heading = TextRole::Heading.style();
        let success = TextRole::Success.style();
        for cell in &mut buffer.content {
            if self.ascii {
                // Only trusted, single-cell decoration. Unknown symbols are not
                // rewritten: metadata escaping belongs to its existing boundary.
                let replacement = super::components::glyphs::ascii(cell.symbol());
                if let Some(symbol) = replacement {
                    cell.set_symbol(symbol);
                }
            }
            if self.monochrome {
                // ANSI cyan + bold is reserved for headings and focus accents.
                if Some(cell.fg) == heading.fg && cell.modifier.contains(heading.add_modifier) {
                    cell.modifier.insert(Modifier::UNDERLINED);
                } else if Some(cell.fg) == success.fg {
                    cell.modifier.insert(Modifier::BOLD);
                }
                cell.fg = Color::Reset;
                cell.bg = Color::Reset;
                cell.modifier.remove(Modifier::DIM);
            } else if self.color_depth != ColorDepth::Direct {
                cell.fg = self.color_depth.map(cell.fg);
                cell.bg = self.color_depth.map(cell.bg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_precedence_and_independent_capabilities() {
        let classify = |all, ctype, lang, term, no_color| {
            Appearance::classify(term, all, ctype, lang, no_color)
        };
        assert_eq!(
            classify(None, None, Some("en_US.uTf8"), Some("xterm"), None),
            Appearance::default()
        );
        assert_eq!(
            classify(Some(""), Some("C.UTF-8"), Some("C"), None, Some("")),
            Appearance::default()
        );
        assert!(classify(Some("C"), Some("C.UTF-8"), Some("en_US.UTF-8"), None, None).ascii);
        assert!(classify(None, Some("POSIX"), Some("C.UTF-8"), None, None).ascii);
        assert!(classify(None, None, None, None, None).ascii);
        let utf8 = |term, no_color| classify(None, None, Some("C.UTF-8"), Some(term), no_color);
        assert_eq!(
            utf8("xterm", Some("0")),
            Appearance {
                monochrome: true,
                ..Appearance::default()
            }
        );
        assert_eq!(
            utf8("ANSI", None),
            Appearance {
                ascii: true,
                ..Appearance::default()
            }
        );
        for term in ["vt100", "VT102", "dumb"] {
            assert_eq!(
                utf8(term, None),
                Appearance {
                    ascii: true,
                    monochrome: true,
                    ..Appearance::default()
                }
            );
        }
        assert_eq!(
            utf8("xterm-mono", None),
            Appearance {
                monochrome: true,
                ..Appearance::default()
            }
        );
    }

    #[test]
    fn adaptation_changes_only_known_decoration_and_colors() {
        use ratatui::layout::Rect;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        let symbols = [
            "┌",
            "┐",
            "└",
            "┘",
            "├",
            "┤",
            "┬",
            "┴",
            "┼",
            "─",
            "│",
            "·",
            "é",
            "x",
            "\\",
            "+",
            crate::tui::components::glyphs::MENU,
            crate::tui::components::glyphs::EXPAND,
        ];
        for (cell, symbol) in buffer.content.iter_mut().zip(symbols) {
            cell.set_symbol(symbol);
            cell.fg = Color::Cyan;
            cell.bg = Color::Black;
            cell.modifier =
                Modifier::REVERSED | Modifier::BOLD | Modifier::UNDERLINED | Modifier::DIM;
        }
        Appearance {
            ascii: true,
            monochrome: true,
            ..Appearance::default()
        }
        .apply(&mut buffer);
        for (cell, expected) in buffer.content.iter().zip([
            "+", "+", "+", "+", "+", "+", "+", "+", "+", "-", "|", "|", "é", "x", "\\", "+", "v",
            ">",
        ]) {
            assert_eq!(cell.symbol(), expected);
            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
            assert_eq!(
                cell.modifier,
                Modifier::REVERSED | Modifier::BOLD | Modifier::UNDERLINED
            );
        }
    }
}
