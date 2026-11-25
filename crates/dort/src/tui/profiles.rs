use super::{
    app::App,
    components::{self as ui, ActionButton},
    configuration::Kind,
    input::{Target, Wheel},
    navigation::ListViewport,
    shell::Action,
};
use dort_core::profiles::Row as ProfileRow;
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Rect},
    widgets::Row,
};
use ui::collection::{Collection, Header, RowTargets};

#[derive(Default)]
pub struct Page {
    pub open: bool,
    pub selected: usize,
    viewport: ListViewport,
}

impl App {
    fn profile_rows(&self) -> &[ProfileRow] {
        self.data.as_ref().map_or(&[], |data| &data.profile_rows)
    }

    pub(super) fn selected_profile(&self) -> Option<&ProfileRow> {
        self.profile_rows().get(self.profiles.selected)
    }

    pub(super) fn profile_action_reason(&self) -> Option<&'static str> {
        if !self.profiles.open || self.selected_profile().is_none_or(|row| row.name == "base") {
            Some("Select a profile other than base. Base stays on.")
        } else {
            None
        }
    }

    pub(super) fn profiles_key(&mut self, key: KeyCode) -> Option<super::actions::Task> {
        let last = self.profile_rows().len().saturating_sub(1);
        let page = self.profiles.viewport.page();
        let selected = self.profiles.selected;
        self.profiles.selected = match key {
            KeyCode::Esc => {
                self.profiles.open = false;
                return None;
            }
            KeyCode::Down => selected.saturating_add(1).min(last),
            KeyCode::Up => selected.saturating_sub(1),
            KeyCode::PageDown => selected.saturating_add(page).min(last),
            KeyCode::PageUp => selected.saturating_sub(page),
            KeyCode::Home => 0,
            KeyCode::End => last,
            _ => {
                if let Some(action) = super::context::control_action(key, controls(self)) {
                    return self.activate(action);
                }
                return None;
            }
        };
        None
    }
}

fn controls(app: &App) -> impl Iterator<Item = ActionButton> {
    let editable = app.selected_profile().is_some_and(|row| row.name != "base");
    [
        (
            Action::SaveProfileChoice.label(),
            "s",
            Action::SaveProfileChoice,
            editable,
        ),
        (
            "New profile",
            "n",
            Action::Configure(Kind::CreateProfile),
            true,
        ),
        (
            Kind::EditProfileDefaults.label(),
            "e",
            Action::Configure(Kind::EditProfileDefaults),
            editable,
        ),
        (
            Kind::RenameProfile.label(),
            "r",
            Action::Configure(Kind::RenameProfile),
            editable,
        ),
        (
            Kind::DeleteProfile.label(),
            "d",
            Action::Configure(Kind::DeleteProfile),
            editable,
        ),
        ("Temporary profiles", "t", Action::Profiles, true),
    ]
    .into_iter()
    .filter(|&(_, _, _, visible)| visible)
    .map(|(label, key, action, _)| ActionButton {
        label,
        key,
        target: Target::Action(action),
    })
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    for control in controls(app) {
        app.hits.present(control);
    }
    let profiles = app.profile_rows();
    if profiles.is_empty() {
        ui::chrome_prose(frame, area, "Profile choices are unavailable.", ui::muted());
        return;
    }
    let collection = Collection::new(
        [Constraint::Min(16), Constraint::Length(12)],
        Header::Heading(["PROFILE / ACTIVE STATE", "LOCAL CHOICE"]),
    )
    .titled("Profile definitions", true);
    let window = collection.window(
        area,
        &app.profiles.viewport,
        profiles.len(),
        Some(app.profiles.selected),
    );
    collection.draw(
        frame,
        area,
        window.range.clone().map(|index| {
            let row = &profiles[index];
            (
                index,
                Row::new([
                    ui::inline(&row.label()),
                    if row.name == "base" {
                        "Fixed"
                    } else {
                        row.choice.label()
                    }
                    .into(),
                ]),
            )
        }),
        window.selection,
        RowTargets {
            hits: &app.hits,
            wheel: Some(Wheel::Choices),
            target: Target::Row,
        },
    );
}
