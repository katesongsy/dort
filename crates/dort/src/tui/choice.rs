//! Choice selection for explicit options. Selecting a value runs nothing.
use super::{
    app::{App, Tab},
    components as ui,
    help::Topic,
    shell::{AdoptionDraft, Form, Overlay},
    tool_declaration::{self, PROVIDER_FIELD},
    tools_install,
};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReferenceField {
    AdoptionProfiles,
    MappingProfiles,
    MappingTools,
    DeclarationProfiles,
    DeclarationTools,
    DeclarationRuntime,
}
impl ReferenceField {
    fn profiles(self) -> bool {
        matches!(
            self,
            Self::AdoptionProfiles | Self::MappingProfiles | Self::DeclarationProfiles
        )
    }
    fn multiple(self) -> bool {
        self != Self::DeclarationRuntime
    }
    pub(super) fn action(self) -> &'static str {
        if self.profiles() {
            "Choose profiles"
        } else if self.multiple() {
            "Choose tools"
        } else {
            "Choose runtime"
        }
    }
    fn ids(self, draft: &str) -> Vec<String> {
        if self.multiple() {
            super::configuration::list(draft)
        } else {
            (!draft.is_empty())
                .then(|| draft.to_owned())
                .into_iter()
                .collect()
        }
    }
    fn draft(self, app: &App) -> Option<&String> {
        match self {
            Self::AdoptionProfiles => Some(&app.shell.adoption.profiles),
            Self::MappingProfiles | Self::MappingTools => {
                app.shell.configuration.as_ref()?.reference_value(self)
            }
            _ => app.tools_view.declaration.reference_value(self),
        }
    }
    fn apply(self, app: &mut App, value: String) {
        match self {
            Self::AdoptionProfiles
                if matches!(app.shell.overlay, Some(Overlay::Form(Form::Adopt))) =>
            {
                if app.shell.adoption.profiles != value {
                    app.shell.adoption.profiles = value;
                    app.shell.form_feedback = None;
                }
            }
            Self::MappingProfiles | Self::MappingTools
                if matches!(app.shell.overlay, Some(Overlay::Form(Form::Configure))) =>
            {
                if let Some(draft) = &mut app.shell.configuration {
                    draft.set_reference(self, value);
                }
            }
            Self::DeclarationProfiles | Self::DeclarationTools | Self::DeclarationRuntime
                if app.tab == Tab::Tools && app.tools_view.declaration_open =>
            {
                app.tools_view.declaration.set_reference(self, value);
            }
            _ => (),
        }
    }
    fn options(self, app: &App) -> Option<Vec<String>> {
        let data = app.data.as_ref()?;
        if self.profiles() {
            return Some(
                data.profile_declarations
                    .as_ref()?
                    .known()
                    .into_iter()
                    .collect(),
            );
        }
        let mut ids: Vec<_> = data
            .tools
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .map(|tool| tool.id.clone())
            .filter(|id| !self.is_self(app, id))
            .collect();
        ids.sort();
        Some(ids)
    }
    fn is_self(self, app: &App, id: &str) -> bool {
        matches!(self, Self::DeclarationTools | Self::DeclarationRuntime)
            && id == app.tools_view.declaration.fields[0]
    }
}
struct References {
    field: ReferenceField,
    options: Vec<String>,
    missing_start: usize,
    pending: Vec<String>,
    note: Option<&'static str>,
}
impl References {
    fn new(field: ReferenceField, app: &App) -> Option<Self> {
        let pending = field.ids(field.draft(app)?);
        let options = field.options(app);
        let note = match options.as_deref() {
            None if field.profiles() => Some("Profiles unavailable. Blank uses base."),
            None => Some("Tools unavailable. Open a repository with a readable manifest."),
            Some([]) if field == ReferenceField::MappingTools => {
                Some("No tools declared in this repository.")
            }
            Some([]) => Some("No other tools declared in this repository."),
            Some(_) if field.profiles() => Some("Blank uses base. Space toggles a profile."),
            Some(_) if field.multiple() => Some("Space toggles a tool."),
            _ => None,
        };
        let mut options = options.unwrap_or_default();
        let missing_start = options.len();
        for id in &pending {
            if !field.is_self(app, id) && !options.contains(id) {
                options.push(id.clone());
            }
        }
        Some(Self {
            field,
            options,
            missing_start,
            pending,
            note,
        })
    }
    fn toggle(&mut self, index: usize) {
        if let Some(id) = self.options.get(index) {
            if self.pending.contains(id) {
                self.pending.retain(|selected| selected != id);
            } else {
                self.pending.push(id.clone());
            }
        }
    }
    fn value(&self, index: usize) -> String {
        if !self.field.multiple()
            && let Some(id) = self.options.get(index)
        {
            id.clone()
        } else {
            self.pending.join(", ")
        }
    }
    fn rows(&self) -> Vec<(&str, &str)> {
        self.options
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let selected = self.pending.contains(id);
                let missing = index >= self.missing_start;
                let status = match (selected, missing) {
                    (true, true) => "Kept: missing",
                    (true, false) if !self.field.multiple() => "Current",
                    (true, false) => "Selected",
                    (false, true) => "Missing",
                    (false, false) => "",
                };
                (id.as_str(), status)
            })
            .collect()
    }
}
#[derive(Clone)]
enum Value {
    ClosureProvider(usize),
    SaveProfile(String, dort_core::profiles::Choice),
    DeclarationProvider {
        field: usize,
        provider: &'static str,
    },
}
/// Template files the generated draft can reference. Dort reads only these paths.
struct Sources {
    rows: Vec<dort_core::template_files::Row>,
    current: Option<String>,
}

impl Sources {
    fn rows(&self) -> Vec<(String, String)> {
        self.rows
            .iter()
            .map(|row| {
                let mappings = match row.mappings {
                    0 => "Unused".to_string(),
                    1 => "1 mapping".to_string(),
                    count => format!("{count} mappings"),
                };
                (row.path.display().to_string(), mappings)
            })
            .collect()
    }

    fn note(&self) -> Option<&'static str> {
        if self.rows.is_empty() {
            Some("No template files yet. Create one in Files / Templates, then choose again.")
        } else {
            Some("Enter reads the file into this draft. Esc keeps the draft unchanged.")
        }
    }

    fn selected(&self) -> usize {
        self.current
            .as_deref()
            .and_then(|current| {
                self.rows
                    .iter()
                    .position(|row| row.path.to_str() == Some(current))
            })
            .unwrap_or(0)
    }

    fn path(&self, index: usize) -> Option<String> {
        self.rows
            .get(index)
            .map(|row| row.path.display().to_string())
    }
}

struct Item {
    label: String,
    scope: String,
    value: Value,
}
enum Choices {
    Commands(Vec<Item>),
    References(References),
    Sources(Sources),
}
pub(super) struct Choice {
    pub title: &'static str,
    pub topic: Topic,
    selected: usize,
    pub details: bool,
    scroll: u16,
    feedback: Option<&'static str>,
    choices: Choices,
}
impl Choice {
    fn new(title: &'static str, topic: Topic, choices: Choices, selected: usize) -> Self {
        Self {
            title,
            topic,
            selected,
            details: false,
            scroll: 0,
            feedback: None,
            choices,
        }
    }
    fn count(&self) -> usize {
        match &self.choices {
            Choices::Commands(items) => items.len(),
            Choices::References(references) => references.options.len(),
            Choices::Sources(sources) => sources.rows.len(),
        }
    }
    pub(super) fn edits_draft(&self) -> bool {
        matches!(&self.choices, Choices::References(_) | Choices::Sources(_))
    }
    pub(super) fn can_toggle(&self) -> bool {
        matches!(&self.choices, Choices::References(r) if r.field.multiple() && !r.options.is_empty())
    }
    fn toggle(&mut self) {
        if let Choices::References(references) = &mut self.choices
            && references.field.multiple()
        {
            references.toggle(self.selected);
        }
    }
    pub(super) fn click_row(&mut self, index: usize) {
        if !self.details && index < self.count() {
            self.selected = index;
            self.toggle();
        }
    }
    pub fn draw(
        &self,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        hits: &super::input::Hits,
        feedback: Option<&'static str>,
    ) {
        let source_rows = match &self.choices {
            Choices::Sources(sources) => sources.rows(),
            _ => Vec::new(),
        };
        let (rows, column, note): (Vec<(&str, &str)>, &'static str, Option<&'static str>) =
            match &self.choices {
                Choices::Commands(items) => (
                    items
                        .iter()
                        .map(|item| (item.label.as_str(), item.scope.as_str()))
                        .collect(),
                    "SCOPE",
                    None,
                ),
                Choices::References(references) => (references.rows(), "STATUS", references.note),
                Choices::Sources(sources) => (
                    source_rows
                        .iter()
                        .map(|(path, mappings)| (path.as_str(), mappings.as_str()))
                        .collect(),
                    "MAPPINGS",
                    sources.note(),
                ),
            };
        ui::choice::draw(
            frame,
            area,
            hits,
            ui::choice::View {
                rows: &rows,
                column,
                selected: self.selected,
                details: self.details,
                scroll: self.scroll,
                note,
                feedback: self.feedback.or(feedback),
            },
        );
    }
}
impl App {
    /// Picker over the repository template files a generated draft can reference.
    pub(super) fn open_source_choice(&mut self, rows: Vec<dort_core::template_files::Row>) {
        let current = self
            .shell
            .generated
            .as_ref()
            .and_then(|draft| draft.source_path())
            .map(str::to_owned);
        let sources = Sources { rows, current };
        let selected = sources.selected();
        self.choice = Some(Choice::new(
            "Choose a template file",
            super::help::Topic::ChooseTemplateFile,
            Choices::Sources(sources),
            selected,
        ));
        self.hits.clear();
    }

    pub(super) fn open_profile_choice(&mut self, row: dort_core::profiles::Row) {
        use dort_core::profiles::Choice as P;
        let values = [P::Defaults, P::On, P::Off];
        self.choice = Some(Choice::new(
            "Choose saved profile setting",
            super::help::Topic::ProfileChoice,
            Choices::Commands(
                values
                    .into_iter()
                    .map(|value| Item {
                        label: value.label().into(),
                        scope: row.name.clone(),
                        value: Value::SaveProfile(row.name.clone(), value),
                    })
                    .collect(),
            ),
            values.iter().position(|v| *v == row.choice).unwrap_or(0),
        ));
    }

    pub(super) fn focused_reference(&self) -> Option<ReferenceField> {
        if !self.form_has_focus() {
            return None;
        }
        match self.form_page()? {
            "adopt" if self.shell.adoption.focused == AdoptionDraft::PROFILES_FIELD => {
                Some(ReferenceField::AdoptionProfiles)
            }
            "configuration" => self
                .shell
                .configuration
                .as_ref()
                .and_then(|d| d.reference_field(d.focus)),
            "declaration" => {
                let draft = &self.tools_view.declaration;
                if !draft.controls().contains(&draft.focus) {
                    return None;
                }
                tool_declaration::State::reference_field(draft.focus)
            }
            _ => None,
        }
    }
    pub(super) fn open_control_choice(&mut self, key: KeyEvent) -> bool {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
            || self.busy
            || self.help.is_some()
            || self.more.is_some()
            || self.choice.is_some()
        {
            return false;
        }
        let code = key.code;
        if matches!(code, KeyCode::Enter | KeyCode::Char(' '))
            && let Some(field) = self.focused_reference()
        {
            if let Some(references) = References::new(field, self) {
                let selected = references
                    .pending
                    .first()
                    .and_then(|id| references.options.iter().position(|option| option == id))
                    .unwrap_or(0);
                self.choice = Some(Choice::new(
                    field.action(),
                    Topic::DraftReferences,
                    Choices::References(references),
                    selected,
                ));
                self.hits.clear();
            }
            return true;
        }
        if self.tab != Tab::Tools || self.shell.overlay.is_some() {
            return false;
        }
        let tools = &self.tools_view;
        let opening = matches!(
            code,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right
        );
        let choice = if tools.install_open
            && tools.installation.mode == tools_install::Mode::CloseInputs
            && code == KeyCode::F(4)
        {
            Some(Choice::new(
                "Choose package manager",
                super::help::Topic::CloseRecordProvider,
                Choices::Commands(
                    ["npm", "Homebrew"]
                        .iter()
                        .enumerate()
                        .map(|(index, label)| Item {
                            label: (*label).into(),
                            scope: "Saved inputs; nothing is retried.".into(),
                            value: Value::ClosureProvider(index),
                        })
                        .collect(),
                ),
                tools.installation.closure_provider(),
            ))
        } else if self.form_page() == Some("declaration")
            && self.form_has_focus()
            && tools.declaration.focus == PROVIDER_FIELD
            && opening
        {
            let field = PROVIDER_FIELD;
            let options = [("Homebrew", "brew"), ("npm", "npm")];
            let selected = options
                .iter()
                .position(|(_, value)| *value == tools.declaration.fields[field])
                .unwrap_or(0);
            Some(Choice::new(
                "Package manager",
                super::help::Topic::DeclarationProvider,
                Choices::Commands(
                    options
                        .into_iter()
                        .map(|(label, provider)| Item {
                            label: label.into(),
                            scope: "Declaration only; no install".into(),
                            value: Value::DeclarationProvider { field, provider },
                        })
                        .collect(),
                ),
                selected,
            ))
        } else {
            None
        };
        if let Some(choice) = choice {
            self.choice = Some(choice);
            self.hits.clear();
            true
        } else {
            false
        }
    }
    pub(super) fn choice_key(&mut self, code: KeyCode) -> Option<super::actions::Task> {
        let Some(choice) = &mut self.choice else {
            return None;
        };
        if code == KeyCode::F(3) {
            choice.details = !choice.details;
            choice.scroll = 0;
            return None;
        }
        if choice.details {
            match code {
                KeyCode::Esc => choice.details = false,
                KeyCode::Down | KeyCode::PageDown => {
                    choice.scroll = choice.scroll.saturating_add(2)
                }
                KeyCode::Up | KeyCode::PageUp => choice.scroll = choice.scroll.saturating_sub(2),
                KeyCode::Home => choice.scroll = 0,
                _ => (),
            }
            return None;
        }
        let count = choice.count();
        match code {
            KeyCode::Esc => self.choice = None,
            KeyCode::Char(' ') => choice.toggle(),
            KeyCode::Down | KeyCode::Tab => choice.selected = (choice.selected + 1) % count.max(1),
            KeyCode::Up | KeyCode::BackTab => {
                choice.selected = (choice.selected + count.saturating_sub(1)) % count.max(1)
            }
            KeyCode::Home => choice.selected = 0,
            KeyCode::End => choice.selected = count.saturating_sub(1),
            KeyCode::PageDown => {
                choice.selected = (choice.selected + 3).min(count.saturating_sub(1))
            }
            KeyCode::PageUp => choice.selected = choice.selected.saturating_sub(3),
            KeyCode::F(2) | KeyCode::Char('k') => {
                choice.feedback = Some("Choose a value or cancel before opening another menu.")
            }
            KeyCode::Enter => {
                if let Some(choice) = &mut self.choice
                    && matches!(&choice.choices, Choices::Sources(sources) if sources.rows.is_empty())
                {
                    choice.feedback = Some(
                        "No template files to choose. Cancel and create one in Files / Templates.",
                    );
                    return None;
                }
                let choice = self.choice.take()?;
                self.hits.clear();
                let value = match choice.choices {
                    Choices::Commands(items) => items.get(choice.selected)?.value.clone(),
                    Choices::References(references) => {
                        references
                            .field
                            .apply(self, references.value(choice.selected));
                        return None;
                    }
                    Choices::Sources(sources) => {
                        let path = sources.path(choice.selected)?;
                        self.shell.generated.as_mut()?.choose_source(path.clone());
                        self.shell.form_feedback = None;
                        let root = self.template_repository()?;
                        return Some(super::actions::Task::ReadDraftSource(root, path.into()));
                    }
                };
                match value {
                    Value::SaveProfile(name, value) => {
                        let root = self.data.as_ref()?.repository.clone()?;
                        return Some(super::actions::Task::Profile(root, name, value));
                    }
                    Value::ClosureProvider(index) => {
                        self.tools_view.installation.choose_closure_provider(index)
                    }
                    Value::DeclarationProvider { field, provider }
                        if self.tools_view.declaration_open =>
                    {
                        self.tools_view.declaration.choose_provider(field, provider)
                    }
                    _ => (),
                }
            }
            _ => (),
        }
        None
    }
}
