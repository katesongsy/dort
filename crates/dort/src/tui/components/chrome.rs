//! One shared shell: location, optional destination rail, working area and one strip.
use super::{self as ui, design, glyphs};
use crate::tui::{
    app::Tab,
    input::{Hits, Target},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

pub struct Areas {
    pub body: Rect,
    pub footer: Rect,
}
pub fn shell(
    frame: &mut Frame,
    title: &'static str,
    tab: Tab,
    dry_run: bool,
    blocked: bool,
    hits: &Hits,
) -> Areas {
    let area = frame.area();
    let suffix = if dry_run { " [DRY RUN]" } else { "" };
    let title = format!(
        "{}{}",
        ui::chrome_clip(
            &format!("Dort / {title}"),
            area.width.saturating_sub(suffix.len() as u16),
        ),
        suffix,
    );
    frame.render_widget(
        Paragraph::new(title).style(ui::heading()),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let footer = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let mut body = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(2),
    );
    if area.width >= 100 {
        let rail = Rect::new(body.x, body.y, 16, body.height);
        let choices = crate::tui::navigation::SECTION_ORDER
            .into_iter()
            .filter_map(crate::tui::navigation::section_tab);
        for (index, destination) in choices.enumerate() {
            let label = crate::tui::navigation::section_hint(destination);
            if index as u16 >= rail.height {
                break;
            }
            let row = Rect::new(rail.x, rail.y + index as u16, rail.width, 1);
            let active = destination == tab;
            let line = Line::from(vec![
                Span::styled(
                    if active {
                        format!("{} ", glyphs::EXPAND)
                    } else {
                        "  ".into()
                    },
                    ui::accent(),
                ),
                Span::raw(label),
            ]);
            frame.render_widget(
                Paragraph::new(line).style(if active {
                    design::selection()
                } else {
                    ui::muted()
                }),
                row,
            );
            hits.click(
                row,
                if blocked {
                    Target::NavigationBlocked
                } else {
                    Target::Destination(destination)
                },
            );
        }
        body.x += rail.width + 1;
        body.width = body.width.saturating_sub(rail.width + 1);
    }
    Areas { body, footer }
}
