//! Trusted single-cell decoration. Runtime text stays out.
pub const SELECTED: &str = "›";
pub const MENU: &str = "▾";
pub const EXPAND: &str = "▸";
pub const COMPLETE: &str = "✓";
pub const FAILED: &str = "×";
pub const PENDING: &str = "·";
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
        symbol if symbol == EXPAND || symbol == SELECTED => Some(">"),
        "✓" => Some("+"),
        "×" => Some("x"),
        "█" => Some("#"),
        "░" => Some("-"),
        symbol if SPINNER.contains(&symbol) => Some("|"),
        _ => None,
    }
}
