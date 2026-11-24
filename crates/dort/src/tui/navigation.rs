//! Section destinations and local page catalogs.
use super::{
    app::{App, Tab},
    components as ui,
    input::Target,
    shell::Action,
};
use ratatui::{Frame, layout::Rect};

#[derive(Default)]
pub struct ListViewport {
    first: std::cell::Cell<usize>,
    capacity: std::cell::Cell<usize>,
}
impl ListViewport {
    pub fn start(&self, len: usize, capacity: usize, selected: Option<usize>) -> usize {
        self.capacity.set(capacity);
        let mut first = self.first.get().min(len.saturating_sub(capacity));
        if let Some(selected) = selected.filter(|&row| row < len && capacity > 0) {
            if selected < first {
                first = selected;
            } else if selected - first >= capacity {
                first = selected - (capacity - 1);
            }
        }
        self.first.set(first);
        first
    }
    #[cfg(test)]
    pub fn first(&self) -> usize {
        self.first.get()
    }
    pub fn page(&self) -> usize {
        self.capacity.get().max(1)
    }
}

/// Stable internal IDs are separate from presentation order; rail and keys share this order.
pub const SECTION_ORDER: [usize; 7] = [1, 2, 6, 3, 4, 5, 0];
pub fn section_tab(group: usize) -> Option<Tab> {
    match group {
        1 => Some(Tab::Overview),
        2 => Some(Tab::Files),
        6 => Some(Tab::Tools),
        3 => Some(Tab::Recovery),
        4 => Some(Tab::Checks),
        5 => Some(Tab::Git),
        _ => None,
    }
}
pub fn context_key(key: &'static str) -> &'static str {
    match key {
        "o" => "O",
        "f" => "F",
        "t" => "T",
        "r" => "R",
        "c" => "C",
        "g" => "P",
        _ => key,
    }
}
pub const SECTION_HINTS: [&str; 6] = [
    "Overview o",
    "Files f",
    "Recovery r",
    "Checks c",
    "Git g",
    "Tools t",
];
/// A section key opens its first page, matching the rail and digit keys.
pub fn section_shortcut(key: char) -> Option<Place> {
    let tab = match key {
        'o' => Tab::Overview,
        'f' => Tab::Files,
        'r' => Tab::Recovery,
        'c' => Tab::Checks,
        'g' => Tab::Git,
        't' => Tab::Tools,
        _ => return None,
    };
    Some(section_place(tab))
}
pub fn section_place(tab: Tab) -> Place {
    match tab {
        Tab::Checks => Place::Manifest,
        Tab::Git => Place::Action(Action::GitStatus),
        tab => Place::Destination(tab),
    }
}
pub fn section_hint(tab: Tab) -> &'static str {
    SECTION_HINTS[index(tab)]
}
/// One page or operation reachable from the destination rail and its digit key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Destination(Tab),
    Manifest,
    Action(Action),
}
/// The rail and digit keys share one catalog per section.
pub fn local_catalog(group: usize) -> &'static [(&'static str, Place)] {
    match group {
        2 => &[
            ("1 Managed", Place::Destination(Tab::Files)),
            ("2 Discover", Place::Action(Action::Discover)),
            ("3 Ignored paths", Place::Action(Action::ManageIgnored)),
        ],
        4 => &[
            ("1 Manifest checks", Place::Manifest),
            ("2 Diagnostics", Place::Action(Action::Diagnostics)),
        ],
        5 => &[
            ("1 Status", Place::Action(Action::GitStatus)),
            ("2 Unstaged", Place::Action(Action::GitUnstaged)),
            ("3 Staged", Place::Action(Action::GitStaged)),
        ],
        6 => &[
            ("1 Declared", Place::Destination(Tab::Tools)),
            ("2 Installed", Place::Action(Action::ToolsInstalled)),
            ("3 History", Place::Action(Action::ToolAttempts)),
        ],
        _ => &[],
    }
}
#[derive(Default)]
pub struct State {
    selected: [usize; 6],
    scroll: [u16; 6],
}
fn index(tab: Tab) -> usize {
    match tab {
        Tab::Overview => 0,
        Tab::Files => 1,
        Tab::Recovery => 2,
        Tab::Checks => 3,
        Tab::Git => 4,
        Tab::Tools => 5,
    }
}
pub fn group(app: &App) -> usize {
    index(app.tab) + 1
}
pub fn location(app: &App) -> &'static str {
    match app.tab {
        Tab::Overview => "Overview",
        Tab::Tools if app.tools_view.installed_open => "Tools / Installed packages",
        Tab::Tools if app.tools_view.install_open => match app.tools_view.installation.mode {
            super::tools_install::Mode::Attempts => "Tools / Installation history",
            super::tools_install::Mode::CloseInputs
            | super::tools_install::Mode::LoadingClose
            | super::tools_install::Mode::CloseReview => "Tools / Close record",
        },
        Tab::Tools => "Tools / Declared tools",
        Tab::Files if app.discovering => "Files / Discover",
        Tab::Files => "Files / Managed",
        Tab::Recovery => "Recovery / All repositories",
        Tab::Checks if app.showing_diagnostics => "Checks / Diagnostics",
        Tab::Checks => "Checks / Manifest",
        Tab::Git => match app.git_view.comparison {
            super::git::Comparison::Status => "Git / Status",
            super::git::Comparison::Unstaged => "Git / Unstaged",
            super::git::Comparison::Staged => "Git / Staged",
        },
    }
}
impl App {
    pub(super) fn go_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.form_focus = None;
            self.tools_view.leave_section();
            self.navigation.selected[index(self.tab)] = self.selected;
            self.navigation.scroll[index(self.tab)] = self.details_scroll;
            self.selected = self.navigation.selected[index(tab)];
            self.details_scroll = self.navigation.scroll[index(tab)];
            self.tab = tab;
        }
    }
    #[cfg(test)]
    pub(super) fn navigation_available(&self) -> bool {
        self.navigation_block().is_none()
    }
    pub(super) fn go_place(&mut self, place: Place) -> Option<super::actions::Task> {
        if let Place::Action(action) = place
            && let Some(reason) = self.action_reason(action)
        {
            self.status = reason.into();
            return None;
        }
        self.leave_page_for_navigation();
        match place {
            Place::Manifest => {
                self.go_tab(Tab::Checks);
                self.showing_diagnostics = false;
                None
            }
            Place::Destination(tab) => self.activate(Action::Destination(tab)),
            Place::Action(action) => self.activate(action),
        }
    }
}

fn current(app: &App, group: usize) -> Place {
    let place = match app.tab {
        Tab::Files
            if matches!(
                app.shell.overlay,
                Some(super::shell::Overlay::Ignored { .. })
            ) =>
        {
            Place::Action(Action::ManageIgnored)
        }
        Tab::Files if app.discovering => Place::Action(Action::Discover),
        Tab::Checks if app.showing_diagnostics => Place::Action(Action::Diagnostics),
        Tab::Checks => Place::Manifest,
        Tab::Git => Place::Action(match app.git_view.comparison {
            super::git::Comparison::Status => Action::GitStatus,
            super::git::Comparison::Unstaged => Action::GitUnstaged,
            super::git::Comparison::Staged => Action::GitStaged,
        }),
        Tab::Tools if app.tools_view.installed_open => Place::Action(Action::ToolsInstalled),
        Tab::Tools if app.tools_view.install_open => Place::Action(Action::ToolAttempts),
        tab => Place::Destination(tab),
    };
    if local_catalog(group).iter().any(|(_, item)| *item == place) {
        place
    } else {
        local_catalog(group)
            .first()
            .map_or(place, |(_, item)| *item)
    }
}

/// Stable destinations only: forms, reviews and readers keep their own Back path.
pub fn local_available(app: &App) -> bool {
    use super::shell::Overlay;
    let ignored = matches!(app.shell.overlay, Some(Overlay::Ignored { .. }));
    if app.busy
        || app.error.is_some()
        || app.preview.is_some()
        || app.show_result
        || app.confirmation.is_some()
        || app.shell.overlay.is_some() && !ignored
        || app.tab == Tab::Git && app.git_view.editing_root
        || app.page_title().is_some()
            && !ignored
            && !matches!(app.tab, Tab::Files | Tab::Git)
            && !(app.tab == Tab::Tools && app.tools_view.collection_visible())
    {
        return false;
    }
    !local_catalog(group(app)).is_empty()
}
pub fn local(frame: &mut Frame, area: Rect, app: &App) -> Rect {
    if !local_available(app) {
        return area;
    }
    let group = group(app);
    let current = current(app, group);
    let items: Vec<_> = local_catalog(group)
        .iter()
        .enumerate()
        .map(|(index, (label, place))| ui::navigation::Destination {
            label,
            target: Target::LocalDestination(group, index),
            selected: current == *place,
        })
        .collect();
    if items.len() < 2 {
        return area;
    }
    ui::navigation::local(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        &items,
        None,
        &app.hits,
    );
    Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    )
}
