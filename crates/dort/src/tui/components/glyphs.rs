//! Trusted single-cell decoration only. Runtime metadata never passes through here.
pub const SELECTED: &str = "›";
pub const MENU: &str = "▾";
pub const EXPAND: &str = "▸";
pub const COMPLETE: &str = "✓";
pub const FAILED: &str = "×";
pub const PENDING: &str = "·";
pub const SEPARATOR: &str = "·";
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const ASCII_SPINNER: [&str; 4] = ["|", "/", "-", "\\"];

pub fn ascii(symbol: &str) -> Option<&'static str> {
    match symbol {
        "┌" | "┐" | "└" | "┘" | "├" | "┤" | "┬" | "┴" | "┼" | "╭" | "╮" | "╰" | "╯" => {
            Some("+")
        }
        "─" => Some("-"),
        "│" | "·" => Some("|"),
        "▾" => Some("v"),
        "▸" | "›" => Some(">"),
        "✓" => Some("+"),
        "×" => Some("x"),
        "█" => Some("#"),
        "░" => Some("-"),
        symbol if SPINNER.contains(&symbol) => Some("|"),
        _ => None,
    }
}
