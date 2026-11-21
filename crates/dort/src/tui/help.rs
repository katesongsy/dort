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
            Self::Tools => "Tools",
            Self::GitContent => "Git content disclosure",
            Self::Actions => "Actions",
            Self::Menu => "Menu",
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
                "Enter opens the selected tool or package. Check and Install are named actions. Installed tools keeps only the current inspection; Add declaration prefills known package fields for a separate manifest review."
            }
            Self::Actions => "Find a task by name. Unavailable tasks explain their prerequisites.",
            Self::Menu => {
                "Up/Down selects a section; Left/Right selects a local destination. Enter opens it; Esc closes Menu. Opening/dismissing retains drafts."
            }
            Self::More => {
                "Choose a contextual action. This menu never grants an execution approval."
            }
            Self::Choice => {
                "Choose a value for the current form. Selecting a value runs no programs."
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
                ("Up/Down", "Choose a section"),
                ("Left/Right", "Choose a local destination"),
                ("Enter", "Open selected destination"),
                ("1/2/3/4", "Open numbered destination in this section"),
                (
                    "o/f/t/r/c/g",
                    "Open Overview/Files/Tools/Recovery/Checks/Git",
                ),
                ("Ctrl-G", "Open Menu, including Session"),
                ("Esc / Ctrl-G", "Close Menu without navigating"),
            ],
            Self::Form(_) | Self::BatchItem => &[
                ("Tab / Shift-Tab", "Traverse fields only"),
                ("Enter", "Validate/review, or Done for a batch item"),
                ("F3", "Full draft metadata"),
                ("Ctrl-U", "Clear current field"),
                ("Esc", "Back, retaining draft"),
            ],
            Self::Tools if app.tools_view.installed_open => &[
                (
                    "Enter",
                    "Open the selected package; Run inspection on its explicit review",
                ),
                ("a", "Add a declaration with inspected fields prefilled"),
                ("R", "Review a new installed-package inspection"),
                ("/", "Filter current installed packages"),
                ("Esc", "Back, retaining only the current inspection"),
            ],
            Self::Tools if app.tools_view.declaration_open => &[
                ("Tab / Shift-Tab", "Edit declaration fields"),
                ("Space", "Choose the platform package manager"),
                ("Enter", "Review the declaration change"),
                ("F4", "Show/hide dependencies"),
                ("Esc", "Stop editing, then Back"),
            ],
            Self::Tools if app.tools_view.settings_open => &[
                ("Tab / Shift-Tab", "Edit the necessary tool settings"),
                (
                    "Enter",
                    "Validate and save session settings; runs no programs",
                ),
                ("Esc", "Stop editing, then Back"),
            ],
            Self::Tools if app.tools_view.landing() && app.tools_view.declared.result_open => &[
                ("Enter/Esc", "Done; return to tool statuses"),
                ("Up/Down/PgUp/PgDn", "Read per-tool install outcomes"),
            ],
            Self::Tools
                if app.tools_view.declared.check_preview.is_some()
                    || app.tools_view.declared.install_preview.is_some() =>
            {
                &[
                    (
                        "Up/Down/PgUp/PgDn",
                        "Read the complete scope, reasons and risks",
                    ),
                    ("Enter", "Activate the explicitly displayed review action"),
                    ("Esc", "Cancel without executing"),
                ]
            }
            Self::Tools if app.tools_view.install_open => &[
                ("Enter", "Open the selected installation record"),
                ("x", "Review closure of an unresolved record"),
                ("R", "Refresh installation history"),
                ("Esc", "Back without replaying an installation"),
            ],
            Self::Tools => &[
                ("Enter", "Open the selected tool; never Check or Install"),
                ("C", "Review Check installed"),
                ("i", "Review Install missing after checking"),
                ("e / d", "Edit / remove the selected declaration"),
                ("a", "Add a declaration"),
                ("s", "Tool settings when configuration needs attention"),
                ("h", "Installation history"),
                ("Esc", "Back"),
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
    let mut sections = vec![("Workbench", "Ctrl-G / Menu: sections and destinations; Ctrl-K: searchable task launcher; F2: additional contextual actions when shown; F1: Help. Esc returns one meaningful level. Outside text entry, o/f/t/r/c/g opens Overview/Files/Tools/Recovery/Checks/Git. Menu and the wide rail use that order, with Session last in Menu. Contextual O/F/T/R/C retains adoption/discovery/profiles/refresh/inspection actions; P chooses Git scope. Home selects the first Files row. Numbered local destinations use 1/2/3/4 within the current section outside text entry, or the selected Menu section. Fields and filters accept digits and other printable keys literally; footer buttons never join field traversal.".to_owned())];
    if matches!(help.topic, Topic::Tools) {
        sections.insert(0, ("Tools boundaries", "Tools starts with declarations. Check installed prepares only needed detected provider scopes and shows automatic per-tool associations for approval. Installed means a matching package-manager claim, not verified command readiness. Unknown is never missing. Install missing reviews up to 32 eligible tools once, executes serially and stops on failure; earlier effects remain, with no rollback or silent upgrades. Deferred prerequisites need a fresh check/review. Observations and their associations stay internal. Tool settings are available from its details; installation history retains outcomes and explicit record closure. Navigation and preparation run no provider commands; approvals are never replayed.".to_owned()));
    }
    if matches!(help.topic, Topic::Menu)
        && let Some(menu) = app.navigation.menu
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
