use ratatui::layout::Rect;

use super::input::{Hits, Target};

const QUEUE_MIN: u16 = 38;
const READER_MIN: u16 = 50;
const GUTTER: u16 = 2;
const PANE_MIN_HEIGHT: u16 = 10;
pub const SPLIT_MIN_WIDTH: u16 = QUEUE_MIN + GUTTER + READER_MIN;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Queue,
    Details,
}
impl Region {
    pub fn from_details(details: bool) -> Self {
        if details { Self::Details } else { Self::Queue }
    }
    pub fn other(self) -> Self {
        match self {
            Self::Queue => Self::Details,
            Self::Details => Self::Queue,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Recovery,
    Files(super::inventory::Page),
    Git,
    DeclaredTools,
    InstalledTools,
    InstallAttempts,
    Ignored,
    Repositories,
    Profiles,
}

#[derive(Clone, Copy)]
pub struct Workspace {
    pub scope: Scope,
    pub active: Region,
    pub queue: Option<Rect>,
    pub details: Option<Rect>,
}
impl Workspace {
    pub fn new(scope: Scope, body: Rect, active: Region, paired: bool) -> Self {
        let (queue, details) = if paired && let Some((queue, details)) = split(body) {
            (Some(queue), Some(details))
        } else if active == Region::Details {
            (None, Some(body))
        } else {
            (Some(body), None)
        };
        Self {
            scope,
            active,
            queue,
            details,
        }
    }

    pub fn queue_content(self, frame: &mut ratatui::Frame, title: &str) -> Option<Rect> {
        let area = self.queue?;
        let block = super::components::pane(title, self.active == Region::Queue);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        Some(inner)
    }

    pub fn is_split(self) -> bool {
        self.queue.is_some() && self.details.is_some()
    }

    pub fn bind(self, hits: &Hits) {
        hits.bind_workspace(self);
        if self.is_split() {
            for (region, area) in [(Region::Queue, self.queue), (Region::Details, self.details)] {
                if let Some(area) = area {
                    hits.click(area, Target::Workspace(self.scope, region));
                }
            }
        }
    }

    pub fn region_at(self, x: u16, y: u16) -> Option<Region> {
        let contains = |area: Option<Rect>| {
            area.is_some_and(|r| r.contains(ratatui::layout::Position::new(x, y)))
        };
        if contains(self.queue) {
            Some(Region::Queue)
        } else if contains(self.details) {
            Some(Region::Details)
        } else {
            None
        }
    }
}

fn split(area: Rect) -> Option<(Rect, Rect)> {
    if area.width < SPLIT_MIN_WIDTH || area.height < PANE_MIN_HEIGHT {
        return None;
    }
    let width = (area.width / 3).clamp(QUEUE_MIN, area.width - GUTTER - READER_MIN);
    let queue = Rect { width, ..area };
    let details = Rect::new(
        queue.right() + GUTTER,
        area.y,
        area.width - width - GUTTER,
        area.height,
    );
    Some((queue, details))
}
