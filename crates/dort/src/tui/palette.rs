//! Contextual Actions. Highlighting/filtering never constructs a task.
pub use super::context::{Entry, entries};
use super::{app::App, components as ui};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Row, Table, TableState},
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
        Line::styled("Actions | Available operations", ui::heading()),
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
                "Enter opens this action's next step.",
                ui::accent(),
            ));
        }
        lines.push(Line::raw(match entry.effect {
            super::context::Effect::Preview | super::context::Effect::Draft => "Opens an editable draft or requests a read-only preview. Core guards remain authoritative; execution requires a fresh preview and separate explicit confirmation.",
            super::context::Effect::Disclosure => "Opens a content warning with a View patch action.",
            _ => "Writes require a reviewed, explicit approval.",
        }));
    } else {
        lines.push(Line::raw("No matching actions. Ctrl-U clears the search."));
    }
    lines.push(Line::raw("Commands operate on this view or selection. Places provides navigation and session controls. Search matches labels case-insensitively; the operation's review defines execution scope. Effect labels distinguish drafts, mutation previews and content warnings."));
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
    use super::input::Target;
    let rows = matches(entries, query);
    let content = ui::action_bar(
        frame,
        area,
        &app.hits,
        ui::FormActions {
            choices: &[],
            buttons: &[ui::ActionButton {
                label: if app.shell.picker_details {
                    "Actions"
                } else {
                    "Metadata"
                },
                key: "F3",
                target: Target::PickerDetails,
            }],
            feedback: None,
        },
    );
    if app.shell.picker_details {
        app.shell
            .page_rows
            .set(content.height.saturating_sub(2).max(1));
        ui::report(
            frame,
            content,
            "Action details",
            details(app, &rows, query, selected),
            app.shell.scroll,
            false,
        );
        return;
    }
    let detail_height = if content.height >= 14 { 6 } else { 0 };
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(detail_height),
    ])
    .split(content);
    ui::design::filter(
        frame,
        parts[0],
        if app.shell.global_commands {
            "Find a task"
        } else {
            "Find an action"
        },
        query,
        searching && app.hits.input_focus_allowed(),
    );
    if rows
        .get(selected)
        .is_some_and(|entry| app.action_reason(entry.action).is_none())
    {
        app.presented(
            "Open",
            "Enter",
            Target::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
        );
    }
    let mut items: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let unavailable = app.action_reason(entry.action).is_some();
            Row::new([
                entry.scope.short().to_owned(),
                format!(
                    "{}{} [{}]",
                    if unavailable { "x " } else { "" },
                    entry.action.label(),
                    entry.effect.label()
                ),
            ])
            .style(if i == selected {
                ui::selected()
            } else if unavailable {
                ui::muted()
            } else {
                ratatui::style::Style::default()
            })
        })
        .collect();
    if items.is_empty() {
        items.push(Row::new(["", "No matching actions."]).style(ui::muted()));
    }
    let framed = parts[1].height >= 7;
    app.shell.page_rows.set(
        parts[1]
            .height
            .saturating_sub(if framed { 2 } else { 1 })
            .max(1),
    );
    let mut table = Table::new(items, [Constraint::Length(8), Constraint::Min(8)])
        .header(Row::new(["SCOPE", "ACTION / EFFECT"]).style(ui::muted()))
        .highlight_symbol(format!("{} ", ui::glyphs::SELECTED))
        .row_highlight_style(ui::selected());
    if framed {
        table = table.block(ui::pane(
            if rows.is_empty() {
                "No matching actions"
            } else {
                "Available actions"
            },
            !searching,
        ));
    }
    let mut state = TableState::default().with_selected((!rows.is_empty()).then_some(selected));
    frame.render_stateful_widget(table, parts[1], &mut state);
    app.hits.click(parts[0], Target::PaletteSearch);
    app.hits.wheel(parts[1], super::input::Wheel::Choices);
    let inset = u16::from(framed);
    for (y, entry) in rows
        .iter()
        .skip(state.offset())
        .take(usize::from(parts[1].height.saturating_sub(1 + inset)))
        .enumerate()
    {
        app.hits.click(
            Rect::new(
                parts[1].x,
                parts[1].y + 1 + inset + y as u16,
                parts[1].width,
                1,
            ),
            Target::Palette(entry.action),
        );
    }
    if detail_height > 0 {
        ui::report(
            frame,
            parts[2],
            "Action details",
            details(app, &rows, query, selected),
            app.shell.scroll,
            false,
        );
    }
}
