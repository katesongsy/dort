//! Content-free history presentation. Eligibility is advisory; core previews decide.
use dort_core::journal::{Diagnosis, Phase};

pub fn rollback(row: &Diagnosis) -> bool {
    row.problem.is_none()
        && matches!(row.journal_schema, Some(1 | 2))
        && row.phase.is_some_and(|p| p != Phase::RolledBack)
}
pub fn abandon(row: &Diagnosis) -> bool {
    row.problem.is_none()
        && row.journal_schema == Some(3)
        && row.phase.is_some_and(|p| !p.terminal())
}
pub fn resume(row: &Diagnosis) -> bool {
    abandon(row) && row.phase == Some(Phase::Running)
}
pub fn matches(search: &super::inventory::Search, row: &Diagnosis) -> bool {
    search.matches(&row.repository_id)
        || search.matches(&row.transaction_id)
        || search.matches(&format!(
            "schema {:?} phase {:?} checkpoints {} {} {}",
            row.journal_schema,
            row.phase,
            row.valid_checkpoints,
            if row.blocks_writes() {
                "blocks writes"
            } else {
                "terminal"
            },
            row.problem.as_deref().unwrap_or("validated")
        ))
}

pub fn details(row: &Diagnosis) -> String {
    format!(
        "Repository ID: {}\nTransaction ID: {}\nSchema: {:?} | Phase: {:?}\nValidated checkpoints: {}\nIntegrity: {}\nBlocks new writes: {}\n\nActions (eligibility hints only):\n{}\n{}\n{}\n\nCore preview independently checks live paths and artifacts. Completed history is not proof of safe rollback. No automatic recovery or repair. Schema-1 rollback withdraws its new source into private storage; schema-2 leaves repository sources unchanged. Backups/private artifacts are retained; no garbage collection.\nPgUp/PgDn: full details | Esc: list",
        row.repository_id,
        row.transaction_id,
        row.journal_schema,
        row.phase,
        row.valid_checkpoints,
        row.problem
            .as_deref()
            .unwrap_or("validated journal; live drift not inspected"),
        row.blocks_writes(),
        if rollback(row) {
            "b: Preview rollback (schemas 1/2)"
        } else {
            "Rollback unavailable for this diagnosis."
        },
        if resume(row) {
            "s: Preview bootstrap resume"
        } else if abandon(row) && row.phase == Some(Phase::Preparing) {
            "Resume unavailable: incomplete preparation; abandon, then create/attach a new transaction."
        } else {
            "Resume unavailable: not an intact nonterminal bootstrap."
        },
        if abandon(row) {
            "x: Preview abandon; retains published repository, registration and private artifacts."
        } else {
            "Abandon unavailable: not an intact nonterminal bootstrap."
        }
    )
}

pub fn text(app: &super::app::App, height: u16) -> String {
    let Some(data) = &app.data else {
        return "Recovery diagnosis not loaded. r: read all-repository history".into();
    };
    if let Some(error) = &data.history_error {
        return format!(
            "Recovery diagnosis unavailable: {error}\nGlobal write safety is unknown, not healthy empty history.\nr: retry diagnosis | No automatic repair."
        );
    }
    let visible = app.visible_history_rows();
    let mut text = format!(
        "All repositories: {} transactions | {} global write blockers\nShowing {} | / search | Delete clear | i/Enter details\nFilter: {:?}{}\n",
        data.history.len(),
        data.history.iter().filter(|r| r.blocks_writes()).count(),
        visible.len(),
        app.history_search.query,
        if app.history_search.editing {
            " (editing; Enter/Esc ends)"
        } else {
            ""
        }
    );
    if app.history_details {
        if let Some(row) = app.history_selection() {
            text.push_str(&details(row));
        } else {
            text.push_str("No displayed transaction selected. Clear the filter.");
        }
        return text;
    }
    if data.history.is_empty() {
        text.push_str("No retained transactions. Diagnosis is read-only; no state created.");
    } else if visible.is_empty() {
        text.push_str("No matches. Global blockers above are never filtered away.");
    } else {
        let count = usize::from(height.saturating_sub(7)).max(1);
        let position = visible.iter().position(|&i| i == app.selected).unwrap_or(0);
        for &index in visible
            .iter()
            .skip(position.saturating_sub(count / 2))
            .take(count)
        {
            let row = &data.history[index];
            text.push_str(&format!(
                "{} {} schema {:?} {:?} | {} checkpoints | {}\n",
                if index == app.selected { ">" } else { " " },
                row.transaction_id.get(..8).unwrap_or("invalid"),
                row.journal_schema,
                row.phase,
                row.valid_checkpoints,
                if row.problem.is_some() {
                    "INTEGRITY PROBLEM"
                } else if row.blocks_writes() {
                    "BLOCKS WRITES"
                } else {
                    "terminal"
                }
            ));
        }
        text.push_str("Select a row, then i/Enter for full IDs and contextual actions; no hash transcription.");
    }
    text
}
