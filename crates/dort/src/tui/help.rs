//! Metadata-only Help layer. It never moves/clones a draft, approval or private payload.
use super::{
    app::{App, Tab},
    components as ui,
    shell::{Form, Overlay},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
};

#[derive(Clone, Copy)]
pub enum Topic {
    Overview,
    Discover,
    Content,
    Managed,
    Recovery,
    Checks,
    Diagnostics,
    Git,
    GitContent,
    Actions,
    Form(Form),
    Picker,
    Ignored,
    About,
    Batch,
    BatchItem,
    Preview,
    Confirmation,
    Task,
    Error,
    Result,
}
impl Topic {
    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Discover => "Discover files",
            Self::Content => "Discover content pane",
            Self::Managed => "Managed files",
            Self::Recovery => "Recovery",
            Self::Checks => "Manifest checks",
            Self::Diagnostics => "Diagnostics",
            Self::Git => "Git inspection",
            Self::GitContent => "Git content disclosure",
            Self::Actions => "Actions",
            Self::Form(_) => "Form",
            Self::Picker => "Session picker",
            Self::Ignored => "Ignored paths / Unignore",
            Self::About => "About",
            Self::Batch => "Marked adoption",
            Self::BatchItem => "Edit batch item",
            Self::Preview => "Review preview",
            Self::Confirmation => "Confirmation",
            Self::Task => "Active task",
            Self::Error => "Error",
            Self::Result => "Completion",
        }
    }
    fn instructions(self) -> &'static str {
        match self {
            Self::Overview => {
                "Choose repository selects only this session. Create/attach needs a fresh preview and literal yes. Discover works without a repository. Apply reviews the full active-profile scope."
            }
            Self::Discover => {
                "j/k or arrows select; h/l folds; gg/G first/last. Ctrl-F/B pages; Ctrl-D/U half pages. Space/x marks eligible files or matching descendants. e adopts selected; E edits marked files.\nIgnore selected previews a persistent exact HOME-relative exclusion for a file/folder. Literal yes saves preferences only, never changes live files. Ignored paths opens the list: Enter previews Unignore, then literal yes restores discovery visibility.\nTab/Shift-Tab or click switches list/content focus. Enter folds/inspects; i details. / searches metadata only; filters/folds retain marks. ; opens all commands for this context."
            }
            Self::Content => {
                "Tab/Shift-Tab returns list focus; Esc returns the compact list. j/k, gg/G and Ctrl-F/B/D/U scroll wrapped content without moving the selected file. PgUp/PgDn scrolls content; Alt+PgUp/PgDn metadata.\nOnly eligible selected files are read, bounded to 64 KiB. Sensitive/binary/unsafe content is withheld; heuristics are not proof of secrecy. Help withholds the private preview; returning can reread the current eligible file."
            }
            Self::Managed => {
                "j/k or arrows select; h/l folds; gg/G first/last. Ctrl-F/B pages; Ctrl-D/U half pages. Space/x marks an item/group, including folded descendants. Enter/i inspects.\nApply always uses the full active-profile scope. A/K/D request marked apply/keep/delete. Single Keep/Delete and exact-ID drafts are in Actions. Keep materializes a local file; Delete removes only the link. Sources remain. / filters display only; marks survive filters."
            }
            Self::Recovery => {
                "All repositories, independent of selected file scope. Up/Down selects; Enter/i shows full raw IDs and eligibility. b rollback, s resume, x abandon request read-only previews.\nRollback: eligible schema-1/2/4. Resume: intact Running bootstrap only. Preparing can be abandoned, not resumed. Abandon never deletes a published repository. Global blockers remain visible despite / display search. No automatic replay or repair."
            }
            Self::Checks => {
                "c checks the current repository; v opens an independent standalone manifest form. t chooses temporary repository profiles; standalone additions are separate. j opens Diagnostics; m returns to manifest results.\nStructural/dependency checks are not live file/tool inspection. Profiles combine mandatory contributors and validated session additions; choices never save configuration."
            }
            Self::Diagnostics => {
                "Read-only host, registry and history metadata; unavailable observations are unknown, not zero. m returns to manifest results. PgUp/PgDn scrolls; Home returns top. No local variable values or file bodies are included."
            }
            Self::Git => {
                "g chooses an exact independent root; z Status, u Unstaged, s Staged. r refreshes metadata, / filters display, i inspects. w opens a separate patch content warning after a comparison summary.\nNo staging, commits, network, hooks or external diff. Summary/status stays metadata-only."
            }
            Self::GitContent => {
                "Patch viewing requires the separate literal view patch acknowledgement, once. Patches may expose secrets and are not secret-proof. PgUp/PgDn and arrows scroll the disclosed patch; Esc drops it.\nHelp never copies or renders the retained disclosure input/payload. Shrinking below the layout floor drops disclosure; returning from Help never restores disposed content or authorizes another read."
            }
            Self::Actions => {
                "One searchable panel: Context sections contain selected-item, marked-items and current-view commands. Global contains destination and session controls. Other-view workflows stay in their views.\nType a label immediately; Up/Down chooses, Enter requests. / refocuses search. Esc leaves search, then closes. PgDn shows the full label, effect and unavailable reason; Home returns controls. Search is display-only. Effects are independent of scope: drafts, mutation previews and content warnings never authorize execution."
            }
            Self::Form(Form::Adopt) => {
                "TARGET is the exact local path; ID is the mapping identifier; SOURCE is repository-relative. Defaults preserve the HOME-relative path under files/. Edit freely before review. Enter advances through fields to Review; Tab/Shift-Tab moves controls. Choose/Create can retain this draft. Esc returns retaining edits. Only the real preview's Adopt all approves adoption; no typed yes."
            }
            Self::Form(Form::KeepId | Form::RemoveId) => {
                "Enter the exact mapping ID, independent of display filters. Enter advances to Review, then requests a read-only preview. Keep materializes an ordinary local file; Delete removes the link, retaining source. Deletion needs a separate exact-ID acknowledgement before literal yes."
            }
            Self::Form(Form::Check) => {
                "Enter an exact manifest path, relative to the supplied launch directory if not absolute. No repository or Git required. Tab/Shift-Tab moves to Check or temporary standalone profiles. Enter advances/activates. Esc retains the draft and additions."
            }
            Self::Form(Form::Choose) => {
                "Enter an exact repository root. Relative paths resolve from the launch directory, without tilde/environment expansion. Selection is read-only and session-only, not registration. Enter advances to the selection button; Esc retains the draft."
            }
            Self::Form(Form::Init) => {
                "Missing destination creates a local repository; an existing valid Git root with dort.toml attaches. Existing safe parent required. Relative paths use the launch directory; no tilde/environment expansion. Enter advances then requests a read-only preview. Only separate literal yes can approve registration/publication."
            }
            Self::Picker => {
                "Up/Down chooses; Enter selects/activates the typed row. Profile choices are temporary additions, not saved configuration. Tab moves controls; PgDn shows full identity/details; Home returns controls. Esc returns without writing."
            }
            Self::Ignored => {
                "Up/Down selects an exact HOME-relative ignored path. Enter requests an Unignore preview; literal yes saves the removal from local preferences. Esc returns without writing. Ignoring/unignoring never changes live files or repository mappings. Discovery omits descendants of ignored folders; unignoring a child cannot override an ignored parent."
            }
            Self::About => {
                "Capabilities and limitations are read-only session information. PgUp/PgDn scrolls; Home top; Esc returns to the previous destination."
            }
            Self::Batch => {
                "j/k or arrows selects an item; e/Edit item opens a separate TARGET/ID/SOURCE form. Review all is initially focused: Enter reviews all marked files using their editable defaults. Tab moves controls. Esc returns to Discover retaining drafts/marks. No individual preview/write is requested by item editing."
            }
            Self::BatchItem => {
                "Enter advances TARGET, ID, SOURCE, then Done. Tab/Shift-Tab or clicks move fields. Ctrl-U clears the current field. Done or Esc returns to the combined editor with in-memory edits retained. Only Review all requests the combined core preview; only Adopt all approves it."
            }
            Self::Preview => {
                "Review typed counts and per-mapping effects. Up/Down selects mappings; i toggles full report. PgUp/PgDn scrolls; Home resets both axes. Esc returns without writing, retaining drafts.\nAdoption uses one explicit Adopt all activation (Enter/click), without typed yes. Other writes: Enter opens confirmation, then literal yes. Deletion first requires each exact ID separately. Blocked, preview-only and below-floor layouts cannot approve."
            }
            Self::Confirmation => {
                "Type each required exact deletion ID separately, then literal yes and Enter. No mouse approval. Esc cancels to the preview. PgUp/PgDn scrolls the prompt independently of input.\nHelp retains the real pending preview and input but cannot submit it. Enter, yes and clicks in Help do nothing. Shrinking below 48x14 clears the entire acknowledgement sequence even while Help is open. Execution independently revalidates under the writer lock."
            }
            Self::Task => {
                "The owned worker continues while Help is open. Progress is metadata-only and reports actual boundaries, not a percentage. q/Ctrl-C requests exit after completion; no cooperative cancellation, detached worker or automatic rollback. Completion may update the underlying screen while Help stays open."
            }
            Self::Error => {
                "Cause and retained scope are separate. r retries only a read or fresh preview; h opens independent Recovery; Esc returns. Failed writes may have partial effects: inspect Recovery before another write. No automatic undo. The earlier-write warning remains for the session."
            }
            Self::Result => {
                "Completion retains the transaction ID or explicit no-op outcome. Refresh rereads observations; Recovery inspects history; Back returns. A receipt is metadata, never approval or proof that later rollback is safe."
            }
        }
    }
}
pub struct State {
    pub topic: Topic,
    pub scroll: u16,
    pub content_focus: bool,
    pub content_scroll: u16,
}
pub fn topic(app: &App) -> Topic {
    if app.error.is_some() {
        return Topic::Error;
    }
    if app.busy {
        return Topic::Task;
    }
    if app.confirmation.is_some() {
        return Topic::Confirmation;
    }
    if app.preview.is_some() {
        return Topic::Preview;
    }
    if app.show_result {
        return Topic::Result;
    }
    if let Some(e) = app.batch_editor.as_ref().filter(|e| e.open) {
        return if e.item_form {
            Topic::BatchItem
        } else {
            Topic::Batch
        };
    }
    if let Some(o) = &app.shell.overlay {
        return match o {
            Overlay::Actions { .. } => Topic::Actions,
            Overlay::Form(f) => Topic::Form(*f),
            Overlay::Ignored { .. } => Topic::Ignored,
            Overlay::About => Topic::About,
            _ => Topic::Picker,
        };
    }
    match app.tab {
        Tab::Overview => Topic::Overview,
        Tab::Files if app.discovering && app.content.focused => Topic::Content,
        Tab::Files if app.discovering => Topic::Discover,
        Tab::Files => Topic::Managed,
        Tab::Recovery => Topic::Recovery,
        Tab::Checks if app.showing_diagnostics => Topic::Diagnostics,
        Tab::Checks => Topic::Checks,
        Tab::Git if app.git_view.sensitive() => Topic::GitContent,
        Tab::Git => Topic::Git,
    }
}
/// Text focus is surface-specific; background retained forms do not steal task/error Help.
pub fn text_input(app: &App) -> bool {
    if app.busy
        || app.error.is_some()
        || app.show_result
        || app.preview.is_some() && app.confirmation.is_none()
    {
        return false;
    }
    if app.confirmation.is_some() {
        return true;
    }
    if let Some(e) = app.batch_editor.as_ref().filter(|e| e.open) {
        return if e.item_form {
            e.focused < e.item * 3 + 3
        } else {
            e.focused < e.drafts.len() * 3
        };
    }
    app.text_entry()
}
pub fn draw(frame: &mut Frame, app: &App, help: &State) {
    let area = frame.area();
    app.hits.clear();
    let mut lines = vec![
        Line::styled(
            format!("Dort | Help: {}", help.topic.title()),
            ui::heading(),
        ),
        Line::raw("Esc / F1: return exactly | PgUp/PgDn: scroll | Home: top"),
        Line::raw(""),
    ];
    lines.extend(
        help.topic
            .instructions()
            .split('\n')
            .map(|s| Line::raw(s.to_owned())),
    );
    lines.push(Line::raw(""));
    if matches!(
        help.topic,
        Topic::Overview
            | Topic::Discover
            | Topic::Managed
            | Topic::Recovery
            | Topic::Checks
            | Topic::Diagnostics
            | Topic::Git
    ) {
        lines.push(Line::styled("Context commands (; Actions)", ui::heading()));
        for e in super::context::entries(app)
            .into_iter()
            .filter(|e| e.scope != super::context::Scope::Global)
        {
            lines.push(Line::raw(format!(
                "{}: {} [{}]{}",
                e.scope.short(),
                e.action.label(),
                e.effect.label(),
                app.action_reason(e.action)
                    .map(|r| format!(" - {r}"))
                    .unwrap_or_default()
            )));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw("F1 opens contextual Help everywhere. ? opens it outside text; q ? / remain literal in text/search. Ctrl-U clears a field. Global: 1..5 destinations, p repository, n create/attach, ; Actions. q outside text / Ctrl-C exits, waiting for the worker."));
    lines.push(Line::raw("Previews are not authorization. No automatic replay/rollback, provider execution, network or Git writes. Backups are private, not encrypted."));
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    ui::report(
        frame,
        body,
        "Context Help",
        Text::from(lines),
        help.scroll,
        true,
    );
    app.hits.wheel(body, super::input::Wheel::Overlay);
    let back = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    ui::prose(
        frame,
        back,
        "[Esc / F1 Back]  Help never submits or approves",
        ui::accent(),
    );
    app.hits.click(back, super::input::Target::Back);
}
