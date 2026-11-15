//! Read-only metrics and a scrollable fact document. No workflow or approval state.
use super::{clip, inline, pane, report};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Text,
    widgets::Paragraph,
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
            let widget = Paragraph::new(clip(
                &inline(if cards {
                    &metric.value
                } else {
                    &metric.compact
                }),
                area.width,
            ))
            .style(metric.style);
            if cards {
                frame.render_widget(widget.block(pane(metric.label, false)), *area);
            } else {
                frame.render_widget(widget, *area);
            }
        }
    }
    report(frame, rows[1], title, body, scroll, true);
}
