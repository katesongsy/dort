//! Shared presentation catalog. Scope is independent of effect; neither grants authority.
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
impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Item => "Context: selected item",
            Self::Marked => "Context: marked items",
            Self::View => "Context: current view",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::Marked => "Marked",
            Self::View => "View",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    View,
    Read,
    Draft,
    Preview,
    Disclosure,
}
impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Self::View => "View / selection",
            Self::Read => "Read-only",
            Self::Draft => "Editable draft",
            Self::Preview => "Mutation preview",
            Self::Disclosure => "Content warning",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Entry {
    pub action: Action,
    pub scope: Scope,
    pub effect: Effect,
    pub primary: Option<usize>,
}
fn entry(action: Action, primary: Option<usize>) -> Entry {
    use Action::*;
    let scope = match action {
        Inspect | AdoptSelected | Keep | Remove | IgnoreSelected | Rollback | Resume | Abandon
        | ToggleMark => Scope::Item,
        BatchAdopt | ApplyMarked | KeepMarked | RemoveMarked | ClearMarks => Scope::Marked,
        _ => Scope::View,
    };
    let effect = match action {
        Apply | Keep | Remove | IgnoreSelected | Rollback | Resume | Abandon | ApplyMarked
        | KeepMarked | RemoveMarked => Effect::Preview,
        Adopt | AdoptSelected | BatchAdopt | KeepId | RemoveId | Init | CheckStandalone
        | Profiles | StandaloneProfiles => Effect::Draft,
        GitPatch => Effect::Disclosure,
        Discover | Refresh | CheckCurrent | Diagnostics | GitStatus | GitUnstaged | GitStaged
        | GitRefresh => Effect::Read,
        _ => Effect::View,
    };
    Entry {
        action,
        scope,
        effect,
        primary,
    }
}
/// Only operations belonging to this view. Menus own navigation/session commands.
/// Missing prerequisites stay visible and are explained by App's authoritative dispatch guard.
pub fn entries(app: &App) -> Vec<Entry> {
    use Action::*;
    let (primary, other): (Vec<Action>, Vec<Action>) = match app.tab {
        Tab::Overview => (vec![Apply], vec![Profiles, Refresh]),
        Tab::Files if app.discovering => (
            vec![IgnoreSelected, AdoptSelected, BatchAdopt],
            vec![
                ToggleMark,
                ClearMarks,
                ExpandAll,
                CollapseAll,
                Adopt,
                Discover,
                ClearFilter,
                Refresh,
                Profiles,
            ],
        ),
        Tab::Files => (
            vec![
                Apply,
                Keep,
                Discover,
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
            vec![CheckCurrent, CheckStandalone, Profiles],
            vec![StandaloneProfiles, Refresh],
        ),
        Tab::Git => (vec![GitPatch, GitRefresh], vec![ClearFilter]),
    };
    let mut rows: Vec<_> = primary
        .into_iter()
        .enumerate()
        .map(|(i, a)| entry(a, Some(i)))
        .chain(other.into_iter().map(|a| entry(a, None)))
        .collect();
    // Diagnostics is a refresh operation only when already in that subview.
    rows.retain(|e| {
        !super::navigation::owns(e.action)
            || (e.action == Diagnostics && app.tab == Tab::Checks && app.showing_diagnostics)
    });
    rows.sort_by_key(|e| e.scope);
    rows
}
pub fn page_actions(app: &App) -> Vec<Action> {
    // Primary ordering is deliberately independent of the palette's scope sections.
    let mut rows: Vec<_> = entries(app)
        .into_iter()
        .filter(|e| e.primary.is_some())
        .filter(|e| app.tab != Tab::Recovery || app.action_reason(e.action).is_none())
        .collect();
    rows.sort_by_key(|e| e.primary);
    rows.into_iter().map(|e| e.action).collect()
}
