//! Render-owned hit regions.
use super::shell::Action;
use ratatui::layout::{Position, Rect};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy)]
pub enum Target {
    Workspace(super::workspace::Scope, super::workspace::Region),
    Feedback(super::task_surfaces::Control),
    TreeFold(usize),
    TreeMark(usize),
    BatchControl(usize, bool),
    BatchEdit,
    BatchDetails,
    DiscoverFocus(bool),
    Action(Action),
    NavigationBlocked,
    DismissDialog,
    Key(ratatui::crossterm::event::KeyEvent),
    More,
    MoreRow(usize),
    ChoiceRow(usize),
    Destination(super::app::Tab),
    LocalDestination(usize, usize),
    Row(usize),
    Filter,
    FormControl(usize, bool),
    FormAction(super::forms::Command),
    ConfirmAction,
    CancelConfirmation,
    ToolInstallControl(usize, bool),
    ToolInstallProvider,
    ToolAttemptRow(usize),
    ToolsControl(usize, bool),
    ToolsRow(usize),
    ToolDeclarationField(usize),
    ToolDeclarationReview,
    Picker(usize, bool),
    PickerDetails,
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
    GitPatch,
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
    workspace: Cell<Option<super::workspace::Workspace>>,
    actions: RefCell<Vec<super::components::ActionButton>>,
}
impl Hits {
    pub fn begin(&self, area: Rect) {
        self.clear();
        self.workspace.set(None);
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
    pub fn invalidate_workspace(&self) {
        self.workspace.set(None);
    }
    pub fn bind_workspace(&self, workspace: super::workspace::Workspace) {
        self.workspace.set(Some(workspace));
    }
    pub fn workspace(&self, size: (u16, u16)) -> Option<super::workspace::Workspace> {
        (self.size() == size)
            .then(|| self.workspace.get())
            .flatten()
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
}
