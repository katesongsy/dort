//! Render-owned hit regions. Never derive requests from displayed text.
use super::{app::Tab, shell::Action};
use ratatui::layout::{Position, Rect};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy)]
pub enum Target {
    Feedback(super::task_surfaces::Control),
    TreeFold(usize),
    TreeMark(usize),
    BatchControl(usize, bool),
    Action(Action),
    Destination(Tab),
    Row(usize),
    Filter,
    FormControl(usize, bool),
    Picker(usize, bool),
    Palette(Action),
    PaletteSearch,
    Back,
    Continue,
    PreviewRow(usize),
}
#[derive(Clone, Copy)]
pub enum Wheel {
    Rows,
    Choices,
    Details,
    Overlay,
    Preview,
    PreviewRows,
    Confirmation,
    GitPrivate,
}
struct Region {
    area: Rect,
    click: Option<Target>,
    wheel: Option<Wheel>,
}
#[derive(Default)]
pub struct Hits {
    regions: RefCell<Vec<Region>>,
    size: Cell<(u16, u16)>,
}
impl Hits {
    pub fn begin(&self, area: Rect) {
        self.clear();
        self.size.set((area.width, area.height));
    }
    pub fn size(&self) -> (u16, u16) {
        self.size.get()
    }
    pub fn clear(&self) {
        self.regions.borrow_mut().clear();
    }
    pub fn click(&self, area: Rect, target: Target) {
        self.regions.borrow_mut().push(Region {
            area,
            click: Some(target),
            wheel: None,
        });
    }
    pub fn wheel(&self, area: Rect, target: Wheel) {
        self.regions.borrow_mut().push(Region {
            area,
            click: None,
            wheel: Some(target),
        });
    }
    pub fn target(&self, x: u16, y: u16) -> Option<Target> {
        self.regions.borrow().iter().rev().find_map(|r| {
            r.area
                .contains(Position::new(x, y))
                .then_some(r.click)
                .flatten()
        })
    }
    pub fn scroll(&self, x: u16, y: u16) -> Option<Wheel> {
        self.regions.borrow().iter().rev().find_map(|r| {
            r.area
                .contains(Position::new(x, y))
                .then_some(r.wheel)
                .flatten()
        })
    }
    pub fn rows(&self, area: Rect, visible: &[usize], offset: usize) {
        let rows = Rect::new(
            area.x + 1,
            area.y + 2,
            area.width.saturating_sub(2),
            area.height.saturating_sub(3),
        );
        self.wheel(area, Wheel::Rows);
        for (y, &index) in visible
            .iter()
            .skip(offset)
            .take(usize::from(rows.height))
            .enumerate()
        {
            self.click(
                Rect::new(rows.x, rows.y + y as u16, rows.width, 1),
                Target::Row(index),
            );
        }
    }
}
