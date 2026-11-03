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
    pub fn label(self) -> &'static str {
        match self {
            Self::Destinations => "Destinations",
            Self::Content => "Content",
            Self::Actions => "Actions",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Choose,
    Init,
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
    Help,
}
#[derive(Clone, Copy)]
pub enum Action {
    Destination(Tab),
    Choose,
    Init,
    Refresh,
    Apply,
    Keep,
    Remove,
    Rollback,
    Resume,
    Abandon,
    PreviewOnly,
    Help,
    Quit,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Destination(Tab::Overview) => "Overview",
            Self::Destination(Tab::Files) => "Files: managed mappings",
            Self::Destination(_) => "Recovery: all repositories",
            Self::Choose => "Choose repository (session only)",
            Self::Init => "Create/attach local repository",
            Self::Refresh => "Refresh observations and history",
            Self::Apply => "Preview apply (requires repository)",
            Self::Keep => "Keep local file (select in Files)",
            Self::Remove => "Delete managed link (select in Files)",
            Self::Rollback => "Preview rollback (select in Recovery)",
            Self::Resume => "Preview bootstrap resume (select in Recovery)",
            Self::Abandon => "Preview bootstrap abandon (select in Recovery)",
            Self::PreviewOnly => "Toggle preview-only mode",
            Self::Help => "Help / About",
            Self::Quit => "Quit (wait for worker)",
        }
    }
}
pub fn actions(query: &str) -> Vec<Action> {
    [
        Action::Destination(Tab::Overview),
        Action::Destination(Tab::Files),
        Action::Destination(Tab::Recovery),
        Action::Choose,
        Action::Init,
        Action::Refresh,
        Action::Apply,
        Action::Keep,
        Action::Remove,
        Action::Rollback,
        Action::Resume,
        Action::Abandon,
        Action::PreviewOnly,
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
    pub choose_draft: String,
}
pub enum Intent {
    Action(Action),
    Submit(Form),
    Known(usize),
    Active,
}
impl Shell {
    pub fn new() -> Self {
        Self {
            focus: Focus::Content,
            overlay: None,
            scroll: 0,
            init_draft: String::new(),
            choose_draft: String::new(),
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
            Some(
                Overlay::Form(_)
                    | Overlay::Actions {
                        searching: true,
                        ..
                    }
            )
        )
    }
    pub fn draft(&self, form: Form) -> &str {
        match form {
            Form::Choose => &self.choose_draft,
            Form::Init => &self.init_draft,
        }
    }
    pub fn key(&mut self, key: KeyEvent, known_count: usize) -> Option<Intent> {
        match self.overlay.as_mut()? {
            Overlay::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?')) {
                    self.overlay = None;
                }
            }
            Overlay::Form(form) => {
                let form = *form;
                let draft = match form {
                    Form::Choose => &mut self.choose_draft,
                    Form::Init => &mut self.init_draft,
                };
                match key.code {
                    KeyCode::Esc => self.overlay = None,
                    KeyCode::Enter => return Some(Intent::Submit(form)),
                    _ => edit_text(draft, key),
                }
            }
            Overlay::Repositories { selected } => match key.code {
                KeyCode::Esc => self.overlay = None,
                KeyCode::Down => *selected = selected.saturating_add(1).min(known_count + 1),
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Enter if *selected == 0 => {
                    self.scroll = 0;
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
fn edit_text(input: &mut String, key: KeyEvent) {
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
