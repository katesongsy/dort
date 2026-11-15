//! Session-local menu navigation. Typed menu items request existing guarded actions.
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
            action("Managed [m]", Destination(Tab::Files)),
            action("Discover [f]", Discover),
            action("Ignored paths [U]", ManageIgnored),
        ],
        3 => vec![action("All repositories [r]", Destination(Tab::Recovery))],
        4 => vec![
            Entry {
                label: "Manifest checks [m]",
                item: Item::Manifest,
            },
            action("Diagnostics [j]", Diagnostics),
        ],
        5 => vec![
            action("Status [z]", GitStatus),
            action("Unstaged [u]", GitUnstaged),
            action("Staged [s]", GitStaged),
            action("Choose Git scope [g]", GitRoot),
            action("Follow selected repository [f]", GitDefault),
        ],
        _ => vec![
            action("Tools [t]", Destination(Tab::Tools)),
            action("Add tool declaration [a]", ToolDeclaration),
            action("Inspect installed tools [c]", ToolsConfigure),
            action("Session snapshots [s]", ToolsSnapshots),
            action("Installation attempts [i]", ToolAttempts),
            action("Reconcile snapshots [p]", ToolsReconcile),
        ],
    }
}
/// Item shortcuts apply inside their open menu, independently of workspace keys.
pub fn shortcut(item: Item) -> ratatui::crossterm::event::KeyCode {
    use Action::*;
    use ratatui::crossterm::event::KeyCode;
    KeyCode::Char(match item {
        Item::Manifest => 'm',
        Item::Action(action) => match action {
            Choose => 'p',
            Init => 'n',
            PreviewOnly => 'v',
            Help => return KeyCode::F(1),
            About => 'a',
            Quit => 'q',
            Destination(Tab::Overview) => 'o',
            Destination(Tab::Files) => 'm',
            Destination(Tab::Recovery) => 'r',
            Destination(Tab::Tools) => 't',
            ToolDeclaration => 'a',
            ToolsConfigure => 'c',
            ToolsSnapshots => 's',
            ToolAttempts => 'i',
            ToolsReconcile => 'p',
            Discover => 'f',
            ManageIgnored => 'U',
            Diagnostics => 'j',
            GitStatus => 'z',
            GitUnstaged => 'u',
            GitStaged => 's',
            GitRoot => 'g',
            GitDefault => 'f',
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
    pub expanded: bool,
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
        Tab::Tools if app.tools_view.install_open => match app.tools_view.installation.mode {
            super::tools_install::Mode::Attempts => "Tools / Installation attempts",
            super::tools_install::Mode::CloseInputs
            | super::tools_install::Mode::LoadingClose
            | super::tools_install::Mode::CloseReview => "Tools / Close attempt",
            super::tools_install::Mode::Result => "Tools / Installation outcome",
            _ => "Tools / Package installation",
        },
        Tab::Tools => "Tools / Inspection and comparison",
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
            self.tools_view.leave();
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
            selected: 0,
            expanded: false,
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
        if menu.expanded
            && let Some(entry) = entries(menu.group)
                .into_iter()
                .find(|e| shortcut(e.item) == key)
        {
            return self.menu_item(entry.item);
        }
        let menu = self.navigation.menu.as_mut()?;
        match key {
            KeyCode::Esc => self.navigation.menu = None,
            KeyCode::Left if menu.expanded => {
                menu.expanded = false;
                menu.selected = 0;
            }
            KeyCode::Left | KeyCode::Right => {
                menu.group = (menu.group
                    + if key == KeyCode::Left {
                        GROUPS.len() - 1
                    } else {
                        1
                    })
                    % GROUPS.len();
                menu.selected = 0;
            }
            KeyCode::Up if !menu.expanded => {
                menu.group = (menu.group + GROUPS.len() - 1) % GROUPS.len()
            }
            KeyCode::Down if !menu.expanded => menu.group = (menu.group + 1) % GROUPS.len(),
            KeyCode::Up => menu.selected = menu.selected.saturating_sub(1),
            KeyCode::Down => menu.selected = (menu.selected + 1).min(entries(menu.group).len() - 1),
            KeyCode::Home if !menu.expanded => menu.group = 0,
            KeyCode::End if !menu.expanded => menu.group = GROUPS.len() - 1,
            KeyCode::Home => menu.selected = 0,
            KeyCode::End => menu.selected = entries(menu.group).len() - 1,
            KeyCode::Enter if !menu.expanded => {
                menu.expanded = true;
                menu.selected = 0;
            }
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
    let area = ui::design::choice_dialog(frame, "Places", &app.hits);
    let items = entries(menu.group);
    let reason = if menu.expanded {
        items.get(menu.selected).and_then(|entry| match entry.item {
            Item::Action(action) => app.action_reason(action),
            Item::Manifest => None,
        })
    } else {
        None
    }
    .or(app.interaction_notice);
    let extra = if reason.is_some() { 2 } else { 0 };
    let body = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(extra),
    );
    let selected = if menu.expanded {
        menu.selected
    } else {
        menu.group
    };
    let capacity = ui::design::pair_capacity(body).max(1);
    let offset = selected.saturating_sub(capacity - 1);
    let labels: Vec<_> = if menu.expanded {
        items
            .iter()
            .enumerate()
            .map(|(id, entry)| {
                let label = entry
                    .label
                    .split(" [")
                    .next()
                    .unwrap_or(entry.label)
                    .to_owned();
                let key = match shortcut(entry.item) {
                    ratatui::crossterm::event::KeyCode::Char(c) => c.to_string(),
                    ratatui::crossterm::event::KeyCode::F(n) => format!("F{n}"),
                    _ => String::new(),
                };
                (id, label, key)
            })
            .collect()
    } else {
        GROUPS
            .iter()
            .enumerate()
            .map(|(id, label)| (id, (*label).to_owned(), String::new()))
            .collect()
    };
    let rows: Vec<_> = labels
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|(identity, label, key)| ui::design::PairRow {
            identity: *identity,
            values: [label, key],
        })
        .collect();
    ui::design::pairs_with_disclosure(
        frame,
        body,
        ui::design::PairTable {
            headers: [
                if menu.expanded {
                    GROUPS[menu.group]
                } else {
                    "PLACE"
                },
                "KEY",
            ],
            rows: &rows,
            selected,
        },
        &app.hits,
        |index| {
            if menu.expanded {
                Target::MenuItem(index)
            } else {
                Target::MenuGroup(index)
            }
        },
        !menu.expanded,
    );
    if let Some(reason) = reason {
        ui::design::notice(
            frame,
            Rect::new(
                area.x,
                area.bottom().saturating_sub(extra),
                area.width,
                extra,
            ),
            "Unavailable",
            reason,
            ui::design::Tone::Attention,
        );
    }
}
