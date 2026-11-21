//! Session menu navigation over typed actions.
use super::{
    app::{App, Tab},
    components as ui,
    input::Target,
    shell::Action,
};
use ratatui::{Frame, layout::Rect};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Action(Action),
    Manifest,
}
#[derive(Clone, Copy)]
pub struct Entry {
    pub label: &'static str,
    pub item: Item,
}
fn action(label: &'static str, action: Action) -> Entry {
    Entry {
        label,
        item: Item::Action(action),
    }
}
pub const GROUPS: [&str; 7] = [
    "Session", "Overview", "Files", "Recovery", "Checks", "Git", "Tools",
];
/// Stable internal IDs are separate from presentation order; Menu and rail share this order.
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
pub const SECTION_HINTS: [&str; 7] = [
    "Session",
    "Overview o",
    "Files f",
    "Recovery r",
    "Checks c",
    "Git g",
    "Tools t",
];
pub fn section_shortcut(key: char) -> Option<usize> {
    match key {
        'o' => Some(1),
        'f' => Some(2),
        'r' => Some(3),
        'c' => Some(4),
        'g' => Some(5),
        't' => Some(6),
        _ => None,
    }
}
pub fn section_hint(tab: Tab) -> &'static str {
    SECTION_HINTS[index(tab) + 1]
}
/// Numbers share one catalog between pages, Menu and keyboard dispatch.
pub fn local_catalog(group: usize) -> &'static [(usize, &'static str)] {
    match group {
        2 => &[(0, "1 Managed"), (1, "2 Discover"), (2, "3 Ignored paths")],
        4 => &[(0, "1 Manifest checks"), (1, "2 Diagnostics")],
        5 => &[(0, "1 Status"), (1, "2 Unstaged"), (2, "3 Staged")],
        6 => &[(0, "1 Declared"), (1, "2 Installed"), (2, "3 History")],
        _ => &[],
    }
}
/// Numbered destinations retain the same order on pages and in Menu; tasks follow.
pub fn entry_order(group: usize) -> Vec<usize> {
    let mut order: Vec<_> = local_catalog(group)
        .iter()
        .map(|(index, _)| *index)
        .collect();
    for index in 0..entries(group).len() {
        if !order.contains(&index) {
            order.push(index);
        }
    }
    order
}
pub fn entries(group: usize) -> Vec<Entry> {
    use Action::*;
    match group {
        0 => vec![
            action("Choose repository [p]", Choose),
            action("Create/attach [n]", Init),
            action("Preview-only mode [v]", PreviewOnly),
            action("Help [F1]", Help),
            action("About [a]", About),
            action("Quit [q]", Quit),
        ],
        1 => vec![action("Overview [o]", Destination(Tab::Overview))],
        2 => vec![
            action("Managed [1]", Destination(Tab::Files)),
            action("Discover [2]", Discover),
            action("Ignored paths [3]", ManageIgnored),
        ],
        3 => vec![action("All repositories [r]", Destination(Tab::Recovery))],
        4 => vec![
            Entry {
                label: "Manifest checks [1]",
                item: Item::Manifest,
            },
            action("Diagnostics [2]", Diagnostics),
        ],
        5 => vec![
            action("Status [1]", GitStatus),
            action("Unstaged [2]", GitUnstaged),
            action("Staged [3]", GitStaged),
            action("Choose Git scope [p]", GitRoot),
            action("Follow selected repository [b]", GitDefault),
        ],
        _ => vec![
            action("Declared tools [1]", Destination(Tab::Tools)),
            action("Installed packages [2]", ToolsInstalled),
            action("Installation history [3]", ToolAttempts),
        ],
    }
}
/// Item shortcuts apply inside their open menu, independently of workspace keys.
pub fn shortcut(item: Item) -> ratatui::crossterm::event::KeyCode {
    use Action::*;
    use ratatui::crossterm::event::KeyCode;
    KeyCode::Char(match item {
        Item::Manifest => '1',
        Item::Action(action) => match action {
            Choose => 'p',
            Init => 'n',
            PreviewOnly => 'v',
            Help => return KeyCode::F(1),
            About => 'a',
            Quit => 'q',
            Destination(Tab::Overview) => 'o',
            Destination(Tab::Files) => '1',
            Destination(Tab::Recovery) => 'r',
            Destination(Tab::Tools) => '1',
            ToolDeclaration => 'a',
            ToolsConfigure => 's',
            ToolsInstalled => '2',
            ToolAttempts => '3',
            ToolsCheck => 'C',
            ToolsInstallMissing => 'i',
            Discover => '2',
            ManageIgnored => '3',
            Diagnostics => '2',
            GitStatus => '1',
            GitUnstaged => '2',
            GitStaged => '3',
            GitRoot => 'p',
            GitDefault => 'b',
            _ => unreachable!("navigation items have explicit shortcuts"),
        },
    })
}
pub fn owns(action: Action) -> bool {
    matches!(action, Action::Destination(_))
        || (0..GROUPS.len())
            .flat_map(entries)
            .any(|e| e.item == Item::Action(action))
}
#[derive(Clone, Copy)]
pub struct Menu {
    pub group: usize,
    pub selected: usize,
    pub content_focus: bool,
    pub content_scroll: u16,
}
#[derive(Default)]
pub struct State {
    pub menu: Option<Menu>,
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
    pub(super) fn menus_available(&self) -> bool {
        self.navigation_block().is_none()
    }
    pub(super) fn open_menu(&mut self, selected_group: usize, _width: u16) {
        if let Some(reason) = self.navigation_block() {
            self.interaction_notice = Some(reason);
            self.status = reason.into();
            return;
        }
        self.interaction_notice = None;
        if self.git_view.sensitive() {
            self.git_view.discard_patch();
        }
        let (content_focus, content_scroll) = self
            .navigation
            .menu
            .map(|m| (m.content_focus, m.content_scroll))
            .unwrap_or((self.content.focused, self.content.scroll));
        self.navigation.menu = Some(Menu {
            group: selected_group.min(GROUPS.len() - 1),
            selected: current_entry(self, selected_group),
            content_focus,
            content_scroll,
        });
        super::discover_content::sync(self, self.hits.size().0, self.hits.size().1);
    }
    pub(super) fn menu_item(&mut self, item: Item) -> Option<super::actions::Task> {
        if let Item::Action(action) = item
            && let Some(reason) = self.action_reason(action)
        {
            self.status = reason.into();
            return None;
        }
        self.navigation.menu = None;
        self.leave_page_for_navigation();
        match item {
            Item::Manifest => {
                self.go_tab(Tab::Checks);
                self.showing_diagnostics = false;
                None
            }
            Item::Action(action) => self.activate(action),
        }
    }
    pub(super) fn menu_key(
        &mut self,
        key: ratatui::crossterm::event::KeyCode,
        _width: u16,
    ) -> Option<super::actions::Task> {
        use ratatui::crossterm::event::KeyCode;
        let menu = self.navigation.menu?;
        if let KeyCode::Char(digit @ '1'..='9') = key
            && let Some(&(index, _)) = local_catalog(menu.group).get((digit as u8 - b'1') as usize)
        {
            return self.menu_item(entries(menu.group)[index].item);
        }
        if let Some(entry) = entries(menu.group)
            .into_iter()
            .find(|e| shortcut(e.item) == key)
        {
            return self.menu_item(entry.item);
        }
        let menu = self.navigation.menu.as_mut()?;
        let order = entry_order(menu.group);
        let position = order
            .iter()
            .position(|index| *index == menu.selected)
            .unwrap_or(0);
        match key {
            KeyCode::Esc => self.navigation.menu = None,
            KeyCode::Up | KeyCode::Down => {
                let position = SECTION_ORDER
                    .iter()
                    .position(|group| *group == menu.group)
                    .unwrap_or(0);
                menu.group = SECTION_ORDER[(position
                    + if key == KeyCode::Up {
                        SECTION_ORDER.len() - 1
                    } else {
                        1
                    })
                    % SECTION_ORDER.len()];
                menu.selected = 0;
            }
            KeyCode::Left => menu.selected = order[position.saturating_sub(1)],
            KeyCode::Right => menu.selected = order[(position + 1).min(order.len() - 1)],
            KeyCode::Home => menu.selected = order[0],
            KeyCode::End => menu.selected = order[order.len() - 1],
            KeyCode::Enter => {
                let item = entries(menu.group).get(menu.selected)?.item;
                return self.menu_item(item);
            }
            _ => (),
        }
        None
    }
}
pub fn popup(frame: &mut Frame, app: &App, menu: Menu) {
    let items = entries(menu.group);
    let current = items.get(menu.selected);
    let reason = current
        .and_then(|entry| match entry.item {
            Item::Action(action) => app.action_reason(action),
            Item::Manifest => None,
        })
        .or(app.interaction_notice);
    let sections: Vec<_> = SECTION_ORDER
        .iter()
        .copied()
        .map(|group| ui::navigation::Section {
            label: SECTION_HINTS[group],
            target: Target::MenuGroup(group),
            selected: group == menu.group,
            destinations: entry_order(group)
                .into_iter()
                .map(|index| {
                    let entry = entries(group)[index];
                    ui::navigation::Destination {
                        label: local_catalog(group)
                            .iter()
                            .find(|(id, _)| *id == index)
                            .map_or(label_only(entry.label), |(_, label)| label),
                        target: if group == menu.group {
                            Target::MenuItem(index)
                        } else {
                            Target::MenuDestination(group, index)
                        },
                        selected: group == menu.group && index == menu.selected,
                    }
                })
                .collect(),
        })
        .collect();
    ui::navigation::menu(
        frame,
        &sections,
        current.map_or("", |entry| entry.label),
        reason,
        &app.hits,
    );
}

fn label_only(label: &'static str) -> &'static str {
    label.split(" [").next().unwrap_or(label)
}
fn current_entry(app: &App, group: usize) -> usize {
    let item = match app.tab {
        Tab::Files
            if matches!(
                app.shell.overlay,
                Some(super::shell::Overlay::Ignored { .. })
            ) =>
        {
            Item::Action(Action::ManageIgnored)
        }
        Tab::Files if app.discovering => Item::Action(Action::Discover),
        Tab::Checks if app.showing_diagnostics => Item::Action(Action::Diagnostics),
        Tab::Checks => Item::Manifest,
        Tab::Git => Item::Action(match app.git_view.comparison {
            super::git::Comparison::Status => Action::GitStatus,
            super::git::Comparison::Unstaged => Action::GitUnstaged,
            super::git::Comparison::Staged => Action::GitStaged,
        }),
        Tab::Tools if app.tools_view.installed_open => Item::Action(Action::ToolsInstalled),
        Tab::Tools if app.tools_view.install_open => Item::Action(Action::ToolAttempts),
        tab => Item::Action(Action::Destination(tab)),
    };
    entries(group)
        .iter()
        .position(|entry| entry.item == item)
        .unwrap_or(0)
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
        || app.page_title().is_some()
            && !ignored
            && !(app.tab == Tab::Tools && app.tools_view.collection_visible())
        || app.git_view.sensitive()
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
    let current = current_entry(app, group);
    let items: Vec<_> = local_catalog(group)
        .iter()
        .map(|&(index, label)| ui::navigation::Destination {
            label,
            target: Target::LocalDestination(group, index),
            selected: current == index,
        })
        .collect();
    if items.len() < 2 {
        return area;
    }
    ui::navigation::local(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        &items,
        Target::MenuGroup(group),
        &app.hits,
    );
    Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    )
}
