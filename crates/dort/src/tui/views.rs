use super::{
    app::{App, Tab},
    shell::{self, Form, Overlay},
};
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
const HELP: &str = "Left/Right or 1/2/3: Overview / Files / Recovery\nTab / Shift-Tab: focus Destinations, Content, Actions\nEnter: activate focused control | Space: Actions\n/: search Actions | Delete: clear search (outside text)\np: choose repository for session | n: create/attach\nr: refresh observations and all-repository history\na: preview apply (Overview / Files)\nk / d: keep local file / delete managed link in Files\nb: preview rollback in Recovery | s / x: resume / abandon\nPreview: arrows / PgUp/PgDn scroll; Enter continue\nConfirmation: exact file ID for deletion, then literal yes\nEsc: confirmation -> preview -> retained form -> destination\nForms/search: printable q ? / are text; Backspace edits\nCtrl-U in a form: discard its draft | PgUp/PgDn: scroll\nq outside text / Ctrl-C anywhere: exit, joining worker\nNo automatic replay/rollback, network, staging or commits.\nDiscovery/adoption, profile selection and Git inspection are not available in this interface.\nEsc: return to previous destination and focus.";

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 48 || area.height < 14 {
        frame.render_widget(Paragraph::new("Dort: terminal too small\nResize to 48x14; approval disabled.\nq/Ctrl-C: exit (waits for worker)").wrap(Wrap { trim: false }), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(4),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(area);
    let root = app
        .data
        .as_ref()
        .and_then(|d| d.repository.as_ref())
        .or(app.repository.as_ref());
    let scope = root
        .map(|path| format!("{path:?}"))
        .unwrap_or_else(|| "no selection".into());
    let destination = match app.tab {
        Tab::Overview => "Overview",
        Tab::Files => "Files",
        Tab::Recovery => "Recovery",
    };
    frame.render_widget(Paragraph::new(safe(&format!(
        "Dort | 1 Overview  2 Files  3 Recovery | [Actions: Space]\nRepository: {scope}\n{}{} | Profiles: {:?}\n{destination} | Focus: {} | ? Help | p Choose | n Create/attach", app.origin,
        if app.dry_run { " | DRY RUN / PREVIEW ONLY" } else { "" }, app.data.as_ref().map(|d| d.profiles.as_slice()).unwrap_or_default(), app.shell.focus.label()
    ))).style(Style::default().fg(Color::Cyan)), rows[0]);
    let body = if let Some(error) = &app.error {
        Some(format!(
            "{error}\n\nPgUp/PgDn: scroll | Esc: return to retained input/scope"
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
    } else if let Some(overlay) = &app.shell.overlay {
        Some(overlay_text(app, overlay, rows[1].height))
    } else {
        let mut body = app
            .result
            .as_ref()
            .map(|result| format!("{result}\nEsc clears result.\n\n"))
            .unwrap_or_default();
        body.push_str(&match app.tab {
            Tab::Overview => app.data.as_ref().map(|d| d.overview.clone()).unwrap_or_else(|| "Loading...".into()),
            Tab::Files => app.data.as_ref().map(|d| {
                let mut text = format!("{} active mapping(s) | a apply | k keep | d remove\n", d.files.len());
                let count = usize::from(rows[1].height.saturating_sub(9)).max(1);
                for (i,(id,_)) in d.files.iter().enumerate().skip(app.selected.saturating_sub(count / 2)).take(count) { text.push_str(&format!("{} {id}\n", if i == app.selected { ">" } else { " " })); }
                if let Some((_,detail)) = d.files.get(app.selected) { text.push_str(&format!("\n{detail}")); }
                text
            }).unwrap_or_default(),
            Tab::Recovery => app.data.as_ref().map(|d| {
                let mut text = format!("All repositories: {} transaction(s)\nb rollback | s resume | x abandon\n",d.history.len());
                if let Some(error) = &d.history_error { text.push_str(&format!("Diagnosis failed: {error}\n")); }
                let count = usize::from(rows[1].height.saturating_sub(11)).max(1);
                for (i,row) in d.history.iter().enumerate().skip(app.selected.saturating_sub(count / 2)).take(count) { text.push_str(&format!("{} {} schema {:?} {:?}{}\n",if i == app.selected { ">" } else { " " }, row.transaction_id.get(..8).unwrap_or("invalid"),row.journal_schema,row.phase,if row.blocks_writes() { " BLOCKED" } else { "" })); }
                if let Some(row) = d.history.get(app.selected) { text.push_str(&format!("\nRepository ID: {}\nTransaction ID: {}\nValidated checkpoints: {}\n{}",row.repository_id,row.transaction_id,row.valid_checkpoints,row.problem.as_deref().unwrap_or("History integrity is not a live-drift check."))); }
                text
            }).unwrap_or_default(),
        });
        Some(body)
    };
    if let Some(body) = body {
        frame.render_widget(
            Paragraph::new(safe(&body))
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL))
                .scroll((
                    if app.error.is_none() && app.shell.overlay.is_some() {
                        app.shell.scroll
                    } else {
                        app.details_scroll
                    },
                    0,
                )),
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
    let footer = if app.preview.is_some() {
        "Preview: arrows/PgUp/PgDn scroll | Enter continue\nEsc back (no write) | q quit | Ctrl-C waits for worker"
    } else if app.shell.text_entry() {
        "Text entry: Enter review/select | Backspace edit\nEsc back | Ctrl-U discard draft | Ctrl-C quit"
    } else {
        "Tab/Shift-Tab focus | Enter choose | Space Actions\nEsc back | ? help | q quit | PgUp/PgDn details"
    };
    frame.render_widget(Paragraph::new(footer), rows[3]);
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
fn overlay_text(app: &App, overlay: &Overlay, height: u16) -> String {
    match overlay {
        Overlay::Help => format!("Dort {} | Help / About\n{HELP}", env!("CARGO_PKG_VERSION")),
        Overlay::Actions {
            query,
            selected,
            searching,
        } => {
            let mut text = format!(
                "Actions | {}\nSearch: {query}\n/: edit search | Esc: leave search/back | Delete: clear\n",
                if *searching {
                    "TEXT ENTRY"
                } else {
                    "Enter choose"
                }
            );
            let count = usize::from(height.saturating_sub(5)).max(1);
            for (i, action) in shell::actions(query)
                .iter()
                .enumerate()
                .skip(selected.saturating_sub(count / 2))
                .take(count)
            {
                text.push_str(&format!(
                    "{} {}\n",
                    if i == *selected { ">" } else { " " },
                    action.label()
                ));
            }
            text
        }
        Overlay::Repositories { selected } => {
            let roots = app
                .data
                .as_ref()
                .map(|data| data.known_roots.as_slice())
                .unwrap_or_default();
            let mut entries = vec![
                "Enter exact repository path".into(),
                "Use registered active repository".into(),
            ];
            entries.extend(roots.iter().map(|root| format!("{root:?}")));
            let mut text = "Choose repository | SESSION ONLY\nSelection does not register or persist.\nUp/Down choose | Enter select | Esc back\n".to_owned();
            let count = usize::from(height.saturating_sub(5)).max(1);
            for (i, entry) in entries
                .iter()
                .enumerate()
                .skip(selected.saturating_sub(count / 2))
                .take(count)
            {
                text.push_str(&format!(
                    "{} {entry}\n",
                    if i == *selected { ">" } else { " " }
                ));
            }
            if roots.is_empty() {
                text.push_str("No known roots; enter a path or use Create/attach.\n");
            }
            text
        }
        Overlay::Form(form) => {
            let draft = app.shell.draft(*form);
            let root =
                crate::services::resolve_root(std::path::Path::new(draft), &app.launch_directory);
            format!(
                "{}\nPath > {draft}\n\nResolved path: {root:?}\nLaunch directory: {:?}\nEnter: {} | Esc: back, retaining draft\nCtrl-U: discard draft | PgUp/PgDn: scroll\n{}",
                match form {
                    Form::Choose => "Choose exact root (session only)",
                    Form::Init => "Create/attach local repository",
                },
                app.launch_directory,
                if *form == Form::Choose {
                    "select (read-only)"
                } else {
                    "prepare read-only preview"
                },
                if *form == Form::Choose {
                    "No upward search, registration or configuration write."
                } else {
                    "Absent destination: create; existing valid Git+dort.toml: attach.\nExisting safe parent required. No tilde/environment expansion.\nNo writes until preview + literal yes; previews never authorize execution."
                }
            )
        }
    }
}
