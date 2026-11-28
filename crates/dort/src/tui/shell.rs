//! Session shell and saved path drafts.
use super::{app::Tab, components::reader::ScrollKeys};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Choose,
    Init,
    Clone,
    Configure,
    Generated,
    LocalInputs,
    Repair,
    Adopt,
    Check,
    KeepId,
    RemoveId,
}

pub struct ProfilePicker {
    pub problem: Option<String>,
    pub report: Box<crate::services::CheckReport>,
    pub requested: Vec<String>,
    pub standalone: bool,
    pub selected: usize,
}

pub struct AdoptionDraft {
    pub target: String,
    pub id: String,
    pub source: String,
    pub profiles: String,
    pub focused: usize,
}
impl AdoptionDraft {
    pub const FIELDS: usize = 4;
    pub const PROFILES_FIELD: usize = 3;

    pub fn mapping(&self) -> dort_core::model::ManagedFile {
        let dort_core::planner::Request::Adopt { mut mapping } = crate::services::adopt_request(
            std::path::Path::new(&self.target),
            &self.id,
            std::path::Path::new(&self.source),
        ) else {
            unreachable!()
        };
        let profiles = super::configuration::list(&self.profiles);
        if !profiles.is_empty() {
            mapping.profiles = profiles;
        }
        mapping
    }

    pub fn field(&mut self, index: usize) -> &mut String {
        match index {
            0 => &mut self.target,
            1 => &mut self.id,
            2 => &mut self.source,
            _ => &mut self.profiles,
        }
    }

    pub fn new() -> Self {
        Self {
            target: String::new(),
            id: String::new(),
            source: String::new(),
            profiles: String::new(),
            focused: 0,
        }
    }
    pub fn suggest(&mut self, path: &std::path::Path, home: &std::path::Path) {
        // Discovery supplies validated paths beneath its explicit HOME. Never infer
        // HOME from a basename/common ancestor or consult the process environment.
        let relative = path
            .strip_prefix(home)
            .expect("discovery path beneath HOME");
        self.target = path.to_str().expect("validated candidate path").to_owned();
        let name = relative
            .to_str()
            .expect("validated relative candidate path");
        let id: String = name
            .trim_start_matches('.')
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .take(128)
            .collect();
        self.id = if id.is_empty() { "file".into() } else { id };
        self.source = std::path::Path::new("files")
            .join(relative)
            .to_str()
            .expect("validated source suggestion")
            .to_owned();
        self.focused = 0;
    }
}
pub enum Overlay {
    Ignored {
        paths: Vec<std::path::PathBuf>,
        selected: usize,
    },
    Repositories {
        selected: usize,
    },
    Form(Form),
    Profiles(Box<ProfilePicker>),
    About,
    DraftAbout(Form),
    Generated(Box<super::generated::Report>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Destination(Tab),
    GitRoot,
    GitDefault,
    GitStatus,
    GitUnstaged,
    GitStaged,
    GitRefresh,
    ToolDeclaration,
    ToolsConfigure,
    ToolsCheck,
    ToolsStandaloneCheck,
    ToolsInstalled,
    ToolsInstallMissing,
    ToolAttempts,
    Choose,
    Init,
    Clone,
    Discover,
    IgnoreSelected,
    ManageIgnored,
    Adopt,
    AdoptSelected,
    BatchAdopt,
    PreviewGenerated,
    LocalInputs,
    NewGenerated,
    EditGenerated,
    ApplyMarked,
    KeepMarked,
    RemoveMarked,
    ToggleMark,
    ClearMarks,
    ExpandAll,
    CollapseAll,
    Profiles,
    ProfilesPage,
    SaveProfileChoice,
    Configure(super::configuration::Kind),
    EnvironmentPlan,
    EnvironmentCheck,
    EnvironmentResume,
    EnvironmentAbandon,
    EnvironmentRecord,
    RepairClone,
    AttachRepaired,
    CheckCurrent,
    CheckStandalone,
    StandaloneProfiles,
    Refresh,
    Apply,
    Keep,
    Remove,
    KeepId,
    RemoveId,
    ClearFilter,
    Inspect,
    Rollback,
    Resume,
    Abandon,
    PreviewOnly,
    Diagnostics,
    About,
    Help,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Destination(Tab::Overview) => "Overview",
            Self::Destination(Tab::Files) => "Files: managed mappings",
            Self::Destination(Tab::Recovery) => "Recovery: all repositories",
            Self::Destination(Tab::Checks) => "Checks: manifest validation",
            Self::Destination(Tab::Git) => "Git: read-only inspection",
            Self::Destination(Tab::Tools) => "Tools: declared, installed and history",
            Self::ToolDeclaration => "Tools: add a tool declaration (no install)",
            Self::ToolsConfigure => "Tools: settings for the selected tool",
            Self::ToolsStandaloneCheck => "Tools: check tools in a standalone manifest file",
            Self::ToolsCheck => "Tools: check declared tools (review detected package managers)",
            Self::ToolsInstalled => "Tools: installed packages and Add declaration",
            Self::ToolsInstallMissing => "Tools: install missing tools (review the exact set)",
            Self::ToolAttempts => "Tools: install records and close an unresolved record",
            Self::GitRoot => "Git: choose a root for this session",
            Self::GitDefault => "Git: use current file repository root",
            Self::GitStatus => "Git: Status",
            Self::GitUnstaged => "Git: Unstaged summary",
            Self::GitStaged => "Git: Staged summary",
            Self::GitRefresh => "Git: Refresh",
            Self::Choose => "Choose repository (session only)",
            Self::Clone => "Clone repository",
            Self::Init => "Create/attach local repository",
            Self::Discover => "Discover files (no repository needed)",
            Self::IgnoreSelected => "Ignore selected file or folder (saved locally)",
            Self::ManageIgnored => "Manage ignored paths (local)",
            Self::Adopt => "Adopt a file (editable target / ID / source)",
            Self::AdoptSelected => "Adopt selected eligible file (editable draft)",
            Self::BatchAdopt => "Adopt marked files (combined editable batch)",
            Self::PreviewGenerated => "Preview generated file",
            Self::LocalInputs => "Local inputs",
            Self::NewGenerated => "New generated file",
            Self::EditGenerated => "Edit generated file",
            Self::ApplyMarked => "Apply marked mappings (selected batch)",
            Self::KeepMarked => "Keep marked mappings (combined batch)",
            Self::RemoveMarked => "Delete marked links (each exact ID then yes)",
            Self::ToggleMark => "Mark or unmark a file or folder",
            Self::ClearMarks => "Clear marks (current Files inventory)",
            Self::ExpandAll => "Expand all tree groups",
            Self::CollapseAll => "Collapse all tree groups",
            Self::EnvironmentPlan => "Review environment",
            Self::EnvironmentCheck => "Environment check",
            Self::EnvironmentResume => "Review environment resume",
            Self::EnvironmentAbandon => "Abandon run",
            Self::EnvironmentRecord => "Environment run",
            Self::RepairClone => "Repair manifest",
            Self::AttachRepaired => "Review attach",
            Self::ProfilesPage => "Profiles",
            Self::SaveProfileChoice => "Local choice",
            Self::Configure(kind) => kind.label(),
            Self::Profiles => "Profiles: extra profiles for this session",
            Self::CheckCurrent => "Check current repository manifest",
            Self::CheckStandalone => "Check standalone manifest (no repository needed)",
            Self::StandaloneProfiles => "Profiles for standalone manifest",
            Self::Refresh => "Refresh data and history",
            Self::Apply => "Preview apply (requires repository)",
            Self::Keep => "Keep local file (select in Files)",
            Self::Remove => "Delete managed link (select in Files)",
            Self::KeepId => "Keep local file: enter exact mapping ID",
            Self::RemoveId => "Delete managed link: enter exact mapping ID",
            Self::ClearFilter => "Clear current display filter (not execution scope)",
            Self::Inspect => "Inspect selected file, operation or Git entry",
            Self::Rollback => "Preview rollback (select in Recovery)",
            Self::Resume => "Preview setup resume (select in Recovery)",
            Self::Abandon => "Preview setup abandon (select in Recovery)",
            Self::PreviewOnly => "Toggle preview-only mode",
            Self::Diagnostics => "Diagnostics: host, paths and blocked writes",
            Self::About => "About Dort: version, features and limits",
            Self::Help => "Help: keys and context",
        }
    }
}
pub struct Shell {
    pub overlay: Option<Overlay>,
    pub scroll: u16,
    pub page_rows: std::cell::Cell<u16>,
    pub init_draft: String,
    pub clone_fields: [String; 2],
    pub generated: Option<super::generated::Draft>,
    pub local_inputs: Option<super::local_inputs::Panel>,
    pub open_template_editor: bool,
    pub repair: Option<super::configuration::RepairEditor>,
    pub configuration: Option<super::configuration::Draft>,
    pub path_focused: usize,
    pub form_details: bool,
    pub form_feedback: Option<(Form, String)>,
    pub picker_details: bool,
    pub ignored_viewport: super::navigation::ListViewport,
    pub choose_draft: String,
    pub adoption: AdoptionDraft,
    pub return_to_adopt: bool,
    pub manifest_draft: String,
    pub manifest_focused: usize,
    pub check_profiles: Vec<String>,
    pub unmanage_id: String,
    pub unmanage_focused: usize,
}
pub enum Intent {
    Unignore(std::path::PathBuf),
    Action(Action),
    Submit(Form),
    ClearLocalInput,
    LeaveDraft,
    Known(usize),
    Active,
    ApplyProfiles {
        manifest: Option<std::path::PathBuf>,
        requested: Vec<String>,
    },
}
impl Shell {
    pub fn new() -> Self {
        Self {
            overlay: None,
            scroll: 0,
            page_rows: std::cell::Cell::new(1),
            init_draft: String::new(),
            clone_fields: Default::default(),
            generated: None,
            local_inputs: None,
            open_template_editor: false,
            repair: None,
            configuration: None,
            path_focused: 0,
            form_details: false,
            form_feedback: None,
            picker_details: false,
            ignored_viewport: Default::default(),
            choose_draft: String::new(),
            adoption: AdoptionDraft::new(),
            return_to_adopt: false,
            manifest_draft: String::new(),
            manifest_focused: 0,
            check_profiles: vec![],
            unmanage_id: String::new(),
            unmanage_focused: 0,
        }
    }
    /// Page the currently visible inline metadata viewport.
    pub fn page(&mut self, down: bool) {
        let rows = self.page_rows.get().max(1);
        self.scroll = if down {
            self.scroll.saturating_add(rows)
        } else {
            self.scroll.saturating_sub(rows)
        };
    }
    pub fn text_entry(&self) -> bool {
        if self.form_details && matches!(self.overlay, Some(Overlay::Form(_))) {
            return false;
        }
        matches!(
            self.overlay,
            Some(Overlay::Form(Form::Clone | Form::Choose | Form::Init))
                if self.path_focused < 2
        ) || matches!(
            self.overlay,
            Some(Overlay::Form(Form::Repair | Form::Generated))
        ) || (matches!(self.overlay, Some(Overlay::Form(Form::LocalInputs)))
            && self.local_inputs.as_ref().is_some_and(|p| p.editing()))
            || (matches!(self.overlay, Some(Overlay::Form(Form::Configure)))
                && self
                    .configuration
                    .as_ref()
                    .is_some_and(|draft| !draft.is_default_row(draft.focus)))
            || (matches!(self.overlay, Some(Overlay::Form(Form::Adopt)))
                && self.adoption.focused < AdoptionDraft::FIELDS)
            || (matches!(self.overlay, Some(Overlay::Form(Form::Check)))
                && self.manifest_focused == 0)
            || (matches!(
                self.overlay,
                Some(Overlay::Form(Form::KeepId | Form::RemoveId))
            ) && self.unmanage_focused == 0)
    }
    pub fn back(&mut self) {
        self.scroll = 0;
        self.local_inputs = None;
        self.overlay = if self.return_to_adopt {
            self.return_to_adopt = false;
            Some(Overlay::Form(Form::Adopt))
        } else {
            None
        };
    }
    pub fn finish_template_edit(&mut self, result: dort_core::Result<String>) {
        let Some(draft) = self.generated.as_mut() else {
            return;
        };
        draft.focus_template();
        self.form_feedback = match result {
            Ok(text) => {
                draft.template.replace(text);
                None
            }
            Err(error) => Some((Form::Generated, error.to_string())),
        };
    }
    pub fn draft(&self, form: Form) -> &str {
        match form {
            Form::Choose => &self.choose_draft,
            Form::Init => &self.init_draft,
            Form::Clone => &self.clone_fields[1],
            Form::Configure | Form::Repair | Form::Generated | Form::LocalInputs => "",
            Form::Adopt => &self.adoption.target,
            Form::Check => &self.manifest_draft,
            Form::KeepId | Form::RemoveId => &self.unmanage_id,
        }
    }
    pub fn form_action(&mut self, command: super::forms::Command) -> Option<Intent> {
        let Some(Overlay::Form(form)) = self.overlay else {
            return None;
        };
        use super::forms::Command;
        match command {
            Command::Submit => Some(Intent::Submit(form)),
            Command::ClearLocalInput if form == Form::LocalInputs => Some(Intent::ClearLocalInput),
            Command::ClearLocalInput => None,
            Command::InsertLine => {
                if form == Form::Repair {
                    self.repair.as_mut()?.insert();
                }
                None
            }
            Command::OpenTemplateEditor => {
                self.open_template_editor = form == Form::Generated
                    && self
                        .generated
                        .as_ref()
                        .is_some_and(|draft| !draft.entering_inputs());
                None
            }
            Command::ToggleGeneratedInput if form == Form::Generated => {
                self.generated.as_mut()?.toggle_value();
                self.form_feedback = None;
                None
            }
            Command::ToggleGeneratedDeclaration if form == Form::Generated => {
                self.generated.as_mut()?.toggle_declaration();
                self.form_feedback = None;
                None
            }
            Command::ToggleGeneratedInput | Command::ToggleGeneratedDeclaration => None,
            Command::Toggle => {
                self.configuration.as_mut()?.toggle_default();
                None
            }
            Command::Choose => Some(Intent::Action(Action::Choose)),
            Command::Create => Some(Intent::Action(Action::Init)),
            Command::Profiles if form == Form::Check => {
                Some(Intent::Action(Action::StandaloneProfiles))
            }
            Command::Profiles => None,
            Command::Details => {
                self.form_details = !self.form_details;
                self.scroll = 0;
                None
            }
            Command::Discard => {
                match form {
                    Form::Adopt => self.adoption = AdoptionDraft::new(),
                    Form::Check => {
                        self.manifest_draft.clear();
                        self.check_profiles.clear();
                    }
                    _ => (),
                };
                None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent, known_count: usize, focused: bool) -> Option<Intent> {
        if !self.form_details
            && matches!(self.overlay, Some(Overlay::Form(_)))
            && matches!(
                key.code,
                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
            )
        {
            self.form_feedback = None;
        }
        if matches!(
            self.overlay,
            Some(Overlay::Profiles(_) | Overlay::Repositories { .. } | Overlay::Ignored { .. })
        ) {
            if key.code == KeyCode::F(3) {
                self.picker_details = !self.picker_details;
                self.scroll = 0;
                return None;
            }
            if self.picker_details {
                if key.code == KeyCode::Esc {
                    self.picker_details = false;
                } else {
                    ScrollKeys::DETAILS.apply(&mut self.scroll, key.code);
                }
                return None;
            }
        }
        if let Some(Overlay::Form(form)) = self.overlay {
            if key.code == KeyCode::F(3) {
                return if matches!(form, Form::Generated | Form::LocalInputs) {
                    None
                } else {
                    self.form_action(super::forms::Command::Details)
                };
            }
            if self.form_details {
                if key.code == KeyCode::Esc {
                    self.form_details = false;
                } else {
                    ScrollKeys::DETAILS.apply(&mut self.scroll, key.code);
                }
                return None;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let command = match key.code {
                    KeyCode::Char('r') => Some(super::forms::Command::Choose),
                    KeyCode::Char('n') => Some(super::forms::Command::Create),
                    KeyCode::Char('p') => Some(super::forms::Command::Profiles),
                    KeyCode::Char('d') => Some(super::forms::Command::Discard),
                    _ => None,
                };
                if let Some(command) = command {
                    return self.form_action(command);
                }
            }
        }
        if !matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            self.scroll = 0;
        }
        match self.overlay.as_mut()? {
            Overlay::Ignored { paths, selected } => match key.code {
                KeyCode::Esc => self.overlay = None,
                KeyCode::Down | KeyCode::Char('j') => {
                    *selected = (*selected + 1).min(paths.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                KeyCode::PageDown => {
                    *selected = (*selected + self.ignored_viewport.page())
                        .min(paths.len().saturating_sub(1))
                }
                KeyCode::PageUp => {
                    *selected = selected.saturating_sub(self.ignored_viewport.page())
                }
                KeyCode::Home => *selected = 0,
                KeyCode::End => *selected = paths.len().saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(path) = paths.get(*selected) {
                        return Some(Intent::Unignore(path.clone()));
                    }
                }
                _ => (),
            },
            Overlay::Generated(_) | Overlay::DraftAbout(_) => unreachable!(),
            Overlay::About => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?')) {
                    self.overlay = None;
                }
            }
            Overlay::Profiles(picker) => {
                let count = picker.report.choices.known.len();
                match key.code {
                    KeyCode::Esc => {
                        self.overlay = if picker.standalone {
                            Some(Overlay::Form(Form::Check))
                        } else {
                            None
                        }
                    }
                    KeyCode::Down => {
                        picker.selected = (picker.selected + 1).min(count.saturating_sub(1))
                    }
                    KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
                    KeyCode::PageDown => {
                        picker.selected = (picker.selected + usize::from(self.page_rows.get()))
                            .min(count.saturating_sub(1))
                    }
                    KeyCode::PageUp => {
                        picker.selected = picker
                            .selected
                            .saturating_sub(usize::from(self.page_rows.get()))
                    }
                    KeyCode::Enter => {
                        return Some(Intent::ApplyProfiles {
                            manifest: picker.standalone.then(|| picker.report.manifest.clone()),
                            requested: picker.requested.clone(),
                        });
                    }
                    KeyCode::Char(' ') => {
                        if let Some(name) = picker.report.choices.known.iter().nth(picker.selected)
                            && !picker.report.choices.mandatory.contains(name)
                        {
                            picker.problem = None;
                            if picker.requested.contains(name) {
                                picker.requested.retain(|p| p != name);
                            } else {
                                picker.requested.push(name.clone());
                            }
                        }
                    }
                    _ => (),
                }
            }
            Overlay::Form(Form::KeepId | Form::RemoveId) => {
                let Some(Overlay::Form(form)) = self.overlay else {
                    unreachable!()
                };
                match key.code {
                    KeyCode::Esc => self.overlay = None,
                    KeyCode::Tab | KeyCode::BackTab => {
                        self.unmanage_focused = 0;
                        self.scroll = 0;
                    }
                    KeyCode::Enter => return Some(Intent::Submit(form)),
                    _ if self.unmanage_focused == 0 => edit_text(&mut self.unmanage_id, key),
                    _ => (),
                }
            }
            Overlay::Form(Form::Check) => match key.code {
                KeyCode::Esc => self.overlay = None,
                KeyCode::Tab | KeyCode::BackTab => {
                    self.manifest_focused = 0;
                    self.scroll = 0;
                }
                KeyCode::Enter => return Some(Intent::Submit(Form::Check)),
                _ => edit_text(&mut self.manifest_draft, key),
            },
            Overlay::Form(Form::Adopt) => {
                let draft = &mut self.adoption;
                match key.code {
                    KeyCode::Esc => self.overlay = None,
                    KeyCode::Tab | KeyCode::BackTab => {
                        let backwards = key.code == KeyCode::BackTab
                            || key.modifiers.contains(KeyModifiers::SHIFT);
                        draft.focused = (draft.focused
                            + if backwards {
                                AdoptionDraft::FIELDS - 1
                            } else {
                                1
                            })
                            % AdoptionDraft::FIELDS;
                        self.scroll = 0;
                    }
                    KeyCode::Enter => return Some(Intent::Submit(Form::Adopt)),
                    _ if draft.focused != AdoptionDraft::PROFILES_FIELD => {
                        edit_text(draft.field(draft.focused), key);
                    }
                    _ => (),
                }
            }
            Overlay::Form(Form::Repair) => match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Enter => return Some(Intent::Submit(Form::Repair)),
                _ => self.repair.as_mut()?.key(key),
            },
            Overlay::Form(Form::Generated) => {
                let draft = self.generated.as_mut()?;
                match key.code {
                    KeyCode::Esc if !draft.back() => return Some(Intent::LeaveDraft),
                    KeyCode::Esc => self.form_feedback = None,
                    KeyCode::Enter if !focused && key.modifiers.is_empty() => {
                        return Some(Intent::Submit(Form::Generated));
                    }
                    KeyCode::F(6) if focused && draft.entering_inputs() => {
                        return self.form_action(super::forms::Command::ToggleGeneratedInput);
                    }
                    KeyCode::F(5) if !draft.entering_inputs() => {
                        return self.form_action(super::forms::Command::OpenTemplateEditor);
                    }
                    _ if focused => {
                        if let Err(error) = draft.key(key) {
                            self.form_feedback = Some((Form::Generated, error.to_string()));
                        }
                    }
                    _ => (),
                }
            }
            Overlay::Form(Form::LocalInputs) => {
                let panel = self.local_inputs.as_mut()?;
                match key.code {
                    KeyCode::Esc => return Some(Intent::LeaveDraft),
                    KeyCode::Enter
                        if (!panel.editing() || !focused) && key.modifiers.is_empty() =>
                    {
                        return Some(Intent::Submit(Form::LocalInputs));
                    }
                    KeyCode::F(6) if !panel.editing() => return Some(Intent::ClearLocalInput),
                    _ => {
                        self.form_feedback = panel
                            .key(key)
                            .err()
                            .map(|e| (Form::LocalInputs, e.to_string()));
                    }
                }
            }
            Overlay::Form(Form::Configure) => match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Enter => return Some(Intent::Submit(Form::Configure)),
                _ => self.configuration.as_mut()?.key(key),
            },
            Overlay::Form(Form::Clone) => match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Tab | KeyCode::BackTab => self.path_focused ^= 1,
                KeyCode::Enter => return Some(Intent::Submit(Form::Clone)),
                _ => edit_text(&mut self.clone_fields[self.path_focused], key),
            },
            Overlay::Form(form) => {
                let form = *form;
                let draft = match form {
                    Form::Choose => &mut self.choose_draft,
                    Form::Init => &mut self.init_draft,
                    Form::Configure
                    | Form::Generated
                    | Form::LocalInputs
                    | Form::Repair
                    | Form::Clone
                    | Form::Adopt
                    | Form::Check
                    | Form::KeepId
                    | Form::RemoveId => {
                        unreachable!()
                    }
                };
                match key.code {
                    KeyCode::Esc => self.back(),
                    KeyCode::Tab | KeyCode::BackTab => self.path_focused = 0,
                    KeyCode::Enter => return Some(Intent::Submit(form)),
                    _ if self.path_focused == 0 => edit_text(draft, key),
                    _ => (),
                }
            }
            Overlay::Repositories { selected } => match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Down => *selected = selected.saturating_add(1).min(known_count + 1),
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::PageDown => {
                    *selected = (*selected + usize::from(self.page_rows.get())).min(known_count + 1)
                }
                KeyCode::PageUp => {
                    *selected = selected.saturating_sub(usize::from(self.page_rows.get()))
                }
                KeyCode::Enter if *selected == 0 => {
                    self.scroll = 0;
                    self.path_focused = 0;
                    self.overlay = Some(Overlay::Form(Form::Choose))
                }
                KeyCode::Enter if *selected == 1 => return Some(Intent::Active),
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Some(Intent::Action(Action::Init));
                }
                KeyCode::Enter => return Some(Intent::Known(*selected - 2)),
                _ => (),
            },
        }
        None
    }
}
pub(super) fn edit_text(input: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            input.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => input.clear(),
        KeyCode::Char(c)
            if !c.is_control()
                && key
                    .modifiers
                    .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    .is_empty()
                && input.len() + c.len_utf8() <= 4096 =>
        {
            input.push(c)
        }
        _ => (),
    }
}
