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
const HELP: &str = "Left/Right or 1/2/3/4/5: Overview / Files / Recovery / Checks / Git\nTab / Shift-Tab: focus Destinations, Content, Actions\nEnter: activate focused control | Space: Actions\n/: search Files/Recovery metadata, or Actions when open | Delete: clear filter\np: choose repository for session | n: create/attach\nr: refresh current observations/check/diagnosis (never execute)\na: preview apply (Overview / Files)\nk / d: keep local file / delete managed link in Files\ni: file metadata details | Actions: enter exact mapping ID\nFile filters are display-only; apply always uses full active scope\nRecovery i/Enter: full IDs and contextual action hints\nb: preview rollback in Recovery | s / x: resume / abandon\nErrors: r retry read/fresh preview; h independent Recovery\nPreview: arrows / PgUp/PgDn scroll; Enter continue\nConfirmation: exact file ID for deletion, then literal yes\nEsc: confirmation -> preview -> retained form -> destination\nForms/search: printable q ? / are text; Backspace edits\nCtrl-U in a form: discard its draft | PgUp/PgDn: scroll\nq outside text / Ctrl-C anywhere: exit, joining worker\nNo automatic replay/rollback, network, staging or commits.\nf: Discover metadata | o: retained manual adoption form\nDiscover: Enter opens selected candidate as an editable draft\nAdoption: Tab/Shift-Tab fields/buttons; Enter advances fields\nt: temporary repository profiles | c: check repository manifest\nv: standalone manifest check (independent scope/profiles)\nProfiles union: base/default/platform/local plus temporary additions\nGit: g exact root | z Status | u Unstaged | s Staged\nr metadata refresh | / search | i details | w View patch content\nPatch: warning + literal view patch, once; Esc drops content\nEsc: return to previous destination and focus.";

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
        Tab::Checks => "Checks",
        Tab::Git => "Git",
    };
    frame.render_widget(Paragraph::new(safe(&format!(
        "Dort | 1 Overview  2 Files  3 Recovery  4 Checks  5 Git | [Actions: Space]\nRepository: {scope}\n{}{} | Profiles: {:?}\n{destination} | Focus: {} | ? Help | p Choose | n Create/attach", app.origin,
        if app.dry_run { " | DRY RUN / PREVIEW ONLY" } else { "" }, app.data.as_ref().map(|d| d.profiles.as_slice()).unwrap_or_default(), app.shell.focus.label()
    ))).style(Style::default().fg(Color::Cyan)), rows[0]);
    let body = if let Some(error) = &app.error {
        Some(format!(
            "{error}\n\nPgUp/PgDn: scroll | Esc: retained input/scope\nr: retry read/fresh preview (never replay approval)\nh: inspect all-repository Recovery\nAfter a failed write, r only refreshes; choose an action again for a new preview and confirmation."
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
        Some(overlay_text(app, overlay, rows[1].height, rows[1].width))
    } else if app.tab == Tab::Git {
        app.git_view.draw(frame, rows[1]);
        None
    } else if app.tab == Tab::Files {
        draw_files(frame, rows[1], app);
        None
    } else {
        let mut body = app
            .result
            .as_ref()
            .map(|result| format!("{result}\nEsc clears result.\n\n"))
            .unwrap_or_default();
        body.push_str(&match app.tab {
            Tab::Overview => app
                .data
                .as_ref()
                .map(|d| d.overview.clone())
                .unwrap_or_else(|| "Loading...".into()),
            Tab::Checks => check_text(app),
            Tab::Files | Tab::Git => unreachable!(),
            Tab::Recovery => super::recovery::text(app, rows[1].height),
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
    let footer = if app.error.is_some() {
        "r retry read/fresh preview | h Recovery | Esc back\nFailed write: refresh only; never replay approval"
    } else if app.preview.is_some() {
        "Preview: arrows/PgUp/PgDn scroll | Enter continue\nEsc back (no write) | q quit | Ctrl-C waits for worker"
    } else if app.tab == Tab::Git && app.shell.overlay.is_none() {
        "g root | z Status | u Unstaged | s Staged | r refresh\n/ search | i details | w View patch content | Esc back"
    } else if app.text_entry() {
        "Text: Enter advance/select | Backspace / Ctrl-U edit\nTab/Shift-Tab fields | Esc back | Ctrl-C quit"
    } else if app.tab == Tab::Recovery && app.shell.overlay.is_none() {
        "/ search | i/Enter details | b rollback | s resume\nx abandon | r diagnosis | Esc list | Space Actions"
    } else if app.tab == Tab::Checks && app.shell.overlay.is_none() {
        "j Diagnostics | m manifest result | c current check\nv standalone | r refresh | Space Actions | ? Help"
    } else if app.tab == Tab::Files && app.shell.overlay.is_none() {
        "/ search | Delete clear | i details | Space Actions\na full-scope apply | f discover | o adopt | q quit"
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
fn file_details(app: &App) -> String {
    let mut text = app
        .result
        .as_ref()
        .map(|r| format!("{r}\nEsc clears result.\n\n"))
        .unwrap_or_default();
    if app.discovering {
        let Some(discovery) = &app.discovery else {
            return text + "No discovery result loaded. Use Discover / Refresh.";
        };
        let report = &discovery.report;
        text.push_str(&format!(
            "Protected repository: {:?}\n",
            discovery.repository
        ));
        if let Some(candidate) = report
            .candidates
            .get(app.candidate_selected)
            .filter(|_| app.visible_file_rows().contains(&app.candidate_selected))
        {
            text.push_str(&format!(
                "Path: {:?}\nKind: {:?} | Link: {:?}\nMetadata eligibility: {} (not approval)\n",
                candidate.entry.path,
                candidate.entry.kind,
                candidate.link_state,
                candidate.supported_for_adoption
            ));
        } else {
            text.push_str("No displayed selection. Clear filter or use manual adoption.\n");
        }
        text.push_str("Enter: editable adoption draft | o: manual adoption\nBounded hidden HOME/.config scan; keys/caches/generated data excluded.\n");
        text.push_str(report.sensitivity_notice);
        for warning in &report.warnings {
            text.push_str(&format!("\nWarning: {warning}"));
        }
    } else if let Some(data) = &app.data {
        if let Some(error) = &data.files_error {
            text.push_str(&format!("Inventory unavailable: {error}\n"));
        } else if let Some((_, detail)) = data
            .files
            .get(app.selected)
            .filter(|_| app.visible_file_rows().contains(&app.selected))
        {
            text.push_str(detail);
        } else {
            text.push_str(
                "No displayed selection. Clear filter or enter exact mapping ID in Actions.",
            );
        }
        text.push_str("\nInventory is metadata, not execution authorization.\nApply always covers the full active-profile scope, not the filter.\nk: keep | d: delete link | Actions: enter exact mapping ID\nSources are retained by both unmanage modes.");
    }
    text.push_str("\nPgUp/PgDn: scroll details | Esc: back | Space Actions");
    text
}

fn draw_files(frame: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible_file_rows();
    let selected = app.file_selection();
    let search = app.inventory.search(app.discovering);
    let scope = if app.discovering {
        "Discover"
    } else {
        "Managed"
    };
    let wide = area.width >= 100;
    let panes = if wide {
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area)
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(area)
    };
    if wide || (!app.inventory.details && app.result.is_none()) {
        let pane = panes[0];
        let total = if app.discovering {
            app.discovery
                .as_ref()
                .map_or(0, |d| d.report.candidates.len())
        } else {
            app.data.as_ref().map_or(0, |d| d.files.len())
        };
        let summary = if app.discovering && app.discovery.is_none() {
            "Discovery not loaded | f refresh".into()
        } else if !app.discovering && app.data.is_none() {
            "Inventory not loaded".into()
        } else if !app.discovering && app.data.as_ref().is_some_and(|d| d.files_error.is_some()) {
            "Inventory unavailable".into()
        } else {
            format!(
                "{total} {} | {} shown",
                if app.discovering {
                    "metadata candidate(s)"
                } else {
                    "active mapping(s)"
                },
                visible.len()
            )
        };
        let mut list = format!(
            "{summary}\nFilter{}: {}\n",
            if search.editing { " [TEXT]" } else { "" },
            search.query
        );
        if !app.discovering
            && let Some(error) = app.data.as_ref().and_then(|d| d.files_error.as_ref())
        {
            list.push_str(&format!("Unavailable: {error}"));
        } else {
            let count = usize::from(pane.height.saturating_sub(5)).max(1);
            let position = visible.iter().position(|&i| i == selected).unwrap_or(0);
            for &index in visible
                .iter()
                .skip(position.saturating_sub(count / 2))
                .take(count)
            {
                let label = if app.discovering {
                    app.discovery
                        .as_ref()
                        .map(|d| {
                            format!(
                                "{:?} [{:?}]",
                                d.report.candidates[index].entry.path,
                                d.report.candidates[index].entry.kind
                            )
                        })
                        .unwrap_or_default()
                } else {
                    app.data
                        .as_ref()
                        .map(|d| d.files[index].0.clone())
                        .unwrap_or_default()
                };
                list.push_str(&format!(
                    "{} {label}\n",
                    if index == selected { ">" } else { " " }
                ));
            }
            if visible.is_empty() {
                list.push_str("No displayed rows. Delete: clear filter.");
            }
        }
        frame.render_widget(
            Paragraph::new(safe(&list)).block(
                Block::default()
                    .title(format!(" Files / {scope} | / search | i details "))
                    .borders(Borders::ALL),
            ),
            pane,
        );
    }
    if wide || app.inventory.details || app.result.is_some() {
        let pane = if wide { panes[1] } else { panes[0] };
        frame.render_widget(
            Paragraph::new(safe(&file_details(app)))
                .wrap(Wrap { trim: false })
                .scroll((app.details_scroll, 0))
                .block(
                    Block::default()
                        .title(" File metadata / result | Esc back ")
                        .borders(Borders::ALL),
                ),
            pane,
        );
    }
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

fn check_text(app: &App) -> String {
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

fn overlay_text(app: &App, overlay: &Overlay, height: u16, width: u16) -> String {
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
            "Dort {} | Help\nRecovery: / search all repositories; i/Enter full IDs and action hints\nChecks: j Diagnostics | m manifest result | Actions: About\n{HELP}",
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
                "{}\n{}\n\nFull draft: {draft:?}\nResolved path: {root:?}\nLaunch directory: {:?}\nEnter: {} | Esc: back, retaining draft\nCtrl-U: discard draft | PgUp/PgDn: scroll\n{}",
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
