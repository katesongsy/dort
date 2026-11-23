//! Read-only metrics and a scrollable fact document. No workflow or approval state.
use super::{clip, design::titled_block, heading, inline, report};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Text},
    widgets::{Borders, Paragraph},
};
pub struct Metric {
    pub label: &'static str,
    pub value: String,
    pub compact: String,
    pub style: Style,
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    metrics: &[Metric],
    body: Text<'static>,
    scroll: u16,
) {
    let cards = area.height >= 9;
    let rows = Layout::vertical([
        Constraint::Length(if cards { 2 } else { 1 }),
        Constraint::Min(3),
    ])
    .split(area);
    if !metrics.is_empty() {
        let columns = Layout::horizontal(vec![
            Constraint::Ratio(1, metrics.len() as u32);
            metrics.len()
        ])
        .split(rows[0]);
        for (area, metric) in columns.iter().zip(metrics) {
            let value = if cards {
                &metric.value
            } else {
                &metric.compact
            };
            let widget =
                Paragraph::new(Line::styled(clip(&inline(value), area.width), metric.style));
            if cards {
                let label = titled_block(metric.label, Borders::TOP, heading().patch(metric.style));
                frame.render_widget(widget.block(label), *area);
            } else {
                frame.render_widget(widget, *area);
            }
        }
    }
    report(frame, rows[1], title, body, scroll, true);
}
