//! Metadata-only Help layer. It never moves/clones a draft, approval or private payload.
use super::{
    app::{App, Tab},
    components as ui,
    shell::{Form, Overlay},
};
use ratatui::Frame;

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
    Tools,
    Actions,
    Menu,
    More,
    Choice,
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
            Self::Tools => "Explicit provider inspection",
            Self::GitContent => "Git content disclosure",
            Self::Actions => "Actions",
            Self::Menu => "Places",
            Self::More => "More actions",
            Self::Choice => "Choose a value",
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
                "Start with a repository, inspect observations, then review exact changes."
            }
            Self::Discover => {
                "Inspect a selected file first. Adoption is a separate, reviewed action."
            }
            Self::Content => {
                "Only the selected eligible file is read; the preview is bounded and ephemeral."
            }
            Self::Managed => {
                "Inspect linked-file observations. Filters never narrow a later whole-scope apply."
            }
            Self::Recovery => {
                "Inspect all-repository transaction metadata before requesting guarded recovery."
            }
            Self::Checks => {
                "Structural checks validate declarations, not live files or installed-tool readiness."
            }
            Self::Diagnostics => {
                "Content-free host, path, registry and history availability facts."
            }
            Self::Git => {
                "Read-only status and comparison metadata. Patch content has its own disclosure gate."
            }
            Self::GitContent => {
                "Patches may contain secrets. Each new view requires deliberate disclosure."
            }
            Self::Tools => {
                "Provider queries and installations require their own real, single-use reviews."
            }
            Self::Actions => "Find a task by name. Unavailable tasks explain their prerequisites.",
            Self::Menu => {
                "Choose a destination group, then a place or task. Opening/dismissing retains drafts."
            }
            Self::More => {
                "Choose a contextual action. This menu never grants an execution approval."
            }
            Self::Choice => {
                "Select a draft value or explicit snapshot binding; no provider executes."
            }
            Self::Form(Form::Adopt) => {
                "Declare one ordinary file mapping, then review the real adoption effects."
            }
            Self::Form(Form::Check) => {
                "Check an independent manifest with explicit temporary profile additions."
            }
            Self::Form(Form::Choose) => {
                "Select an exact repository root for this session without persisting registration."
            }
            Self::Form(Form::Init) => {
                "Prepare local creation or attachment. Only the later approval persists changes."
            }
            Self::Form(Form::KeepId | Form::RemoveId) => {
                "Use the exact mapping ID; the repository source is retained."
            }
            Self::Picker => {
                "Choose a repository or session profile additions; selection is not write approval."
            }
            Self::Ignored => {
                "Inspect an ignored path, then review removal of its discovery preference."
            }
            Self::About => "Current capabilities, limitations, privacy and supervision facts.",
            Self::Batch => {
                "Review the complete marked adoption set; editing an item never adopts it individually."
            }
            Self::BatchItem => "Edit TARGET, ID and SOURCE. Done returns to the combined draft.",
            Self::Preview => {
                "Review all effects, warnings and guards. Blocked or preview-only layouts cannot approve."
            }
            Self::Confirmation => {
                "The displayed scope binds the real approval internally. No typed phrase is required."
            }
            Self::Task => {
                "Progress describes observed metadata, not an invented percentage or completion claim."
            }
            Self::Error => {
                "Read retry is separate from execution. Failed writes may have partial or ongoing effects."
            }
            Self::Result => {
                "Inspect the observed result. A fresh attempt always needs a new review."
            }
        }
    }
    fn keys(self, app: &App) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::More | Self::Choice => &[
                ("Up/Down", "Move between choices"),
                ("Enter", "Choose"),
                ("F3", "Inspect choice details"),
                ("Esc", "Return without choosing"),
            ],
            Self::Menu => &[
                ("Left/Right", "Choose a group"),
                ("Up/Down", "Choose a destination"),
                ("Enter", "Open group or destination"),
                ("Esc", "Back one level"),
            ],
            Self::Form(_) | Self::BatchItem => &[
                ("Tab / Shift-Tab", "Traverse fields only"),
                ("Enter", "Validate/review, or Done for a batch item"),
                ("F3", "Full draft metadata"),
                ("Ctrl-U", "Clear current field"),
                ("Esc", "Back, retaining draft"),
            ],
            Self::Tools if app.tools_view.declaration_open => &[
                ("Tab", "Traverse visible declaration controls"),
                ("Space", "Choose a platform provider"),
                ("Enter", "Prepare declaration review"),
                ("F3", "Draft metadata"),
                ("F4", "Show/hide optional dependencies"),
            ],
            Self::Tools if app.tools_view.configuring => &[
                ("Tab", "Traverse relevant scope controls"),
                ("Space", "Choose provider from its field"),
                ("Enter", "Prepare exact query review"),
                ("F3", "Full draft scope"),
                ("Esc", "Back, retaining input"),
            ],
            Self::Tools if app.tools_view.setup => &[
                ("Up/Down", "Select manager"),
                ("Space", "Toggle an eligible inventory"),
                ("Enter", "Inspect selected reviewed scopes"),
                ("F3", "Exact prepared scopes"),
                ("F2", "Advanced settings and refresh"),
            ],
            Self::Tools if app.tools_view.recon_open => &[
                ("Tab", "Traverse fields/tool bindings"),
                ("Space", "Choose an explicit compatible snapshot"),
                ("Enter", "Load declarations, calculate, or inspect a result"),
                ("F3", "Full metadata"),
                ("F2", "Eligible installation review or fresh inventory"),
            ],
            Self::Tools => &[
                ("Enter", "Contextual primary shown in the action strip"),
                ("F2", "Inspection, installation and closure review actions"),
                ("F3", "Full metadata when offered"),
                ("Esc", "Back; never replay an old approval"),
            ],
            Self::Actions => &[
                ("Type", "Search task names"),
                ("Up/Down", "Choose a task"),
                ("Enter", "Open the selected available task"),
                ("F3", "Task scope and prerequisites"),
                ("Esc", "Close directly"),
            ],
            Self::Picker => &[
                ("Up/Down", "Traverse choices"),
                ("Space", "Toggle optional profile membership"),
                ("Enter", "Validate/apply selection"),
                ("F3", "Full selected metadata"),
                ("Ctrl-N", "Create/attach from repository picker"),
            ],
            Self::Preview => &[
                ("PgUp/PgDown", "Read all review rows"),
                ("F3", "Full report"),
                ("Enter", "Contextual review/approval step"),
                ("Esc", "Back without execution"),
            ],
            Self::Confirmation => &[
                ("Enter", "Explicitly confirm the displayed scope"),
                ("Esc", "Cancel"),
            ],
            Self::Task => &[
                ("q / Ctrl-C", "Request exit; wait joined for owned work"),
                ("F1", "Metadata-only Help"),
            ],
            Self::Error => &[
                (
                    "Enter",
                    "Retry a read, or open Recovery after a failed write",
                ),
                ("F2", "Other guarded recovery/refresh actions"),
                ("Esc", "Back; retained failure warning remains"),
            ],
            Self::Result => &[
                ("Enter", "Done, or fresh-review navigation for Tools"),
                ("PgUp/PgDown", "Read result metadata"),
                ("F2", "Recovery or fresh observations"),
            ],
            Self::Content => &[
                ("Up/Down", "Scroll selected content"),
                ("PgUp/PgDown", "Page selected content"),
                ("Alt-PgUp/PgDown", "Read selected metadata"),
                ("Esc", "Return to list"),
            ],
            Self::Discover
            | Self::Managed
            | Self::Recovery
            | Self::Git
            | Self::Ignored
            | Self::Batch => &[
                ("Up/Down", "Traverse displayed rows"),
                ("Enter", "Inspect, or the named primary action"),
                ("Space", "Toggle optional membership where offered"),
                ("F2", "Contextual actions with scope/reasons"),
                ("F3", "Metadata when offered"),
            ],
            _ => &[
                ("Enter", "Contextual primary shown in the action strip"),
                ("F2", "More actions"),
                ("F3", "Full metadata when offered"),
                ("PgUp/PgDown", "Read the visible document"),
            ],
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
    if app.choice.is_some() {
        return Topic::Choice;
    }
    if app.more.is_some() {
        return Topic::More;
    }
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
        Tab::Tools => Topic::Tools,
    }
}
/// Text focus is surface-specific; background retained forms do not steal task/error Help.
pub fn text_input(app: &App) -> bool {
    if app.navigation.menu.is_some() || app.more.is_some() || app.choice.is_some() {
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
    let mut area = ui::design::choice_dialog(frame, "Help", &app.hits);
    if let Some(notice) = app.interaction_notice {
        let height = (1 + notice.len().div_ceil(usize::from(area.width).max(1)) as u16)
            .min(3)
            .min(area.height);
        ui::design::notice(
            frame,
            ratatui::layout::Rect::new(area.x, area.y, area.width, height),
            "Navigation paused",
            notice,
            ui::design::Tone::Attention,
        );
        area.y += height;
        area.height = area.height.saturating_sub(height);
    }
    let mut sections = vec![("Workbench", "F10: Places; Ctrl-K: searchable task launcher; F2: contextual More; F1: Help. Esc returns one meaningful level. Fields accept printable keys literally; footer buttons never join field traversal.".to_owned())];
    if matches!(help.topic, Topic::Tools) {
        sections.insert(0, ("Tools boundaries", "Discovery is metadata-only. Prepare selected real scopes before the single inspection confirmation. Snapshots are session claims, not ownership/readiness or execution authority. Bind each declared tool explicitly; changed npm declarations need fresh inspection. Declaration is manifest-only. Installs and historical closure have separate reviews; fresh-review entry never queries, binds or replays automatically. Closure is neither success nor repair.".to_owned()));
    }
    if matches!(help.topic, Topic::Menu)
        && let Some(menu) = app.navigation.menu
        && menu.expanded
        && let Some(entry) = super::navigation::entries(menu.group).get(menu.selected)
    {
        let mut detail = entry.label.to_owned();
        if let super::navigation::Item::Action(action) = entry.item
            && let Some(reason) = app.action_reason(action)
        {
            detail.push_str(": ");
            detail.push_str(reason);
        }
        sections.insert(0, ("Selected destination", detail));
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
        let related = super::context::entries(app)
            .into_iter()
            .map(|e| {
                format!(
                    "{}: {} [{}]{}",
                    e.scope.short(),
                    e.action.label(),
                    e.effect.label(),
                    app.action_reason(e.action)
                        .map(|r| format!(" — {r}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(("Related tasks", related));
    }
    sections.push(("Reading Help", "Up/Down or PgUp/PgDown scroll; Home returns to the top. Esc or F1 closes Help. Only this foreground owns input; underlying task keys do not act.".to_owned()));
    sections.push(("Approval and privacy", "Writes require a fresh real review and independent revalidation. Dialogs/busy work explain paused navigation; outside clicks never cancel or approve. q/Ctrl-C waits for owned work. Private backups are unencrypted. Selected-file and Git content stays outside Help, metadata reports and diagnostics; heuristics are not proof of secrecy.".to_owned()));
    ui::text_view(
        frame,
        area,
        ui::help::document(
            help.topic.title(),
            help.topic.instructions(),
            help.topic.keys(app),
            &sections,
        ),
        help.scroll,
    );
    app.hits.wheel(area, super::input::Wheel::Overlay);
}
