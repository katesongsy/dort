//! Pure session-local shell, focus and retained path drafts. No environment or I/O.
use super::app::Tab;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Destinations,
    Content,
    Actions,
}
impl Focus {
    pub fn next(self, backwards: bool) -> Self {
        match (self, backwards) {
            (Self::Destinations, false) | (Self::Actions, true) => Self::Content,
            (Self::Content, false) | (Self::Destinations, true) => Self::Actions,
            _ => Self::Destinations,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Choose,
    Init,
    Adopt,
    Check,
    KeepId,
    RemoveId,
}

pub struct ProfilePicker {
    pub report: Box<crate::services::CheckReport>,
    pub requested: Vec<String>,
    pub standalone: bool,
    pub selected: usize,
}

pub struct AdoptionDraft {
    pub target: String,
    pub id: String,
    pub source: String,
    // Fields, Review, Choose repository, Create/attach, Discard.
    pub focused: usize,
}
impl AdoptionDraft {
    fn new() -> Self {
        Self {
            target: String::new(),
            id: String::new(),
            source: String::new(),
            focused: 0,
        }
    }
    pub fn suggest(&mut self, path: &std::path::Path) {
        self.target = path.to_str().expect("validated candidate path").to_owned();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
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
        self.source = format!("files/{}", self.id);
        self.focused = 0;
    }
}
pub enum Overlay {
    Actions {
        query: String,
        selected: usize,
        searching: bool,
    },
    Repositories {
        selected: usize,
    },
    Form(Form),
    Profiles(Box<ProfilePicker>),
    About,
    Help,
}
#[derive(Clone, Copy)]
pub enum Action {
    Destination(Tab),
    GitRoot,
    GitDefault,
    GitStatus,
    GitUnstaged,
    GitStaged,
    GitRefresh,
    GitPatch,
    Choose,
    Init,
    Discover,
    Adopt,
    AdoptSelected,
    Profiles,
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
    Quit,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Destination(Tab::Overview) => "Overview",
            Self::Destination(Tab::Files) => "Files: managed mappings",
            Self::Destination(Tab::Recovery) => "Recovery: all repositories",
            Self::Destination(Tab::Checks) => "Checks: manifest validation",
            Self::Destination(Tab::Git) => "Git: read-only inspection",
            Self::GitRoot => "Git: choose independent root (session only)",
            Self::GitDefault => "Git: use current file repository root",
            Self::GitStatus => "Git: Status metadata",
            Self::GitUnstaged => "Git: Unstaged summary",
            Self::GitStaged => "Git: Staged summary",
            Self::GitRefresh => "Git: Refresh metadata only",
            Self::GitPatch => "View patch content (Git comparison; may expose secrets)",
            Self::Choose => "Choose repository (session only)",
            Self::Init => "Create/attach local repository",
            Self::Discover => "Discover files (metadata only; no repository needed)",
            Self::Adopt => "Adopt a file (editable target / ID / source)",
            Self::AdoptSelected => "Adopt selected eligible file (inspect first)",
            Self::Profiles => "Profiles: temporary repository additions",
            Self::CheckCurrent => "Check current repository manifest",
            Self::CheckStandalone => "Check standalone manifest (no repository needed)",
            Self::StandaloneProfiles => "Profiles for standalone manifest",
            Self::Refresh => "Refresh observations and history",
            Self::Apply => "Preview apply (requires repository)",
            Self::Keep => "Keep local file (select in Files)",
            Self::Remove => "Delete managed link (select in Files)",
            Self::KeepId => "Keep local file: enter exact mapping ID",
            Self::RemoveId => "Delete managed link: enter exact mapping ID",
            Self::ClearFilter => "Clear Files / Recovery display filter (not execution scope)",
            Self::Inspect => "Inspect selected file / recovery transaction details",
            Self::Rollback => "Preview rollback (select in Recovery)",
            Self::Resume => "Preview bootstrap resume (select in Recovery)",
            Self::Abandon => "Preview bootstrap abandon (select in Recovery)",
            Self::PreviewOnly => "Toggle preview-only mode",
            Self::Diagnostics => "Diagnostics: host, paths and global write blockers",
            Self::About => "About Dort: version, capabilities and limits",
            Self::Help => "Help: workflows and safeguards",
            Self::Quit => "Quit (wait for worker)",
        }
    }
}
pub fn actions(query: &str) -> Vec<Action> {
    [
        Action::Destination(Tab::Overview),
        Action::Destination(Tab::Files),
        Action::Destination(Tab::Recovery),
        Action::Destination(Tab::Checks),
        Action::Destination(Tab::Git),
        Action::Choose,
        Action::Init,
        Action::Discover,
        Action::Adopt,
        Action::AdoptSelected,
        Action::Profiles,
        Action::CheckCurrent,
        Action::CheckStandalone,
        Action::Refresh,
        Action::Apply,
        Action::Keep,
        Action::Remove,
        Action::KeepId,
        Action::RemoveId,
        Action::ClearFilter,
        Action::Inspect,
        Action::Rollback,
        Action::Resume,
        Action::Abandon,
        Action::PreviewOnly,
        Action::GitRoot,
        Action::GitDefault,
        Action::GitStatus,
        Action::GitUnstaged,
        Action::GitStaged,
        Action::GitRefresh,
        Action::GitPatch,
        Action::Diagnostics,
        Action::About,
        Action::Help,
        Action::Quit,
    ]
    .into_iter()
    .filter(|action| {
        action
            .label()
            .to_ascii_lowercase()
            .contains(&query.to_ascii_lowercase())
    })
    .collect()
}

pub struct Shell {
    pub focus: Focus,
    pub overlay: Option<Overlay>,
    pub scroll: u16,
    pub init_draft: String,
    pub path_focused: usize,
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
    Action(Action),
    Submit(Form),
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
            focus: Focus::Content,
            overlay: None,
            scroll: 0,
            init_draft: String::new(),
            path_focused: 0,
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
    pub fn open_actions(&mut self, searching: bool) {
        self.scroll = 0;
        self.overlay = Some(Overlay::Actions {
            query: String::new(),
            selected: 0,
            searching,
        });
    }
    pub fn text_entry(&self) -> bool {
        matches!(
            self.overlay,
            Some(Overlay::Actions {
                searching: true,
                ..
            })
        ) || (matches!(self.overlay, Some(Overlay::Form(Form::Choose | Form::Init)))
            && self.path_focused == 0)
            || (matches!(self.overlay, Some(Overlay::Form(Form::Adopt)))
                && self.adoption.focused < 3)
            || (matches!(self.overlay, Some(Overlay::Form(Form::Check)))
                && self.manifest_focused == 0)
            || (matches!(
                self.overlay,
                Some(Overlay::Form(Form::KeepId | Form::RemoveId))
            ) && self.unmanage_focused == 0)
    }
    pub fn back(&mut self) {
        self.scroll = 0;
        self.overlay = if self.return_to_adopt {
            self.return_to_adopt = false;
            Some(Overlay::Form(Form::Adopt))
        } else {
            None
        };
    }
    pub fn draft(&self, form: Form) -> &str {
        match form {
            Form::Choose => &self.choose_draft,
            Form::Init => &self.init_draft,
            Form::Adopt => &self.adoption.target,
            Form::Check => &self.manifest_draft,
            Form::KeepId | Form::RemoveId => &self.unmanage_id,
        }
    }
    pub fn key(&mut self, key: KeyEvent, known_count: usize) -> Option<Intent> {
        if !matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            self.scroll = 0;
        }
        match self.overlay.as_mut()? {
            Overlay::Help | Overlay::About => {
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
                    KeyCode::Down | KeyCode::Tab => {
                        picker.selected = (picker.selected + 1).min(count)
                    }
                    KeyCode::Up | KeyCode::BackTab => {
                        picker.selected = picker.selected.saturating_sub(1)
                    }
                    KeyCode::Enter if picker.selected == count => {
                        return Some(Intent::ApplyProfiles {
                            manifest: picker.standalone.then(|| picker.report.manifest.clone()),
                            requested: picker.requested.clone(),
                        });
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if let Some(name) = picker.report.choices.known.iter().nth(picker.selected)
                            && !picker.report.choices.mandatory.contains(name)
                        {
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
                        self.unmanage_focused = 1 - self.unmanage_focused;
                        self.scroll = 0;
                    }
                    KeyCode::Enter if self.unmanage_focused == 0 => self.unmanage_focused = 1,
                    KeyCode::Enter => return Some(Intent::Submit(form)),
                    _ if self.unmanage_focused == 0 => edit_text(&mut self.unmanage_id, key),
                    _ => (),
                }
            }
            Overlay::Form(Form::Check) => match key.code {
                KeyCode::Esc => self.overlay = None,
                KeyCode::Tab | KeyCode::BackTab => {
                    self.manifest_focused = (self.manifest_focused
                        + if key.code == KeyCode::BackTab
                            || key.modifiers.contains(KeyModifiers::SHIFT)
                        {
                            3
                        } else {
                            1
                        })
                        % 4;
                    self.scroll = 0;
                }
                KeyCode::Enter if self.manifest_focused == 0 => self.manifest_focused = 1,
                KeyCode::Enter if self.manifest_focused == 1 => {
                    return Some(Intent::Submit(Form::Check));
                }
                KeyCode::Enter if self.manifest_focused == 2 => {
                    return Some(Intent::Action(Action::StandaloneProfiles));
                }
                KeyCode::Enter => {
                    self.manifest_draft.clear();
                    self.check_profiles.clear();
                    self.manifest_focused = 0;
                }
                _ if self.manifest_focused == 0 => edit_text(&mut self.manifest_draft, key),
                _ => (),
            },
            Overlay::Form(Form::Adopt) => {
                let draft = &mut self.adoption;
                match key.code {
                    KeyCode::Esc => self.overlay = None,
                    KeyCode::Tab | KeyCode::BackTab => {
                        let backwards = key.code == KeyCode::BackTab
                            || key.modifiers.contains(KeyModifiers::SHIFT);
                        draft.focused = (draft.focused + if backwards { 6 } else { 1 }) % 7;
                        self.scroll = 0;
                    }
                    KeyCode::Enter if draft.focused < 3 => draft.focused += 1,
                    KeyCode::Enter if draft.focused == 3 => {
                        return Some(Intent::Submit(Form::Adopt));
                    }
                    KeyCode::Enter if draft.focused == 4 => {
                        return Some(Intent::Action(Action::Choose));
                    }
                    KeyCode::Enter if draft.focused == 5 => {
                        return Some(Intent::Action(Action::Init));
                    }
                    KeyCode::Enter => {
                        self.adoption = AdoptionDraft::new();
                        self.scroll = 0;
                    }
                    _ => match draft.focused {
                        0 => edit_text(&mut draft.target, key),
                        1 => edit_text(&mut draft.id, key),
                        2 => edit_text(&mut draft.source, key),
                        _ => (),
                    },
                }
            }
            Overlay::Form(form) => {
                let form = *form;
                let draft = match form {
                    Form::Choose => &mut self.choose_draft,
                    Form::Init => &mut self.init_draft,
                    Form::Adopt | Form::Check | Form::KeepId | Form::RemoveId => unreachable!(),
                };
                match key.code {
                    KeyCode::Esc => self.back(),
                    KeyCode::Tab | KeyCode::BackTab => {
                        self.path_focused = (self.path_focused
                            + if key.code == KeyCode::BackTab
                                || key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                2
                            } else {
                                1
                            })
                            % 3;
                    }
                    KeyCode::Enter if self.path_focused == 0 => self.path_focused = 1,
                    KeyCode::Enter if self.path_focused == 1 => return Some(Intent::Submit(form)),
                    KeyCode::Enter => self.back(),
                    _ if self.path_focused == 0 => edit_text(draft, key),
                    _ => (),
                }
            }
            Overlay::Repositories { selected } => match key.code {
                KeyCode::Esc => self.back(),
                KeyCode::Down => *selected = selected.saturating_add(1).min(known_count + 1),
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Enter if *selected == 0 => {
                    self.scroll = 0;
                    self.path_focused = 0;
                    self.overlay = Some(Overlay::Form(Form::Choose))
                }
                KeyCode::Enter if *selected == 1 => return Some(Intent::Active),
                KeyCode::Enter => return Some(Intent::Known(*selected - 2)),
                _ => (),
            },
            Overlay::Actions {
                query,
                selected,
                searching,
            } => match key.code {
                KeyCode::Esc if *searching => *searching = false,
                KeyCode::Esc => self.overlay = None,
                KeyCode::Down => {
                    *selected = selected
                        .saturating_add(1)
                        .min(actions(query).len().saturating_sub(1))
                }
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(action) = actions(query).get(*selected) {
                        return Some(Intent::Action(*action));
                    }
                }
                KeyCode::Char('/') if !*searching => *searching = true,
                KeyCode::Delete if !*searching => {
                    query.clear();
                    *selected = 0;
                }
                _ if *searching => {
                    edit_text(query, key);
                    *selected = 0;
                }
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
