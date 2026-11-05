//! Typed, context-ordered Actions. Highlighting/filtering never constructs a task.
use super::{
    app::{App, Tab},
    components as ui,
    shell::Action,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Paragraph, Row, Table, TableState},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Item,
    View,
    Navigate,
    Repository,
    Workflow,
    Preview,
    Content,
    Help,
}
impl Group {
    fn label(self) -> &'static str {
        match self {
            Self::Item => "Current item",
            Self::View => "Current view",
            Self::Navigate => "Navigate",
            Self::Repository => "Repository",
            Self::Workflow => "Other workflows",
            Self::Preview => "Mutation previews",
            Self::Content => "Content disclosure",
            Self::Help => "Help and session",
        }
    }
    fn short(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::View => "View",
            Self::Workflow => "Workflow",
            Self::Preview => "Preview",
            Self::Content => "Content",
            Self::Help => "Help",
            _ => self.label(),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Entry {
    pub action: Action,
    pub group: Group,
}

pub fn entries(tab: Tab, discovering: bool, preferred: &[Action]) -> Vec<Entry> {
    use Action::*;
    let mut rows: Vec<_> = [
        Destination(Tab::Overview),
        Destination(Tab::Files),
        Destination(Tab::Recovery),
        Destination(Tab::Checks),
        Destination(Tab::Git),
        Choose,
        Init,
        Discover,
        Adopt,
        AdoptSelected,
        BatchAdopt,
        ApplyMarked,
        KeepMarked,
        RemoveMarked,
        ToggleMark,
        ClearMarks,
        ExpandAll,
        CollapseAll,
        Profiles,
        CheckCurrent,
        CheckStandalone,
        StandaloneProfiles,
        Refresh,
        Apply,
        Keep,
        Remove,
        KeepId,
        RemoveId,
        ClearFilter,
        Inspect,
        Rollback,
        Resume,
        Abandon,
        PreviewOnly,
        GitRoot,
        GitDefault,
        GitStatus,
        GitUnstaged,
        GitStaged,
        GitRefresh,
        GitPatch,
        Diagnostics,
        About,
        Help,
        Quit,
    ]
    .into_iter()
    .map(|action| {
        let group = match action {
            Apply | Keep | Remove | KeepId | RemoveId | Rollback | Resume | Abandon
            | BatchAdopt | ApplyMarked | KeepMarked | RemoveMarked => Group::Preview,
            GitPatch => Group::Content,
            Inspect if matches!(tab, Tab::Files | Tab::Recovery) => Group::Item,
            AdoptSelected if tab == Tab::Files && discovering => Group::Item,
            ToggleMark | ClearMarks | ExpandAll | CollapseAll if tab == Tab::Files => Group::View,
            Refresh => Group::View,
            Discover | Adopt | ClearFilter if tab == Tab::Files => Group::View,
            ClearFilter if tab == Tab::Recovery => Group::View,
            CheckCurrent | CheckStandalone | StandaloneProfiles | Diagnostics
                if tab == Tab::Checks =>
            {
                Group::View
            }
            GitStatus | GitUnstaged | GitStaged | GitRefresh | GitRoot | GitDefault
                if tab == Tab::Git =>
            {
                Group::View
            }
            Choose | Init | Discover if tab == Tab::Overview => Group::View,
            Destination(_) => Group::Navigate,
            Choose | Init | Profiles => Group::Repository,
            About | Help | Quit | PreviewOnly => Group::Help,
            _ => Group::Workflow,
        };
        Entry { action, group }
    })
    .collect();
    // Stable sort: context/page recommendations first within their group. Every
    // action occurs exactly once; risk groups never mix into routine navigation.
    rows.sort_by_key(|row| {
        (
            row.group,
            preferred
                .iter()
                .position(|a| *a == row.action)
                .unwrap_or(usize::MAX),
        )
    });
    rows
}
pub fn matches<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let query = query.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| e.action.label().to_ascii_lowercase().contains(&query))
        .collect()
}
fn details(app: &App, rows: &[&Entry], query: &str, selected: usize) -> Text<'static> {
    let mut lines = vec![
        Line::styled("Actions | No work while browsing", ui::heading()),
        ui::metadata("Full query: ", query),
    ];
    if let Some(entry) = rows.get(selected) {
        lines.push(ui::metadata("Group: ", entry.group.label()));
        lines.push(ui::metadata("Action: ", entry.action.label()));
        if let Some(reason) = app.action_reason(entry.action) {
            lines.push(Line::styled("Unavailable", ui::warning()));
            lines.push(Line::raw(ui::inline(reason)));
        } else {
            lines.push(Line::styled(
                "Enter requests this action; it is not approval.",
                ui::accent(),
            ));
        }
        lines.push(Line::raw(match entry.group {
            Group::Preview => "Read-only preview or exact-ID draft only. Core guards remain authoritative; execution requires a fresh preview and separate explicit confirmation.",
            Group::Content => "Only opens the content warning. Literal view patch is separately required. Highlighting/search never reads patch content.",
            _ => "Navigation, drafts and read requests do not authorize writes.",
        }));
    } else {
        lines.push(Line::raw("No matching actions. Ctrl-U clears the search."));
    }
    lines.push(Line::raw("Search matches labels only, case-insensitively; it never narrows execution scope. Mutation previews and content disclosure are separate groups."));
    lines.push(Line::raw("Home returns controls. Arrows choose results; / edits search. Esc leaves search, then closes Actions."));
    Text::from(lines)
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    entries: &[Entry],
    query: &str,
    selected: usize,
    searching: bool,
) {
    let rows = matches(entries, query);
    if app.shell.scroll > 0 {
        ui::report(
            frame,
            area,
            "Actions | Full selected action",
            details(app, &rows, query, selected),
            app.shell.scroll - 1,
            true,
        );
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(area);
    ui::input(
        frame,
        parts[0],
        if searching {
            "Actions · Search by label"
        } else {
            "Actions · / to search"
        },
        query,
        searching,
    );
    let mut previous = None;
    let items: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let group = if previous == Some(entry.group) {
                ""
            } else {
                entry.group.short()
            };
            previous = Some(entry.group);
            let unavailable = app.action_reason(entry.action).is_some();
            let style = if i == selected {
                ui::selected()
            } else if unavailable {
                ui::muted()
            } else {
                ratatui::style::Style::default()
            };
            Row::new([
                group.to_owned(),
                format!(
                    "{}{}",
                    if unavailable { "x " } else { "" },
                    ui::inline(entry.action.label())
                ),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(items, [Constraint::Length(10), Constraint::Min(8)])
        .header(Row::new(["GROUP", "ACTION"]).style(ui::muted()))
        .block(ui::pane("Actions · Enter choose", !searching))
        .highlight_symbol("> ")
        .row_highlight_style(ui::selected());
    let mut state = TableState::default().with_selected((!rows.is_empty()).then_some(selected));
    frame.render_stateful_widget(table, parts[1], &mut state);
    app.hits
        .click(parts[0], super::input::Target::PaletteSearch);
    app.hits.wheel(parts[1], super::input::Wheel::Choices);
    for (y, entry) in rows
        .iter()
        .skip(state.offset())
        .take(usize::from(parts[1].height.saturating_sub(3)))
        .enumerate()
    {
        app.hits.click(
            Rect::new(
                parts[1].x + 1,
                parts[1].y + 2 + y as u16,
                parts[1].width.saturating_sub(2),
                1,
            ),
            super::input::Target::Palette(entry.action),
        );
    }
    let note = if let Some(entry) = rows.get(selected) {
        format!(
            "{} | {}",
            entry.group.label(),
            app.action_reason(entry.action)
                .unwrap_or("Enter request · PgDn full details · Esc back")
        )
    } else {
        "No matching actions. Ctrl-U clears search.".into()
    };
    frame.render_widget(
        Paragraph::new(ui::inline(&note))
            .style(
                if rows
                    .get(selected)
                    .is_some_and(|e| app.action_reason(e.action).is_some())
                {
                    ui::warning()
                } else {
                    ui::muted()
                },
            )
            .wrap(ratatui::widgets::Wrap { trim: false }),
        parts[2],
    );
}
