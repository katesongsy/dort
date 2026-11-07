//! Session-local menu navigation. Typed menu items request existing guarded actions.
use super::{
    app::{App, Tab},
    components as ui,
    input::Target,
    shell::Action,
};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph},
};

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
pub const GROUPS: [&str; 6] = ["Session", "Overview", "Files", "Recovery", "Checks", "Git"];
pub fn entries(group: usize) -> Vec<Entry> {
    use Action::*;
    match group {
        0 => vec![
            action("Choose repository [p]", Choose),
            action("Create/attach [n]", Init),
            action("Preview-only mode", PreviewOnly),
            action("Help [F1]", Help),
            action("About", About),
            action("Quit [q]", Quit),
        ],
        1 => vec![action("Overview [1]", Destination(Tab::Overview))],
        2 => vec![
            action("Managed [2]", Destination(Tab::Files)),
            action("Discover [f]", Discover),
            action("Ignored paths [U]", ManageIgnored),
        ],
        3 => vec![action("All repositories [3]", Destination(Tab::Recovery))],
        4 => vec![
            Entry {
                label: "Manifest checks [4, m]",
                item: Item::Manifest,
            },
            action("Diagnostics [j]", Diagnostics),
        ],
        _ => vec![
            action("Status [z]", GitStatus),
            action("Unstaged [u]", GitUnstaged),
            action("Staged [s]", GitStaged),
            action("Choose Git scope [g]", GitRoot),
            action("Follow selected repository", GitDefault),
        ],
    }
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
    selected: [usize; 5],
    scroll: [u16; 5],
}
fn index(tab: Tab) -> usize {
    match tab {
        Tab::Overview => 0,
        Tab::Files => 1,
        Tab::Recovery => 2,
        Tab::Checks => 3,
        Tab::Git => 4,
    }
}
pub fn group(app: &App) -> usize {
    index(app.tab) + 1
}
pub fn location(app: &App) -> &'static str {
    match app.tab {
        Tab::Overview => "Overview",
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
            self.navigation.selected[index(self.tab)] = self.selected;
            self.navigation.scroll[index(self.tab)] = self.details_scroll;
            self.selected = self.navigation.selected[index(tab)];
            self.details_scroll = self.navigation.scroll[index(tab)];
            self.tab = tab;
        }
    }
    pub(super) fn menus_available(&self) -> bool {
        !self.busy
            && self.error.is_none()
            && !self.show_result
            && self.preview.is_none()
            && self.confirmation.is_none()
            && self.shell.overlay.is_none()
            && !self.batch_editor.as_ref().is_some_and(|e| e.open)
            && !(self.tab == Tab::Git && self.git_view.navigation_input())
    }
    pub(super) fn open_menu(&mut self, selected_group: usize, width: u16) {
        if !self.menus_available() {
            return;
        }
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
            expanded: width >= 80,
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
        width: u16,
    ) -> Option<super::actions::Task> {
        use ratatui::crossterm::event::KeyCode;
        let menu = self.navigation.menu.as_mut()?;
        match key {
            KeyCode::Esc | KeyCode::F(10) => self.navigation.menu = None,
            KeyCode::Left if width < 80 && menu.expanded => {
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
            KeyCode::Up | KeyCode::Char('k') if !menu.expanded => {
                menu.group = (menu.group + GROUPS.len() - 1) % GROUPS.len()
            }
            KeyCode::Down | KeyCode::Char('j') if !menu.expanded => {
                menu.group = (menu.group + 1) % GROUPS.len()
            }
            KeyCode::Up | KeyCode::Char('k') => menu.selected = menu.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                menu.selected = (menu.selected + 1).min(entries(menu.group).len() - 1)
            }
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
pub fn bar(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 80 {
        let label = if app.menus_available() {
            format!(" [F10 Menu]  {}", location(app))
        } else {
            " [Menu unavailable during this interaction]".into()
        };
        ui::prose(
            frame,
            Rect::new(area.x, area.y, area.width, 1),
            &label,
            ui::accent(),
        );
        if app.menus_available() {
            app.hits.click(
                Rect::new(area.x, area.y, 12.min(area.width), 1),
                Target::MenuOpen,
            );
        }
        return;
    }
    let mut x = area.x;
    for (i, name) in GROUPS.iter().enumerate() {
        let label = format!(" {name} v ");
        let width = label.chars().count() as u16;
        let rect = Rect::new(x, area.y, width.min(area.right().saturating_sub(x)), 1);
        let selected = app.navigation.menu.is_some_and(|m| m.group == i)
            || (app.navigation.menu.is_none() && group(app) == i);
        frame.render_widget(
            Paragraph::new(label).style(if selected {
                ui::selected()
            } else {
                ui::muted()
            }),
            rect,
        );
        if app.menus_available() {
            app.hits.click(rect, Target::MenuGroup(i));
        }
        x += width;
    }
    ui::prose(
        frame,
        Rect::new(x, area.y, area.right().saturating_sub(x), 1),
        " F10 Menu",
        ui::muted(),
    );
}
pub fn popup(frame: &mut Frame, app: &App, menu: Menu) {
    let area = frame.area();
    app.hits.clear();
    let bar_area = Rect::new(area.x, area.y + 1, area.width, 1);
    bar(frame, bar_area, app);
    let compact = area.width < 80;
    let items = entries(menu.group);
    let labels: Vec<String> = if menu.expanded {
        items
            .iter()
            .map(|e| {
                if e.item == Item::Action(Action::PreviewOnly) {
                    format!("{}: {}", e.label, if app.dry_run { "ON" } else { "OFF" })
                } else {
                    e.label.to_owned()
                }
            })
            .collect()
    } else {
        GROUPS.iter().map(|g| format!("{g} >")).collect()
    };
    let width = (labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16 + 4)
        .max(28)
        .min(area.width);
    let x = if compact {
        area.x
    } else {
        area.x
            + GROUPS[..menu.group]
                .iter()
                .map(|s| s.len() as u16 + 5)
                .sum::<u16>()
    }
    .min(area.right().saturating_sub(width));
    let rect = Rect::new(
        x,
        area.y + 2,
        width,
        (labels.len() as u16 + 4).min(area.height.saturating_sub(2)),
    );
    frame.render_widget(Clear, rect);
    let title = if menu.expanded {
        GROUPS[menu.group]
    } else {
        "Navigation"
    };
    let block = ui::pane(title, true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    for (i, label) in labels
        .iter()
        .enumerate()
        .take(inner.height.saturating_sub(2) as usize)
    {
        let selected = if menu.expanded {
            menu.selected == i
        } else {
            menu.group == i
        };
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let reason = if menu.expanded {
            match items[i].item {
                Item::Action(a) => app.action_reason(a),
                Item::Manifest => None,
            }
        } else {
            None
        };
        let style = if selected {
            ui::selected()
        } else if reason.is_some() {
            ui::muted()
        } else {
            ui::accent()
        };
        frame.render_widget(
            Paragraph::new(ui::clip(
                &format!("{}{}", if selected { "> " } else { "  " }, label),
                inner.width,
            ))
            .style(style),
            row,
        );
        if selected {
            frame.set_cursor_position((row.x, row.y));
        }
        app.hits.click(
            row,
            if menu.expanded {
                Target::MenuItem(i)
            } else {
                Target::MenuGroup(i)
            },
        );
    }
    let reason = if menu.expanded {
        match items[menu.selected].item {
            Item::Action(a) => app.action_reason(a),
            Item::Manifest => None,
        }
    } else {
        None
    };
    let hint = reason.unwrap_or("Arrows move · Enter opens · Esc back");
    let footer = Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2);
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(ui::clip(hint, inner.width)),
            Line::raw("F1 Help · Esc close"),
        ]),
        footer,
    );
}
