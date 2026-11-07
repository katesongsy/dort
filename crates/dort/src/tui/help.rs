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
    Menu,
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
            Self::Menu => "Navigation menus",
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
            Self::Menu => {
                "Menus organize navigation and session commands. Left/Right selects a menu; Up/Down chooses an item; Enter opens it, or press its labeled shortcut. In compact layouts choose a group first; Left returns to groups. Esc closes. Unavailable entries show the requirement to continue. Operation and patch commands open their respective review screens."
            }
            Self::Overview => {
                "Overview summarizes repository, profiles and file state. Session selects a repository or creates/attaches one. Files offers discovery and managed mappings; Checks validates declarations; Recovery inspects retained transactions; Git shows status and diffs."
            }
            Self::Discover => {
                "Up/Down selects a path; h/l folds directories. Space marks eligible files, including matching folded descendants. Enter inspects metadata. Adopt selected edits target, ID and source; E reviews marked adoption. I reviews Ignore selected; Files > Ignored paths manages saved exclusions. / filters the list."
            }
            Self::Content => {
                "Tab switches the file list and content pane. Up/Down scrolls the focused pane; Ctrl-F/B pages and gg/G moves to the ends. Line numbers identify source lines; continuation rows share a gutter. Configuration keys are emphasized. Content reads are bounded; sensitive/binary files are withheld. Help and menus release content; returning rereads the selected eligible file."
            }
            Self::Managed => {
                "Up/Down selects; h/l folds; gg/G moves to the ends. Space marks an item or group, including folded descendants. Enter inspects. u reviews Keep for the selected mapping. Apply uses the full active-profile scope; A/K/D review marked apply/keep/delete. Keep materializes an ordinary local file; Delete removes the managed link. Repository sources remain. / filters the display."
            }
            Self::Recovery => {
                "Recovery lists transactions across repositories. Select one to inspect history, integrity and eligibility. Rollback, Resume and Abandon each request a fresh core preview. Review exact paths and effects before approval. Later edits may block execution. Abandon retains published repositories, registration and private artifacts."
            }
            Self::Checks => {
                "Manifest checks inspect declarations and effective profiles. Check current validates the selected repository; Check manifest accepts an independent path. Profiles adds session contributors to the mandatory profile union. Diagnostics shows host, paths and registry observations."
            }
            Self::Diagnostics => {
                "Diagnostics summarizes the host, capabilities, resolved paths and registry metadata. Unavailable observations carry their cause. Refresh diagnostics retries the read. Live file inspection belongs in Files; installed tool readiness requires your own verification."
            }
            Self::Git => {
                "Choose Status, Unstaged or Staged in the Git menu. Choose Git scope selects an independent root; Follow selected repository restores repository scope. / searches metadata; Enter inspects; View patch opens a content warning. Manage commits, branches and synchronization with your preferred Git tool."
            }
            Self::GitContent => {
                "A patch may expose secrets. Type literal view patch once to disclose the requested comparison. Headers and hunks are emphasized; + marks additions and - marks removals. PgUp/PgDn scrolls vertically; Left/Right scrolls horizontally. Esc releases the view. Help hides the retained view; shrinking below the layout floor releases it. Released bytes may remain in memory."
            }
            Self::Actions => {
                "Actions contains operations for the current view, selection and marked set. Menus organize navigation and session controls. Type a label to search; Up/Down selects; Enter requests. / refocuses search. Esc leaves search, then closes. Selected action details stay below the list. PgUp/PgDn scroll that pane; Home goes to its top. Execution scope comes from the operation's review."
            }
            Self::Form(Form::Adopt) => {
                "Enter advances TARGET, ID and SOURCE, then requests a review. Tab moves controls. SOURCE is repository-relative. Inspect contents privately before adoption. Esc retains the draft. Explicit Adopt all on the combined preview approves adoption."
            }
            Self::Form(Form::KeepId | Form::RemoveId) => {
                "Enter an exact mapping ID and request its preview. Keep materializes an ordinary file; Delete removes the managed link. Both retain the repository source. Deletion requires exact-ID acknowledgement followed by literal yes."
            }
            Self::Form(Form::Check) => {
                "Enter a manifest path and optional profile additions. Relative paths resolve from the launch directory. Enter requests a structural check. Local profiles contribute to the effective union. Esc retains the draft."
            }
            Self::Form(Form::Choose) => {
                "Enter an exact repository root. Selection stays in this session. Relative paths resolve from the launch directory; enter literal paths. Esc retains the draft."
            }
            Self::Form(Form::Init) => {
                "Enter a repository path and ID. An absent destination creates a repository; an existing valid Git repository with dort.toml attaches it. Review effects, then type literal yes to approve persistent registration and selection. Failed preparation retains private artifacts for Recovery."
            }
            Self::Picker => {
                "Up/Down selects a repository or profile; Enter activates it. Repository choices update session scope. Profile additions join mandatory contributors. Esc returns with your previous draft intact."
            }
            Self::Ignored => {
                "Saved HOME-relative exclusions prune matching paths and descendants from discovery. Select a path and request Unignore to review removal of that exclusion. Type literal yes to approve. Live files remain in place. Esc returns."
            }
            Self::About => {
                "About summarizes Dort's local workflows, host support and license. Diagnostics provides host and path metadata."
            }
            Self::Batch => {
                "Up/Down selects a marked file; e opens its TARGET, ID and SOURCE form. Review all is initially focused: Enter reviews the complete marked set. Tab moves controls. Selected-item paths remain inline; PgUp/PgDn scrolls them and Home goes to their top. Esc retains drafts and marks."
            }
            Self::BatchItem => {
                "Enter advances TARGET, ID and SOURCE; Done returns to the marked set. Tab moves fields. Esc retains edits. Review all requests the combined preview."
            }
            Self::Preview => {
                "Review outcome, paths, ordered effects and blockers. File reviews pair a summary table with numbered change details; i opens the full report. Up/Down selects a file; PgDn scrolls details. Enter continues to approval. Adopt all explicitly approves adoption. Esc returns."
            }
            Self::Confirmation => {
                "Confirm the reviewed operation using literal yes. Managed-link deletion first requires each exact mapping ID. Enter submits for fresh core revalidation. Esc returns to review. Shrinking below the layout floor clears typed approval and removal acknowledgements."
            }
            Self::Task => {
                "The active task keeps its original scope while observations remain on screen. Progress describes metadata and stages. q requests exit after the joined worker finishes. Approved writes run to completion; inspect their result and Recovery if a write fails."
            }
            Self::Error => {
                "The error shows its cause and retained scope. Retry repeats a read or requests a fresh preview. Failed writes may have partial effects: inspect Recovery before another write. Back returns to retained input."
            }
            Self::Result => {
                "Completion records a transaction ID or an up-to-date outcome. Refresh rereads observations; Recovery inspects history; Back returns. Future recovery obtains a fresh preview against current live state."
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
    if app.navigation.menu.is_some() {
        return Topic::Menu;
    }
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
    if app.navigation.menu.is_some() {
        return false;
    }
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
        Line::raw("Esc: return | PgUp/PgDn: scroll | Home: top"),
        Line::raw(""),
    ];
    lines.extend(
        help.topic
            .instructions()
            .split('\n')
            .map(|s| Line::raw(s.to_owned())),
    );
    lines.push(Line::raw(""));
    if matches!(help.topic, Topic::Menu)
        && let Some(menu) = app.navigation.menu
        && menu.expanded
        && let Some(entry) = super::navigation::entries(menu.group).get(menu.selected)
    {
        lines.push(Line::raw(format!("Selected: {}", entry.label)));
        if let super::navigation::Item::Action(action) = entry.item
            && let Some(reason) = app.action_reason(action)
        {
            lines.push(Line::raw(super::views::safe(reason)));
        }
    }
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
        for e in super::context::entries(app).into_iter() {
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
    lines.push(Line::raw("F1 opens contextual Help. Fields accept printable keys literally; Ctrl-U clears a field. F10 opens navigation menus."));
    lines.push(Line::raw("Writes require explicit reviewed approval and fresh revalidation. Recovery is deliberate. Private backups are stored unencrypted."));
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
    ui::prose(frame, back, " [Esc Back] ", ui::accent());
    app.hits.click(back, super::input::Target::Back);
}
