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

const RAIL_WIDTH: u16 = 16;
const RAIL_GAP: u16 = 1;

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
    let title_start = "Dort / ".len()
        + title
            .rsplit_once(" / ")
            .map_or(0, |(section, _)| section.len() + " / ".len());
    let title = ui::chrome_clip(
        &format!("Dort / {title}"),
        area.width.saturating_sub(suffix.len() as u16),
    );
    let (brand, location) = title.split_at("Dort".len().min(title.len()));
    let (location, title) = location.split_at((title_start - brand.len()).min(location.len()));
    let title = Line::from(vec![
        Span::styled(brand, design::brand()),
        Span::styled(location, design::muted_chrome()),
        Span::styled(title, ui::heading()),
        Span::styled(suffix, ui::heading()),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(design::chrome()),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let footer = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let mut body = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(2),
    );
    if area.width >= crate::tui::workspace::SPLIT_MIN_WIDTH + RAIL_WIDTH + RAIL_GAP {
        let rail = Rect::new(body.x, body.y, RAIL_WIDTH, body.height);
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
            let (marker, style) = if active {
                let style = design::active_rail();
                (
                    Span::styled(format!("{} ", glyphs::EXPAND), design::focus_marker(style)),
                    style,
                )
            } else {
                (Span::raw("  "), ui::muted())
            };
            let line = Line::from(vec![marker, Span::raw(label)]);
            frame.render_widget(Paragraph::new(line).style(style), row);
            hits.click(
                row,
                if blocked {
                    Target::NavigationBlocked
                } else {
                    Target::Destination(destination)
                },
            );
        }
        body.x += rail.width + RAIL_GAP;
        body.width = body.width.saturating_sub(rail.width + RAIL_GAP);
    }
    Areas { body, footer }
}
