use ratatui::{
    buffer::{Buffer, CellWidth},
    crossterm::event::KeyCode::{self, *},
    layout::Rect,
    style::{Color, Modifier},
};

use crate::{
    appearance::Appearance,
    model::{self, App, Record, Status},
    render::render,
};

struct Terminal {
    app: App,
    area: Rect,
    appearance: Appearance,
}

impl Terminal {
    fn new(width: u16, height: u16) -> Self {
        Self {
            app: model::demo(),
            area: Rect::new(0, 0, width, height),
            appearance: Appearance::Color,
        }
    }

    fn buffer(&self) -> Buffer {
        let mut buffer = Buffer::empty(self.area);
        render(&mut buffer, self.area, &self.app, self.appearance);
        buffer
    }

    fn lines(&self) -> Vec<String> {
        let buffer = self.buffer();
        (0..self.area.height)
            .map(|y| {
                let mut line = String::new();
                let mut x = 0;
                while x < self.area.width {
                    let cell = &buffer[(x, y)];
                    line.push_str(cell.symbol());
                    x += cell.cell_width().max(1);
                }
                line
            })
            .collect()
    }

    fn text(&self) -> String {
        self.lines().join("\n")
    }

    fn key(&mut self, key: KeyCode) -> &mut Self {
        let mut buf = Buffer::empty(self.area);
        let view = render(&mut buf, self.area, &self.app, self.appearance).unwrap();
        self.app.key(key, view);
        self
    }

    fn type_filter(&mut self, query: &str) -> &mut Self {
        self.key(Char('/'));
        for c in query.chars() {
            self.key(Char(c));
        }
        self
    }
}

#[test]
fn both_panes_fit_at_90_columns_and_keys_follow_focus() {
    let mut terminal = Terminal::new(90, 22);
    let wide = terminal.text();
    assert!(wide.contains("Recovery queue"));
    assert!(wide.contains("Target    ~/.gitconfig"));
    assert!(wide.contains("Enter  Review restore"));
    assert_eq!(wide.matches("[Active]").count(), 1);

    terminal.key(Tab).key(Down).key(Char(' '));
    let focused = terminal.text();
    assert!(focused.contains("› Git configuration"));
    assert!(focused.contains("Provenance"));
    let heading = &terminal.lines()[3];
    assert!(heading.find("Record").unwrap() < heading.find("[Active]").unwrap());

    terminal.area.width = 89;
    assert!(!terminal.text().contains("Recovery queue"));
    assert!(terminal.text().contains("Target    ~/.gitconfig"));
    terminal.key(Esc);
    assert!(terminal.text().contains("Enter  Open details"));
    assert!(!terminal.text().contains("Target"));
    terminal.key(Down);
    assert!(terminal.text().contains("› Git includes"));
    terminal.key(Enter);
    assert!(terminal.text().contains("Target    ~/.gitconfig.local"));
}

#[test]
fn ready_review_requires_distinct_confirmation_and_restores_filter_and_focus() {
    let mut terminal = Terminal::new(100, 22);
    terminal
        .type_filter("Git")
        .key(Enter)
        .key(Tab)
        .key(Enter)
        .key(Enter);
    let review = terminal.text();
    assert!(review.contains("Planned effects"));
    assert!(!review.contains("Recovery queue"));
    assert!(review.contains("Source    Snapshot R-041"));
    assert!(review.contains("Replace   ~/.gitconfig"));
    assert!(review.contains("Back up   ~/.gitconfig.dort-prev"));
    assert!(review.contains("y  Simulate restore"));

    terminal.key(Esc);
    assert!(terminal.text().contains("Tab Queue"));
    assert!(terminal.text().contains("/ Filter   Git"));
    assert!(terminal.text().contains("› Git configuration"));
    assert!(!terminal.text().contains("Restore simulated"));
    terminal.key(Enter).key(Char('y')).key(Char('y')).key(Enter);
    let receipt = terminal.text();
    assert!(receipt.contains("Restore simulated"));
    assert!(receipt.contains("Mock ID   R-041 / attempt 1"));
    assert!(receipt.contains("Writes    None"));

    terminal.key(Esc).key(Enter).key(Char('y'));
    let finished = terminal.text();
    assert!(finished.contains("Status    Finished"));
    assert!(finished.contains("› Git configuration"));
    assert!(finished.contains("/ Filter   Git"));
    assert!(finished.contains("Tab Queue"));
    assert!(!finished.contains("Review restore"));
}

#[test]
fn review_cancel_restores_queue_focus_when_opened_from_browse() {
    let mut terminal = Terminal::new(100, 22);
    terminal.key(Enter).key(Esc).key(Down);
    let queue = terminal.text();
    assert!(queue.contains("Tab Details"));
    assert!(queue.contains("› Git includes"));
    assert!(!queue.contains("Restore simulated"));
}

#[test]
fn blocked_and_previously_finished_never_offer_approval() {
    let mut terminal = Terminal::new(90, 22);
    terminal
        .key(Down)
        .key(Enter)
        .key(Tab)
        .key(Enter)
        .key(Char('y'));
    let blocked = terminal.text();
    assert!(blocked.contains("Status    Blocked"));
    assert!(blocked.contains("Snapshot R-019 has no checksum."));
    assert!(blocked.contains("Capture a verified snapshot."));
    assert!(!blocked.contains("Review restore"));
    assert!(!blocked.contains("Restore simulated"));

    terminal.key(Tab).key(Down).key(Enter).key(Char('y'));
    let finished = terminal.text();
    assert!(finished.contains("Status    Finished"));
    assert!(finished.contains("No further action needed."));
    assert!(!finished.contains("Review restore"));
    assert!(!finished.contains("Restore simulated"));
}

#[test]
fn compact_review_keeps_tail_above_footer_and_disallows_early_confirmation() {
    let mut terminal = Terminal::new(44, 14);
    terminal.key(Down).key(Down).key(Down).key(Enter);
    let first = terminal.text();
    assert!(first.contains("Record"));
    assert!(!first.contains("Recovery queue"));
    assert!(first.contains("Status    Ready"));
    assert!(first.contains("チーム/atelier-e\u{0301}"));
    terminal.key(Down).key(Down);
    assert!(terminal.text().contains("settings.lua"));

    terminal.key(Enter).key(Char('y'));
    assert!(terminal.text().contains("Read remaining effects"));
    assert!(!terminal.text().contains("Simulate restore"));
    let mut backup_tail = false;
    let mut final_effect = false;
    for _ in 0..25 {
        let frame = terminal.lines();
        assert!(frame[11].contains("─────"));
        let body = frame[4..11].join("\n");
        backup_tail |= body.contains("settings.lua.dort-prev");
        final_effect |= body.contains("All other files unchanged.");
        assert!(!frame[11..].join("\n").contains("dort-prev"));
        terminal.key(Down);
    }
    assert!(backup_tail);
    assert!(final_effect);
    assert!(terminal.lines()[12].contains("y  Simulate restore"));
    terminal.key(Char('y')).key(Esc);
    assert!(terminal.text().contains("settings.lua"));
    terminal.key(Up).key(Up);
    assert!(terminal.text().contains("Editor settings"));
    assert!(terminal.text().contains("Status    Finished"));
    assert!(!terminal.text().contains("Review restore"));
}

#[test]
fn long_queue_scrolls_and_filter_draft_does_not_steal_selection() {
    let mut terminal = Terminal::new(44, 14);
    let titles = [
        "1 one", "2 two", "3 three", "4 four", "5 five", "6 six", "7 seven", "8 eight", "9 nine",
    ];
    terminal.app = App::new(
        titles
            .into_iter()
            .map(|title| Record {
                title,
                tool: "Tool",
                target: "~/settings",
                snapshot: "S-1",
                date: "03 Jun 2026",
                status: Status::Ready,
                origin: "Local recovery journal",
            })
            .collect(),
    );
    for _ in 0..8 {
        terminal.key(Down);
    }
    let scrolled = terminal.text();
    assert!(scrolled.contains("5–9 of 9 matches"));
    assert!(scrolled.contains("› 9 nine"));
    assert!(!scrolled.contains("1 one"));

    terminal.type_filter("nomatch").key(Down);
    assert!(terminal.text().contains("No records match this filter."));
    terminal.key(Esc);
    assert!(terminal.text().contains("› 9 nine"));
    terminal
        .type_filter("nomatch")
        .key(Enter)
        .key(Enter)
        .key(Tab);
    assert!(terminal.text().contains("/  Change filter"));
    assert!(!terminal.text().contains("Review restore"));
}

#[test]
fn terminal_cells_escape_control_data_and_keep_unicode_graphemes() {
    let mut terminal = Terminal::new(100, 22);
    terminal.app = App::new(vec![Record {
        title: "名 e\u{0301} 👩\u{200d}💻 <a>",
        target: "~/<x>&\\files\n\r\t\x1b[2J\u{202e}tail",
        tool: "Git",
        snapshot: "S-1",
        date: "03 Jun 2026",
        status: Status::Ready,
        origin: "Local",
    }]);
    let frame = terminal.text();
    assert!(frame.contains("名 e\u{0301} 👩\u{200d}💻 <a>"));
    assert!(frame.contains("~/<x>&\\\\files\\n\\r\\t\\u{1b}[2J\\u{202e}tail"));
    assert!(!frame.contains('\x1b'));
    assert!(!frame.contains('\u{202e}'));
}

#[test]
fn meaning_survives_without_color_and_color_roles_are_distinct() {
    let mut terminal = Terminal::new(100, 22);
    let buffer = terminal.buffer();
    let heading = &buffer[(2, 3)];
    assert_eq!(heading.fg, Color::Rgb(130, 219, 234));
    assert!(heading.modifier.contains(Modifier::BOLD));
    let colors = buffer
        .content()
        .iter()
        .map(|cell| cell.fg)
        .collect::<Vec<_>>();
    assert!(colors.contains(&Color::Rgb(155, 218, 168)));
    assert!(colors.contains(&Color::Rgb(241, 202, 129)));
    assert!(colors.contains(&Color::Rgb(255, 167, 166)));
    terminal.key(Down).key(Tab);
    terminal.appearance = Appearance::Mono;
    let buffer = terminal.buffer();
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
    );
    assert!(buffer[(2, 3)].modifier.contains(Modifier::UNDERLINED));
    let text = terminal.text();
    assert!(text.contains("[Active]"));
    assert!(text.contains("› Git includes"));
    assert!(text.contains("Status    Blocked"));
    assert!(text.contains("Capture a verified snapshot."));
}
