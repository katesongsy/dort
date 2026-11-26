use super::{app::App, components as ui, input::Wheel};
use dort_core::generated::{DestinationStatus, Preview};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
};
use ui::{
    design::Tone,
    reader::{ReviewScroll, ReviewStage, field, section},
};

pub struct Report {
    document: Text<'static>,
    pub scroll: ReviewScroll,
}

impl Report {
    pub fn new(preview: &Preview) -> Self {
        let tone = match preview.destination_status {
            DestinationStatus::Blocked => Tone::Error,
            DestinationStatus::Existing => Tone::Attention,
            DestinationStatus::Missing => Tone::Neutral,
        };
        let mut lines = vec![
            section(&preview.id),
            Line::raw("Read-only preview. No files written."),
            field(
                "Inputs from dort.toml",
                &crate::services::presentation::generated_inputs(preview),
                Tone::Neutral,
            ),
            Line::default(),
            field(
                "Source",
                &preview.source.display().to_string(),
                Tone::Neutral,
            ),
            field(
                "Destination",
                &preview.destination.display().to_string(),
                Tone::Neutral,
            ),
            field(
                "Destination status",
                preview.destination_status.label(),
                tone,
            ),
        ];
        if let Some(mode) = &preview.permissions {
            lines.push(field("Declared permissions", mode, Tone::Neutral));
        }
        lines.extend([Line::default(), section("Rendered output")]);
        if preview.rendered.is_empty() {
            lines.push(Line::styled("Empty output", ui::muted()));
        } else {
            lines.extend(
                super::views::safe(&preview.rendered)
                    .split('\n')
                    .map(|line| Line::raw(line.to_owned())),
            );
        }
        lines.push(Line::styled("End of rendered output", ui::muted()));
        Self {
            document: Text::from(lines),
            scroll: ReviewScroll::default(),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, app: &App) {
        ui::reader::Reader::new(&app.hits)
            .with_wheel(Wheel::Overlay)
            .review(
                frame,
                area,
                None,
                &self.document,
                ReviewStage::Generated,
                &self.scroll,
            );
    }
}
