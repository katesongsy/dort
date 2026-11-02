use super::app::{App, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

// Escape all non-ASCII/control metadata before terminal rendering, including bidi controls.
pub fn safe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\n' || c.is_ascii_graphic() || c == ' ' {
            escaped.push(c);
        } else {
            escaped.extend(c.escape_default());
        }
    }
    escaped
}
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 48 || area.height < 14 {
        frame.render_widget(Paragraph::new("Dort: terminal too small\nResize to 48x14; approval disabled.\nq/Ctrl-C: exit (waits for worker)").wrap(Wrap { trim: false }), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(area);
    let tab = match app.tab {
        Tab::Overview => "Overview",
        Tab::Files => "Files",
        Tab::Recovery => "Recovery",
        Tab::Help => "Help",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Dort | 1 Overview  2 Files  3 Recovery  ? Help\n{tab}{}",
            if app.dry_run {
                " | DRY RUN: approval disabled"
            } else {
                ""
            }
        ))
        .style(Style::default().fg(Color::Cyan)),
        rows[0],
    );
    let body = if let Some(error) = &app.error {
        Some(format!(
            "{error}\n\nPgUp/PgDn: scroll | Esc: dismiss, then r: refresh Recovery"
        ))
    } else if let Some(preview) = &app.preview {
        frame.render_widget(
            Paragraph::new(safe(&preview.text))
                .block(
                    Block::default()
                        .title(" Preview (metadata only) ")
                        .borders(Borders::ALL),
                )
                .scroll((preview.scroll, preview.horizontal)),
            rows[1],
        );
        None
    } else {
        Some(match app.tab {
            Tab::Overview => app.data.as_ref().map(|d| d.overview.clone()).unwrap_or_else(|| "Loading...".into()),
            Tab::Help => "Tab / 1 / 2 / 3: change view\nUp/Down: select a file or transaction\nr: refresh observations and history\na: preview apply (Overview / Files)\nk: preview unmanage-and-keep selected file\nd: preview delete managed link + unmanage\nb: preview rollback selected schema-1/2 history\ns / x: preview resume / abandon schema-3 bootstrap\nPreview: Up/Down/PgUp/PgDn scroll; Enter confirm\nConfirmation: type exact ID for deletion, then yes\nEsc: dismiss preview/confirmation without writes\nq / Ctrl-C: exit; approved writes finish first\nNo automatic replay or rollback; inspect Recovery on failure.\nUse CLI for init, discovery/adoption and Git inspection.".into(),
            Tab::Files => app.data.as_ref().map(|d| {
                let first = app.selected.saturating_sub(2);
                let mut text = format!("{} active mapping(s) | a apply | k keep | d remove\n", d.files.len());
                for (i,(id,_)) in d.files.iter().enumerate().skip(first).take(3) { text.push_str(&format!("{} {id}\n", if i == app.selected { ">" } else { " " })); }
                if let Some((_,detail)) = d.files.get(app.selected) { text.push_str(&format!("\n{detail}")); }
                text
            }).unwrap_or_default(),
            Tab::Recovery => app.data.as_ref().map(|d| {
                let mut text = format!("{} transaction(s) | b rollback | s resume | x abandon\n",d.history.len());
                if let Some(error) = &d.history_error { text.push_str(&format!("Diagnosis failed: {error}\n")); }
                for (i,row) in d.history.iter().enumerate().skip(app.selected.saturating_sub(2)).take(3) { text.push_str(&format!("{} {} schema {:?} {:?}{}\n",if i == app.selected { ">" } else { " " }, row.transaction_id.get(..8).unwrap_or("invalid"),row.journal_schema,row.phase,if row.blocks_writes() { " BLOCKED" } else { "" })); }
                if let Some(row) = d.history.get(app.selected) { text.push_str(&format!("\nRepository ID: {}\nTransaction ID: {}\nValidated checkpoints: {}\n{}",row.repository_id,row.transaction_id,row.valid_checkpoints,row.problem.as_deref().unwrap_or("History integrity is not a live-drift check."))); }
                text
            }).unwrap_or_default(),
        })
    };
    if let Some(body) = body {
        frame.render_widget(
            Paragraph::new(safe(&body))
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL))
                .scroll((app.details_scroll, 0)),
            rows[1],
        );
    }
    frame.render_widget(
        Paragraph::new(safe(&app.status))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(if app.writing {
                Color::Yellow
            } else {
                Color::White
            })),
        rows[2],
    );
    frame.render_widget(Paragraph::new(if app.preview.is_some() { "Preview: Up/Down/PgUp/PgDn, Left/Right scroll\nEnter confirm | Esc dismiss | q exit" } else { "Tab views | PgUp/PgDn details | r refresh | ? help | q exit\nFiles: a apply / k keep / d remove | Recovery: b / s / x" }), rows[3]);
    if let Some(confirmation) = &app.confirmation {
        let modal = Rect::new(
            area.x + 2,
            area.y + 2,
            area.width.saturating_sub(4),
            area.height.saturating_sub(4),
        );
        let removal = app.preview.as_ref().and_then(|p| p.pending.removal_id());
        let prompt = if let Some(id) = removal.filter(|_| confirmation.removal_ack.is_none()) {
            format!(
                "Separate deletion acknowledgement\nType exact file ID: {id}\nRepository source is retained."
            )
        } else {
            "Approve reviewed effects\nType literal yes, then Enter.\nExecution independently revalidates all guards.".into()
        };
        frame.render_widget(Clear, modal);
        frame.render_widget(
            Paragraph::new(safe(&format!(
                "{prompt}\n\n> {}\n\nEsc: cancel without execution",
                confirmation.input
            )))
            .wrap(Wrap { trim: false })
            .block(Block::default().title(" Confirm ").borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow)),
            modal,
        );
    }
}
