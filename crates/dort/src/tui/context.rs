use super::{
    app::{App, Tab},
    shell::Action,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Item,
    Marked,
    View,
}
#[derive(Clone, Copy)]
pub struct Entry {
    pub action: Action,
    pub scope: Scope,
    pub primary: Option<usize>,
}
pub(super) fn entry(action: Action, primary: Option<usize>) -> Entry {
    use Action::*;
    let scope = match action {
        Inspect | AdoptSelected | Keep | Remove | IgnoreSelected | Rollback | Resume | Abandon
        | ToggleMark => Scope::Item,
        BatchAdopt | ApplyMarked | KeepMarked | RemoveMarked | ClearMarks => Scope::Marked,
        _ => Scope::View,
    };
    Entry {
        action,
        scope,
        primary,
    }
}
/// Only operations belonging to this view. Keys and the rail own navigation and session commands.
/// Missing prerequisites stay visible and are explained by App's authoritative dispatch guard.
pub fn entries(app: &App) -> Vec<Entry> {
    use Action::*;
    let (primary, other): (Vec<Action>, Vec<Action>) = match app.tab {
        Tab::Overview => (vec![Apply], vec![Refresh]),
        Tab::Files if app.discovering => (
            vec![AdoptSelected, IgnoreSelected, ToggleMark, BatchAdopt],
            vec![
                ClearMarks,
                ExpandAll,
                CollapseAll,
                Adopt,
                ClearFilter,
                Refresh,
                Profiles,
            ],
        ),
        Tab::Files => (
            vec![
                Apply,
                Keep,
                ApplyMarked,
                KeepMarked,
                RemoveMarked,
                ToggleMark,
            ],
            vec![
                Remove,
                KeepId,
                RemoveId,
                ClearMarks,
                ExpandAll,
                CollapseAll,
                ClearFilter,
                Adopt,
                Refresh,
                Profiles,
            ],
        ),
        Tab::Recovery => (vec![Rollback, Resume, Abandon], vec![ClearFilter, Refresh]),
        Tab::Checks if app.showing_diagnostics => (vec![Diagnostics], vec![]),
        Tab::Checks => (
            vec![
                CheckCurrent,
                CheckStandalone,
                Configure(super::configuration::Kind::Repair),
                Profiles,
            ],
            vec![StandaloneProfiles, Refresh],
        ),
        Tab::Git => (vec![GitRefresh], vec![ClearFilter]),
        Tab::Tools => (
            vec![ToolsCheck, ToolsInstallMissing],
            vec![
                ToolsStandaloneCheck,
                ToolDeclaration,
                ToolsInstalled,
                ToolAttempts,
            ],
        ),
    };
    let mut rows: Vec<_> = primary
        .into_iter()
        .enumerate()
        .map(|(i, a)| entry(a, Some(i)))
        .chain(other.into_iter().map(|a| entry(a, None)))
        .collect();
    // Diagnostics is a refresh operation only when already in that subview.
    // Section destinations and session commands stay on keys and the rail.
    rows.retain(|e| {
        !matches!(
            e.action,
            Discover
                | ManageIgnored
                | Diagnostics
                | GitStatus
                | GitUnstaged
                | GitStaged
                | ToolsInstalled
                | ToolAttempts
        ) || (e.action == Diagnostics && app.tab == Tab::Checks && app.showing_diagnostics)
    });
    // Related task order is not the foreground primary; only an explicit Enter control is.
    rows.sort_by_key(|e| (e.scope, e.primary.unwrap_or(usize::MAX)));
    rows
}
#[cfg(test)]
pub fn page_actions(app: &App) -> Vec<Action> {
    // Primary ordering is independent of the scope sections.
    let mut rows: Vec<_> = entries(app)
        .into_iter()
        .filter(|e| e.primary.is_some())
        .filter(|e| app.tab != Tab::Recovery || app.action_reason(e.action).is_none())
        .collect();
    rows.sort_by_key(|e| e.primary);
    rows.into_iter().map(|e| e.action).collect()
}

pub(super) fn control_action(
    key: ratatui::crossterm::event::KeyCode,
    controls: impl IntoIterator<Item = super::components::ActionButton>,
) -> Option<Action> {
    let ratatui::crossterm::event::KeyCode::Char(c) = key else {
        return None;
    };
    let mut buf = [0; 4];
    let key = c.encode_utf8(&mut buf);
    controls
        .into_iter()
        .find_map(|control| match control.target {
            super::input::Target::Action(action) if control.key == key => Some(action),
            _ => None,
        })
}

pub(super) fn file_action(key: ratatui::crossterm::event::KeyCode, app: &App) -> Option<Action> {
    use ratatui::crossterm::event::KeyCode;
    let KeyCode::Char(c) = key else { return None };
    let mut buf = [0; 4];
    let shortcut = if c == ' ' {
        "Space"
    } else {
        c.encode_utf8(&mut buf)
    };
    entries(app).into_iter().map(|e| e.action).find(|&action| {
        super::navigation::context_key(super::workbench::shortcut(action)) == shortcut
    })
}
