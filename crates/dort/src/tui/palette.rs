//! Context and Global Actions. Highlighting/filtering never constructs a task.
pub use super::context::{Entry, entries};
use super::{app::App, components as ui};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Paragraph, Row, Table, TableState},
};

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
        lines.push(ui::metadata("Scope: ", entry.scope.label()));
        lines.push(ui::metadata("Effect: ", entry.effect.label()));
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
        lines.push(Line::raw(match entry.effect {
            super::context::Effect::Preview | super::context::Effect::Draft => "Opens an editable draft or requests a read-only preview. Core guards remain authoritative; execution requires a fresh preview and separate explicit confirmation.",
            super::context::Effect::Disclosure => "Only opens the content warning. Literal view patch is separately required. Highlighting/search never reads patch content.",
            _ => "Navigation, drafts and read requests do not authorize writes.",
        }));
    } else {
        lines.push(Line::raw("No matching actions. Ctrl-U clears the search."));
    }
    lines.push(Line::raw("Context commands belong to this view. Global commands navigate or change the session. Search matches labels only, case-insensitively; it never narrows execution scope. Effect labels distinguish drafts, mutation previews and content warnings."));
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
    let items: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let group = entry.scope.short();
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
                    "{}{} [{}]",
                    if unavailable { "x " } else { "" },
                    ui::inline(entry.action.label()),
                    entry.effect.label()
                ),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(items, [Constraint::Length(10), Constraint::Min(8)])
        .header(Row::new(["SCOPE", "ACTION / EFFECT"]).style(ui::muted()))
        .block(ui::pane("Context + Global · Enter choose", !searching))
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
            "{} | {}\n{}",
            entry.scope.label(),
            entry.effect.label(),
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
