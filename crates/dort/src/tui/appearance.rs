//! Pure terminal capabilities and a geometry-preserving rendering adaptation.
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Appearance {
    ascii: bool,
    monochrome: bool,
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
        }
    }

    pub(super) fn apply(self, buffer: &mut Buffer) {
        for cell in &mut buffer.content {
            if self.ascii {
                // Only trusted, single-cell decoration. Unknown symbols are not
                // rewritten: metadata escaping belongs to its existing boundary.
                let replacement = match cell.symbol() {
                    "┌" | "┐" | "└" | "┘" | "├" | "┤" | "┬" | "┴" | "┼" => {
                        Some("+")
                    }
                    "─" => Some("-"),
                    "│" | "·" => Some("|"),
                    _ => None,
                };
                if let Some(symbol) = replacement {
                    cell.set_symbol(symbol);
                }
            }
            if self.monochrome {
                cell.fg = Color::Reset;
                cell.bg = Color::Reset;
                cell.modifier.remove(Modifier::DIM);
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
                ascii: false,
                monochrome: true
            }
        );
        assert_eq!(
            utf8("ANSI", None),
            Appearance {
                ascii: true,
                monochrome: false
            }
        );
        for term in ["vt100", "VT102", "dumb"] {
            assert_eq!(
                utf8(term, None),
                Appearance {
                    ascii: true,
                    monochrome: true
                }
            );
        }
        assert_eq!(
            utf8("xterm-mono", None),
            Appearance {
                ascii: false,
                monochrome: true
            }
        );
    }

    #[test]
    fn adaptation_changes_only_known_decoration_and_colors() {
        use ratatui::layout::Rect;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        let symbols = [
            "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼", "─", "│", "·", "é", "x", "\\", "+",
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
        }
        .apply(&mut buffer);
        for (cell, expected) in buffer.content.iter().zip([
            "+", "+", "+", "+", "+", "+", "+", "+", "+", "-", "|", "|", "é", "x", "\\", "+",
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
