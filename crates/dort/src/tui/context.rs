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
    Global,
}
impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Item => "Context: selected item",
            Self::Marked => "Context: marked items",
            Self::View => "Context: current view",
            Self::Global => "Global: navigation / session",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::Marked => "Marked",
            Self::View => "View",
            Self::Global => "Global",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Navigate,
    Read,
    Draft,
    Preview,
    Disclosure,
}
impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Self::Navigate => "Navigation / session",
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
        Destination(_) | Choose | Init | About | Help | Quit | PreviewOnly => Scope::Global,
        _ => Scope::View,
    };
    let effect = match action {
        Apply | Keep | Remove | IgnoreSelected | Rollback | Resume | Abandon | ApplyMarked
        | KeepMarked | RemoveMarked => Effect::Preview,
        Adopt | AdoptSelected | BatchAdopt | KeepId | RemoveId | Init => Effect::Draft,
        GitPatch => Effect::Disclosure,
        Discover | Refresh | CheckCurrent | Diagnostics | GitStatus | GitUnstaged | GitStaged
        | GitRefresh => Effect::Read,
        _ => Effect::Navigate,
    };
    Entry {
        action,
        scope,
        effect,
        primary,
    }
}
/// Only workflows belonging to this view, followed by global navigation/session controls.
/// Missing prerequisites stay visible and are explained by App's authoritative dispatch guard.
pub fn entries(app: &App) -> Vec<Entry> {
    use Action::*;
    let (primary, other): (Vec<Action>, Vec<Action>) = match app.tab {
        Tab::Overview => (vec![Discover, Apply], vec![Profiles, Refresh]),
        Tab::Files if app.discovering => (
            vec![IgnoreSelected, ManageIgnored, AdoptSelected, BatchAdopt],
            vec![
                Inspect,
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
                Inspect,
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
        Tab::Recovery => (
            vec![Inspect, Rollback, Resume, Abandon],
            vec![ClearFilter, Refresh],
        ),
        Tab::Checks => (
            vec![CheckCurrent, CheckStandalone, Diagnostics, Profiles],
            vec![StandaloneProfiles, Refresh],
        ),
        Tab::Git => (
            vec![GitStatus, GitUnstaged, GitStaged, GitPatch],
            vec![Inspect, ClearFilter, GitRoot, GitDefault, GitRefresh],
        ),
    };
    let mut rows: Vec<_> = primary
        .into_iter()
        .enumerate()
        .map(|(i, a)| entry(a, Some(i + if app.tab == Tab::Overview { 2 } else { 0 })))
        .chain(other.into_iter().map(|a| entry(a, None)))
        .collect();
    rows.sort_by_key(|e| e.scope);
    rows.extend(
        [
            Destination(Tab::Overview),
            Destination(Tab::Files),
            Destination(Tab::Recovery),
            Destination(Tab::Checks),
            Destination(Tab::Git),
            Choose,
            Init,
            PreviewOnly,
            About,
            Help,
            Quit,
        ]
        .into_iter()
        .map(|a| {
            entry(
                a,
                if app.tab == Tab::Overview {
                    match a {
                        Init => Some(0),
                        Choose => Some(1),
                        _ => None,
                    }
                } else {
                    None
                },
            )
        }),
    );
    rows
}
pub fn page_actions(app: &App) -> Vec<Action> {
    // Primary ordering is deliberately independent of the palette's scope sections.
    let mut rows: Vec<_> = entries(app)
        .into_iter()
        .filter(|e| e.primary.is_some())
        .filter(|e| {
            app.tab != Tab::Recovery
                || e.action == Action::Inspect
                || app.action_reason(e.action).is_none()
        })
        .collect();
    rows.sort_by_key(|e| e.primary);
    rows.into_iter().map(|e| e.action).collect()
}
