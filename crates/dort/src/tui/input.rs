//! Render-owned hit regions. Never derive requests from displayed text.
use super::shell::Action;
use ratatui::layout::{Position, Rect};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy)]
pub enum Target {
    Feedback(super::task_surfaces::Control),
    TreeFold(usize),
    TreeMark(usize),
    BatchControl(usize, bool),
    BatchEdit,
    BatchDetails,
    DiscoverFocus(bool),
    Action(Action),
    MenuOpen,
    NavigationBlocked,
    Key(ratatui::crossterm::event::KeyEvent),
    More,
    MoreRow(usize),
    ChoiceRow(usize),
    Commands,
    Destination(super::app::Tab),
    MenuGroup(usize),
    MenuItem(usize),
    MenuDestination(usize, usize),
    LocalDestination(usize, usize),
    Row(usize),
    Filter,
    FormControl(usize, bool),
    FormAction(super::forms::Command),
    PatchConfirm,
    ConfirmAction,
    CancelConfirmation,
    ToolInstallControl(usize, bool),
    ToolInstallProvider,
    ToolAttemptRow(usize),
    ToolsControl(usize, bool),
    ToolsRow(usize),
    ToolDeclarationControl(usize, bool),
    Picker(usize, bool),
    PickerDetails,
    Palette(Action),
    PaletteSearch,
    Back,
    Continue,
    PreviewRow(usize),
}
#[derive(Clone, Copy)]
pub enum Wheel {
    Content,
    Rows,
    Choices,
    Details,
    Overlay,
    Preview,
    PreviewRows,
    Confirmation,
    GitPrivate,
    Tools,
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
    input_focus: Cell<bool>,
    actions: RefCell<Vec<super::components::ActionButton>>,
}
impl Hits {
    pub fn begin(&self, area: Rect) {
        self.clear();
        self.size.set((area.width, area.height));
        self.input_focus.set(true);
    }
    pub fn input_focus_allowed(&self) -> bool {
        self.input_focus.get()
    }
    pub fn input_focus(&self, allowed: bool) {
        self.input_focus.set(allowed);
    }
    pub fn size(&self) -> (u16, u16) {
        self.size.get()
    }
    pub fn clear(&self) {
        self.regions.borrow_mut().clear();
        self.actions.borrow_mut().clear();
    }
    pub fn present(&self, mut action: super::components::ActionButton) {
        action.key = super::navigation::context_key(action.key);
        if !self
            .actions
            .borrow()
            .iter()
            .any(|a| a.label == action.label && a.key == action.key)
        {
            self.actions.borrow_mut().push(action);
        }
    }
    pub fn actions(&self) -> Vec<super::components::ActionButton> {
        self.actions.borrow().clone()
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
            area.x,
            area.y + 2,
            area.width,
            area.height.saturating_sub(2),
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
