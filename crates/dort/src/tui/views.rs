use super::{
    app::{App, Tab},
    shell::{self, Form, Overlay},
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
const HELP: &str = "1/2/3/4/5: Overview / Files / Recovery / Checks / Git\nTab / Shift-Tab: focus Content, Actions, Destinations\nLeft/Right: focused destination tabs or action buttons\nEnter: inspect row or activate focused button | Space: Actions\n/: search Files/Recovery metadata, or Actions when open | Delete: clear filter\np: choose repository for session | n: create/attach\nr: refresh current observations/check/diagnosis (never execute)\na: preview apply (Overview / Files)\nk / d: keep local file / delete managed link in Files\ni: file metadata details | Actions: enter exact mapping ID\nFile filters are display-only; apply always uses full active scope\nRecovery i/Enter: full IDs and contextual action hints\nb: preview rollback in Recovery | s / x: resume / abandon\nErrors: r retry read/fresh preview; h independent Recovery\nPreview: arrows / PgUp/PgDn scroll; Home resets; Enter continue\nConfirmation: exact file ID for deletion, then literal yes\nEsc: confirmation -> preview -> retained form -> destination\nForms/search: printable q ? / are text; Backspace edits\nCtrl-U in a form: clear current field | PgUp/PgDn: scroll\nHome: reset report/help scroll (not text fields)\nq outside text / Ctrl-C anywhere: exit, joining worker\nNo automatic replay/rollback, network, staging or commits.\nf: Discover metadata | o: retained manual adoption form\nDiscover: Enter inspects; Adopt selected is a separate action\nDirectories and other ineligible entries are metadata only\nAdoption: Tab/Shift-Tab fields/buttons; Enter advances fields\nt: temporary repository profiles | c: check repository manifest\nv: standalone manifest check (independent scope/profiles)\nProfiles union: base/default/platform/local plus temporary additions\nGit: g exact root | z Status | u Unstaged | s Staged\nr metadata refresh | / search | i details | w View patch content\nPatch: warning + literal view patch, once; Esc drops content\nEsc: return to previous destination and focus.";

pub fn draw(frame: &mut ratatui::Frame, app: &App) {
    super::workbench::draw(frame, app);
}
pub(super) fn short_field(label: &str, value: &str, width: u16) -> String {
    let escaped = safe(&format!("{value:?}"));
    let available = usize::from(width.saturating_sub(6))
        .saturating_sub(label.len())
        .max(4);
    let shown = if escaped.len() > available {
        format!("...{}", &escaped[escaped.len() - available + 3..])
    } else {
        escaped
    };
    format!("{label} > {shown}")
}

fn window_rows(title: &str, rows: &[String], selected: usize, height: u16, width: u16) -> String {
    let width = usize::from(width.saturating_sub(4));
    let mut text = format!(
        "{}\nItem {}/{} | Tab/Arrows | PgDn details\n",
        safe(title).chars().take(width).collect::<String>(),
        selected + 1,
        rows.len()
    );
    let count = usize::from(height.saturating_sub(5)).max(1);
    for (i, row) in rows
        .iter()
        .enumerate()
        .skip(selected.saturating_sub(count / 2))
        .take(count)
    {
        text.push_str(&format!(
            "{} {}\n",
            if i == selected { ">" } else { " " },
            safe(row).chars().take(width).collect::<String>()
        ));
    }
    if let Some(row) = rows.get(selected) {
        text.push_str(&format!("Selected (full): {row}\n"));
    } else {
        text.push_str("No matching entries.\n");
    }
    text
}

fn adoption_text(app: &App, height: u16, width: u16) -> String {
    let draft = &app.shell.adoption;
    let rows = [
        short_field("TARGET", &draft.target, width),
        short_field("ID", &draft.id, width),
        short_field("SOURCE", &draft.source, width),
        "Review read-only preview".into(),
        "Choose repository (session only)".into(),
        "Create/attach local repository".into(),
        "Discard adoption draft".into(),
    ];
    let mut text = window_rows(
        "Adopt ordinary file | base-profile link",
        &rows,
        draft.focused,
        height,
        width,
    );
    text.push_str(&format!(
        "\nFull TARGET: {:?}\nFull ID: {:?}\nFull SOURCE: {:?}\n",
        draft.target, draft.id, draft.source
    ));
    text.push_str(&format!(
        "\nSelected repository: {:?}\nEffective profiles: {:?}\n",
        app.data
            .as_ref()
            .and_then(|d| d.repository.as_ref())
            .or(app.repository.as_ref()),
        app.data.as_ref().map(|d| &d.profiles)
    ));
    text.push_str("Tab/Shift-Tab: fields/buttons | Enter: advance field or activate button\nEsc: back, retaining draft | Ctrl-U: clear current field | PgUp/PgDn: scroll\nMissing repository? Choose or Create/attach above, then return to this draft.\nSOURCE is repository-relative. Review content privately before adoption.\nOnly owned, single-link ordinary files; core refusals are authoritative.\nNo contents displayed. No writes before fresh preview and literal yes.");
    text
}

pub(super) fn check_text(app: &App) -> String {
    if app.showing_diagnostics {
        return diagnostics_text(app);
    }
    let mut text =
        "Checks | c: current repository | v: standalone manifest | t: repository profiles\n"
            .to_owned();
    text.push_str(
        "j: Diagnostics (host, paths, registry and write blockers) | m: manifest result\n",
    );
    if let Some((report, standalone)) = &app.check {
        text.push_str(&format!("Scope: {}\nValid manifest: {:?}\nSchema: {}\nEffective profiles: {:?}\nSelected: {} tools, {} files\nTool dependency order: {:?}\n",
            if *standalone { "standalone manifest (independent of file repository)" } else { "current repository" },
            report.manifest, report.schema, report.active_profiles, report.selected_tools, report.selected_files, report.tool_dependency_order));
        for capability in &report.unsupported_capabilities {
            text.push_str(&format!("Not implemented: {capability}\n"));
        }
        text.push_str("Read-only structural check; live files and installed tools were not inspected.\nPgUp/PgDn: scroll | r: recheck this scope");
    } else {
        text.push_str("Choose Current repository or Standalone manifest in Actions.\nStandalone needs no repository, registration or Git. No tool execution or file observation.");
    }
    text
}

fn diagnostics_text(app: &App) -> String {
    let Some(report) = &app.diagnostics else {
        return "Diagnostics not loaded. j: read host, paths and registry metadata".into();
    };
    let host = &report.host;
    let mut text = format!(
        "Diagnostics | Dort {} | read-only\nPlatform: {:?} / {:?}\n",
        env!("CARGO_PKG_VERSION"),
        host.platform,
        host.architecture
    );
    let paths = format!(
        "HOME: {:?}\nConfig: {:?}\nState: {:?}\nSession file root: {:?}\nSelection origin: {}\nGit executable: {:?}\nGit session root: {:?} (follows file root: {})\n",
        host.home,
        host.config_dir,
        host.state_dir,
        app.data
            .as_ref()
            .and_then(|d| d.repository.as_ref())
            .or(app.repository.as_ref()),
        app.origin,
        app.git,
        app.git_view.root,
        app.git_view.follows_files
    );
    match &report.registry {
        Ok(registry) => text.push_str(&format!("Registered repositories: {}\nRegistered active ID: {:?}\n", registry.registered_repositories, registry.active_repository)),
        Err(error) => text.push_str(&format!("Configuration unavailable: {error}\nRegistry count and active selection unknown (not zero).\n")),
    }
    match &report.history {
        Ok((count, blockers)) => text.push_str(&format!(
            "All-repository transactions: {count}\nGlobal write blockers: {blockers}\n"
        )),
        Err(error) => text.push_str(&format!(
            "History unavailable: {error}\nGlobal write safety unknown.\n"
        )),
    }
    text.push_str(&paths);
    text.push_str("\nNo directories, locks or repairs created. No local variable values or file contents displayed.\nSupported: guarded local file workflows and recovery; structural checks; hardened read-only Git.\nUnavailable: providers, templates, secrets, cloning/sync, network and Git writes. Live files/tools not inspected by Diagnostics.\nGit requires separately installed trusted Git; missing Git does not disable other workflows.\nConfiguration errors: correct the reported configuration privately, then r to retry. Recovery remains independent.\nPgUp/PgDn: scroll | r refresh Diagnostics | m manifest result | Space: About / Help");
    text
}

fn profiles_text(picker: &shell::ProfilePicker, height: u16, width: u16) -> String {
    let choices = &picker.report.choices;
    let effective: std::collections::BTreeSet<_> =
        choices.mandatory.iter().chain(&picker.requested).collect();
    let contributions = format!(
        "Temporary profiles | {}\nManifest: {:?}\nBase: always active | Defaults: {:?}\nPlatform: {:?} | Local: {:?}\nRequested additions: {:?}\nEffective union: {:?}\nOR selection; mandatory contributors cannot be unchecked.\n",
        if picker.standalone {
            "standalone manifest only"
        } else {
            "repository file/check scope"
        },
        picker.report.manifest,
        choices.defaults,
        choices.platform,
        choices.local,
        picker.requested,
        effective
    );
    let rows: Vec<_> = choices
        .known
        .iter()
        .map(|name| {
            format!(
                "[{}] {name}{}",
                if effective.contains(name) { "x" } else { " " },
                if choices.mandatory.contains(name) {
                    " (mandatory)"
                } else {
                    ""
                }
            )
        })
        .chain(["Apply additions (read-only validation)".into()])
        .collect();
    let mut text = window_rows(
        if picker.standalone {
            "Profiles | standalone manifest"
        } else {
            "Profiles | repository scope"
        },
        &rows,
        picker.selected,
        height,
        width,
    );
    text.push_str(&contributions);
    text.push_str("Up/Down: choose | Space/Enter: toggle optional | Enter on Apply\nEsc: back without changing scope | PgUp/PgDn: details | Never persisted.");
    text
}

pub(super) fn overlay_text(app: &App, overlay: &Overlay, height: u16, width: u16) -> String {
    match overlay {
        Overlay::Profiles(picker) => profiles_text(picker, height, width),
        Overlay::Form(form @ (Form::KeepId | Form::RemoveId)) => {
            let rows = [
                short_field("Exact ID", &app.shell.unmanage_id, width),
                "Review read-only preview".into(),
            ];
            let mut text = window_rows(
                if *form == Form::KeepId {
                    "Keep local file | enter mapping ID"
                } else {
                    "Delete managed link | enter mapping ID"
                },
                &rows,
                app.shell.unmanage_focused,
                height,
                width,
            );
            text.push_str(&format!("\nFull ID: {:?}\nTab: field/review | Enter: advance or review\nFilters never constrain this input. Core validates exact ID and scope.\nBoth modes retain source. Keep materializes an ordinary file; delete removes the link.\nDeletion additionally needs a separate exact-ID acknowledgement then literal yes.\nEsc returns without writing; draft retained.\n", app.shell.unmanage_id));
            text
        }
        Overlay::Form(Form::Check) => {
            let rows = [
                short_field("Manifest path", &app.shell.manifest_draft, width),
                "Check manifest (read-only)".into(),
                "Temporary profiles for this manifest".into(),
                "Discard standalone draft and additions".into(),
            ];
            let mut text = window_rows(
                "Check standalone manifest",
                &rows,
                app.shell.manifest_focused,
                height,
                width,
            );
            text.push_str(&format!("\nFull path: {:?}\nRequested additions: {:?}\nRelative path base: {:?}\nTab/Shift-Tab fields/buttons; Enter advances/activates\nEsc back, retaining draft | Ctrl-U clear path | PgUp/PgDn scroll\nNo repository/registration/Git needed. Local profiles still contribute.\nStructural/dependency/capability check only; live files/tools not inspected.", app.shell.manifest_draft, app.shell.check_profiles, app.launch_directory));
            text
        }
        Overlay::Help => format!(
            "Dort {} | Help\nSpace: Actions | / there: search by label\n1..5: views | Tab: focus | Esc: back\n{HELP}\nChecks: j Diagnostics | m manifest result | Actions: About",
            env!("CARGO_PKG_VERSION")
        ),
        Overlay::About => format!(
            "Dort {} | About\nGit-native developer environment manager\nExperimental guarded local workflows. MIT license.\n\nSupported hosts: macOS and Ubuntu, ARM64 / x86_64.\nLocal create/attach, discovery/adoption, link apply/unmanage, guarded recovery, structural checks and read-only Git.\n\nNot implemented: providers/package managers, templates, secrets, cloning/sync, network, Git staging/commit/push.\nPreviews are not authorization. Failed writes may retain partial effects; inspect Recovery. No automatic rollback/repair.\nGit patches require a separate one-view disclosure; not secret-proof.\nBackups are private, not encrypted. Process-exit tests do not prove power-loss safety.\n\nEsc/Enter: return to prior destination and focus | PgUp/PgDn: scroll",
            env!("CARGO_PKG_VERSION")
        ),
        Overlay::Actions {
            query,
            selected,
            searching,
        } => {
            let entries: Vec<_> = shell::actions(query)
                .iter()
                .map(|action| {
                    let eligibility = match action {
                        shell::Action::Rollback => Some(
                            app.tab == Tab::Recovery
                                && app
                                    .history_selection()
                                    .is_some_and(super::recovery::rollback),
                        ),
                        shell::Action::Resume => Some(
                            app.tab == Tab::Recovery
                                && app.history_selection().is_some_and(super::recovery::resume),
                        ),
                        shell::Action::Abandon => Some(
                            app.tab == Tab::Recovery
                                && app
                                    .history_selection()
                                    .is_some_and(super::recovery::abandon),
                        ),
                        _ => None,
                    };
                    match eligibility {
                        Some(true) => format!("{} [core preview required]", action.label()),
                        Some(false) => {
                            format!("{} [unavailable; inspect diagnosis]", action.label())
                        }
                        None => action.label().to_owned(),
                    }
                })
                .collect();
            let mut text = window_rows(
                if *searching {
                    "Actions | TEXT ENTRY"
                } else {
                    "Actions | Enter choose"
                },
                &entries,
                *selected,
                height,
                width,
            );
            text.push_str(&format!(
                "Search: {query}\n/: edit search | Esc: leave search/back | Delete: clear\n"
            ));
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
            let mut text = window_rows(
                "Choose repository | SESSION ONLY",
                &entries,
                *selected,
                height,
                width,
            );
            text.push_str("Selection does not register or persist.\nUp/Down choose | Enter select | Esc back\n");
            if roots.is_empty() {
                text.push_str("No known roots; enter a path or use Create/attach.\n");
            }
            text
        }
        Overlay::Form(Form::Adopt) => adoption_text(app, height, width),
        Overlay::Form(form) => {
            let draft = app.shell.draft(*form);
            let root =
                crate::services::resolve_root(std::path::Path::new(draft), &app.launch_directory);
            format!(
                "{}\n{}\n\nFull draft: {draft:?}\nResolved path: {root:?}\nLaunch directory: {:?}\nEnter: advance field, then {} | Esc: back, retaining draft\nCtrl-U: discard draft | PgUp/PgDn: scroll\n{}",
                match form {
                    Form::Choose => "Choose exact root (session only)",
                    Form::Init => "Create/attach local repository",
                    Form::Adopt | Form::Check | Form::KeepId | Form::RemoveId => unreachable!(),
                },
                short_field("Path", draft, width),
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
