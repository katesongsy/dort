//! Contextual action menu.
use super::{
    actions::Task,
    app::App,
    components::{self as ui, menu::Item},
    input::Target,
};
use ratatui::crossterm::event::KeyCode;

pub struct More {
    pub title: &'static str,
    pub items: Vec<Item>,
    pub selected: usize,
    pub details: bool,
    pub feedback: Option<&'static str>,
}
impl More {
    pub fn new(title: &'static str, items: Vec<Item>) -> Self {
        Self {
            title,
            items,
            selected: 0,
            details: false,
            feedback: None,
        }
    }
}
pub(super) fn eligible(app: &App, action: &ui::ActionButton) -> bool {
    !matches!(action.key, "Enter" | "Esc")
        && !(matches!(action.target, Target::ToolInstallControl(_, true))
            && app.tools_view.install_open
            && matches!(
                app.tools_view.installation.mode,
                super::tools_install::Mode::CloseReview
            ))
        && !matches!(action.target, Target::Continue)
}
/// Session controls without their own key; More is their only entry point.
fn session_actions(app: &App) -> Vec<ui::ActionButton> {
    use super::app::Tab;
    use super::shell::Action;
    let mut actions = vec![
        ui::ActionButton {
            label: "Review environment plan",
            key: "",
            target: Target::Action(Action::EnvironmentPlan),
        },
        ui::ActionButton {
            label: "Environment check",
            key: "",
            target: Target::Action(Action::EnvironmentCheck),
        },
        ui::ActionButton {
            label: "Review environment resume",
            key: "",
            target: Target::Action(Action::EnvironmentResume),
        },
        ui::ActionButton {
            label: "Abandon environment run",
            key: "",
            target: Target::Action(Action::EnvironmentAbandon),
        },
        ui::ActionButton {
            label: "Clone repository",
            key: "",
            target: Target::Action(Action::Clone),
        },
        ui::ActionButton {
            label: "Saved profiles",
            key: "",
            target: Target::Action(Action::SavedProfiles),
        },
        ui::ActionButton {
            label: "Toggle preview-only mode",
            key: "",
            target: Target::Action(Action::PreviewOnly),
        },
        ui::ActionButton {
            label: "About Dort",
            key: "",
            target: Target::Action(Action::About),
        },
    ];
    use super::configuration::Kind;
    let configurations: &[Kind] = if app.tab == Tab::Files {
        &[Kind::File, Kind::EditFile]
    } else if app.tab == Tab::Overview {
        &[
            Kind::Profile,
            Kind::DeleteProfile,
            Kind::Startup,
            Kind::Forget,
            Kind::Reconnect,
        ]
    } else {
        &[Kind::Profile, Kind::DeleteProfile]
    };
    actions.extend(configurations.iter().map(|&kind| ui::ActionButton {
        label: kind.label(),
        key: "",
        target: Target::Action(Action::Configure(kind)),
    }));
    if app.tab == Tab::Git && !app.git_view.follows_files {
        actions.push(ui::ActionButton {
            label: "Follow selected repository",
            key: "",
            target: Target::Action(Action::GitDefault),
        });
    }
    actions
}
impl App {
    pub(super) fn open_more(&mut self, mut actions: Vec<ui::ActionButton>) {
        if self.busy {
            self.interaction_notice = self.navigation_block();
            return;
        }
        actions.extend(session_actions(self));
        actions.retain(|action| eligible(self, action));
        let mut items = Vec::new();
        for action in actions {
            if items
                .iter()
                .any(|item: &Item| item.action.label == action.label)
            {
                continue;
            }
            let reason = if let Target::Action(action) = action.target {
                self.action_reason(action)
            } else {
                None
            };
            items.push(Item { action, reason });
        }
        self.more = Some(More::new("More actions", items));
    }
    pub(super) fn more_key(&mut self, code: KeyCode, width: u16, height: u16) -> Option<Task> {
        let menu = self.more.as_mut()?;
        menu.feedback = None;
        if menu.details {
            if matches!(code, KeyCode::Esc | KeyCode::F(3)) {
                menu.details = false;
            }
            return None;
        }
        let count = menu.items.len().max(1);
        match code {
            KeyCode::Esc | KeyCode::F(2) => self.more = None,
            KeyCode::Down | KeyCode::Tab => menu.selected = (menu.selected + 1) % count,
            KeyCode::Up | KeyCode::BackTab => menu.selected = (menu.selected + count - 1) % count,
            KeyCode::PageDown => menu.selected = (menu.selected + 3).min(count - 1),
            KeyCode::PageUp => menu.selected = menu.selected.saturating_sub(3),
            KeyCode::Home => menu.selected = 0,
            KeyCode::End => menu.selected = count - 1,
            KeyCode::F(3) => menu.details = true,
            KeyCode::Enter => {
                let item = *menu.items.get(menu.selected)?;
                if let Some(reason) = item.reason {
                    menu.feedback = Some(reason);
                    return None;
                }
                self.more = None;
                self.hits.clear();
                return self.activate_target(item.action.target, width, height);
            }
            _ => (),
        }
        None
    }
    pub(super) fn presented(&self, label: &'static str, key: &'static str, target: Target) {
        self.hits.present(ui::ActionButton { label, key, target });
    }
}
