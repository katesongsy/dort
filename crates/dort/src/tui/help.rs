//! Read-only Help layer. Drafts and approvals stay in App.
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
    Tools,
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
                "Start with a repository. Review what Dort found, then approve exact changes."
            }
            Self::Discover => "Select a file first. Adoption is a separate, reviewed action.",
            Self::Content => "Only the selected regular file is read. The preview is temporary.",
            Self::Managed => "Inspect managed links. Filters do not narrow a later apply.",
            Self::Recovery => "Review operation records across repositories before recovery.",
            Self::Checks => {
                "Checks validate declarations. They do not read files or check installed tools."
            }
            Self::Diagnostics => "Host, path, repository and history details. No file contents.",
            Self::Git => {
                "Read-only status and diff details. Git previews only the selected file. Enter opens or focuses the patch. F3 opens metadata."
            }
            Self::Tools => {
                "Enter opens the selected tool or package. Check and Install are named actions. Installed keeps only the current result. Add declaration fills known package fields for a separate review."
            }
            Self::More => "Choose a contextual action. This menu does not approve work.",
            Self::Choice => "Choose a value for the form. Selecting a value runs nothing.",
            Self::Form(Form::Adopt) => "Describe one regular-file link, then review its effects.",
            Self::Form(Form::Check) => {
                "Check an independent manifest with extra profiles for this run."
            }
            Self::Form(Form::Choose) => {
                "Select a repository root for this session. The choice is not saved."
            }
            Self::Form(Form::Init) => {
                "Prepare a local repository. The next review saves the change."
            }
            Self::Form(Form::KeepId | Form::RemoveId) => {
                "Use the exact mapping ID. The repository source is kept."
            }
            Self::Picker => "Choose a repository or extra profiles. Selection is not approval.",
            Self::Ignored => "Inspect an ignored path, then remove its discovery preference.",
            Self::About => "What works here, tool effects, safety and limits.",
            Self::Batch => "Review the marked files. Editing one item does not adopt it.",
            Self::BatchItem => "Edit target, ID and source. Done returns to the draft.",
            Self::Preview => {
                "Review effects, warnings and limits. Blocked or preview-only pages cannot approve."
            }
            Self::Confirmation => {
                "The preview determines the approval. No typed phrase is required."
            }
            Self::Task => "Progress uses reported steps. It does not estimate a percentage.",
            Self::Error => {
                "Retry repeats the read or asks for a new preview. A failed write may leave partial effects."
            }
            Self::Result => "Review the result. Another attempt needs a new review.",
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
            Self::Form(_) | Self::BatchItem => &[
                ("Tab / Shift-Tab", "Traverse fields only"),
                ("Enter", "Validate/review, or Done for a batch item"),
                ("F3", "Full draft details"),
                ("Ctrl-U", "Clear current field"),
                ("Esc", "Back, retaining draft"),
            ],
            Self::Tools if app.tools_view.installed_open => &[
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
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
                ("Enter/Space", "Open Package manager when focused"),
                (
                    "Enter",
                    "Review from another field, or click Review declaration",
                ),
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
                    ("Up/Down/PgUp/PgDn", "Read scope, reasons and risks"),
                    ("Enter", "Use the shown review action"),
                    ("Esc", "Cancel"),
                ]
            }
            Self::Tools
                if app.tools_view.install_open
                    && app.tools_view.installation.mode
                        == super::tools_install::Mode::CloseInputs =>
            {
                &[
                    ("Tab / Shift-Tab", "Traverse package manager settings"),
                    ("F4", "Choose the package manager"),
                    ("Enter", "Review close"),
                    ("Esc", "Stop editing, then Back"),
                ]
            }
            Self::Tools if app.tools_view.install_open => &[
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                ("Enter", "Open the selected installation record"),
                ("x", "Close an unresolved record"),
                ("R", "Refresh installation history"),
                ("Esc", "Back"),
            ],
            Self::Tools => &[
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                ("Enter", "Open the selected tool"),
                ("C", "Check installed"),
                ("i", "Install missing tools"),
                ("e / d", "Edit / remove the selected declaration"),
                ("a", "Add a declaration"),
                ("s", "Tool settings when configuration needs attention"),
                ("2 / 3", "Open Installed / History"),
                ("Esc", "Back"),
            ],
            Self::Picker => &[
                ("Up/Down", "Traverse choices"),
                ("Space", "Toggle optional profile membership"),
                ("Enter", "Validate/apply selection"),
                ("F3", "Full selection details"),
                ("Ctrl-N", "Create/attach from repository picker"),
            ],
            Self::Preview => &[
                ("PgUp/PgDown", "Read all review rows"),
                ("F3", "Full report"),
                ("Enter", "Review or approve"),
                ("Esc", "Back without execution"),
            ],
            Self::Confirmation => &[
                ("Enter", "Confirm the displayed changes"),
                ("Esc", "Cancel"),
            ],
            Self::Task => &[
                ("q / Ctrl-C", "Request exit; wait joined for owned work"),
                ("F1", "Help"),
            ],
            Self::Error => &[
                (
                    "Enter",
                    "Retry a read, or open Recovery after a failed write",
                ),
                ("F2", "More recovery or refresh"),
                ("Esc", "Back; retained failure warning remains"),
            ],
            Self::Result => &[
                ("Enter", "Done, or fresh-review navigation for Tools"),
                ("PgUp/PgDown", "Read the result"),
                ("F2", "Recovery or refresh"),
            ],
            Self::Content => &[
                ("Up/Down", "Scroll selected content"),
                ("PgUp/PgDown", "Page selected content"),
                ("Alt-PgUp/PgDown", "Read selected details"),
                ("Esc", "Return to list"),
            ],
            Self::Discover => &[
                ("Up/Down, Home/End", "Move selection"),
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                ("a / A", "Adopt selected / marked files"),
                ("Space", "Mark/unmark"),
                ("i", "Ignore the selected path"),
                ("g", "Open Git"),
                ("Enter", "Read the selected file"),
            ],
            Self::Managed => &[
                ("Up/Down, Home/End", "Move selection"),
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                (
                    "u / U",
                    "Unmanage selected / marked files; keep local copies",
                ),
                ("Space", "Mark/unmark"),
                ("m", "Manual adopt"),
                ("a / A", "Apply all active / marked files"),
                ("Enter", "Read file details"),
            ],
            Self::Ignored => &[
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                ("Home/End", "Select first / last path"),
                ("F3", "Read the full path"),
                ("Enter", "Review unignore"),
            ],
            Self::Recovery | Self::Git => &[
                ("Up/Down", "Move selection"),
                ("Ctrl-F / Ctrl-B", "Page forward / back in the list"),
                ("Enter", "Read selected details"),
                ("F2", "More actions"),
            ],
            Self::Batch => &[
                ("Up/Down", "Traverse displayed rows"),
                ("Enter", "Inspect, or the named primary action"),
                ("Space", "Toggle optional membership where offered"),
                ("F2", "Contextual actions with scope/reasons"),
                ("F3", "Details when offered"),
            ],
            _ => &[
                ("Enter", "Contextual primary shown in the action strip"),
                ("F2", "More actions"),
                ("F3", "Full details when offered"),
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
        Tab::Git => Topic::Git,
        Tab::Tools => Topic::Tools,
    }
}
/// Text focus is surface-specific; background retained forms do not steal task/error Help.
pub fn text_input(app: &App) -> bool {
    if app.more.is_some() || app.choice.is_some() {
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
pub fn draw(frame: &mut Frame, mut area: ratatui::layout::Rect, app: &App, help: &State) {
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
    area.x += u16::from(area.width > 2);
    area.width = area.width.saturating_sub(2);
    let mut sections = vec![("Switch pages", "o Overview   f Files   t Tools\nr Recovery   c Checks   g Git\nUse the number on a tab to open it. While editing, keys type into the field or filter.".to_owned())];
    if matches!(help.topic, Topic::Tools) {
        sections.insert(0, ("Tools guide", "Tools opens declarations. Check installed asks which package managers to query, then shows each result for approval. Installed means the package manager reports the package, not that the command works. Install missing reviews eligible tools once, installs in order and stops on failure. Earlier installs stay; there is no rollback. Settings and history live under Tools.".to_owned()));
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
    sections.push((
        "Reading Help",
        "Use the arrows or Page Up and Page Down to scroll. Home jumps to the top. Press Esc or F1 to close Help."
            .to_owned(),
    ));
    sections.push(("Approval and privacy", "Writes need a fresh review. Click outside a dialog to close it. q/Ctrl-C waits for active work. Backups are private, not encrypted. Selected-file and Git content stays out of Help and reports.".to_owned()));
    ui::text_view(
        frame,
        area,
        ui::help::document(
            area.width,
            help.topic.title(),
            help.topic.instructions(),
            help.topic.keys(app),
            &sections,
        ),
        help.scroll,
    );
    app.hits.wheel(area, super::input::Wheel::Overlay);
}
