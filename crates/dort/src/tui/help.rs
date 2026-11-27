use super::{
    actions::Pending,
    app::{App, Tab},
    components as ui,
    configuration::Kind,
    git::Comparison,
    shell::{Form, Overlay},
    tools_install::Mode,
};
use ratatui::{Frame, crossterm::event::KeyCode, layout::Rect};
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) enum Topic {
    Overview,
    OverviewDetails,
    Discover,
    DiscoverDetails,
    Content,
    ManagedContent,
    Managed,
    ManagedDetails,
    GeneratedFiles,
    Generated,
    GeneratedDraft,
    Recovery,
    RecoveryDetails,
    Checks,
    CheckResults,
    Diagnostics,
    Git(Comparison),
    GitRoot,
    GitPatch,
    GitMetadata,
    Declared,
    DeclaredDetails,
    ToolAdd,
    ToolEdit,
    ToolRemove,
    BrewSettings,
    NpmSettings,
    ToolManifest,
    ToolCheck,
    StandaloneToolCheck,
    StandaloneToolResults,
    InstallReview,
    InstallBlocked,
    InstallResults,
    InstalledReview,
    Installed,
    InstalledDetails,
    InstallHistory,
    InstallRecord,
    CloseBrewRecord,
    CloseNpmRecord,
    CloseRecordReview,
    More,
    ProfileChoice,
    DeclarationProvider,
    DraftReferences,
    CloseRecordProvider,
    Adopt,
    AddMapping,
    EditMapping,
    Startup,
    Forget,
    Reconnect,
    OpenRepair,
    Repair,
    CheckForm,
    ChoosePath,
    Clone,
    Init,
    KeepId,
    RemoveId,
    CreateProfile,
    EditProfileDefaults,
    RenameProfile,
    DeleteProfile,
    Profiles,
    Repositories,
    TemporaryProfiles,
    StandaloneProfiles,
    Ignored,
    About,
    Batch,
    BatchItem,
    Preview,
    Environment,
    EnvironmentCheck,
    Task,
    Error,
    Result,
}

impl Topic {
    fn form(form: Form, configuration: Option<Kind>) -> Option<Self> {
        Some(match form {
            Form::Adopt => Self::Adopt,
            Form::Configure => match configuration? {
                Kind::CreateProfile => Self::CreateProfile,
                Kind::EditProfileDefaults => Self::EditProfileDefaults,
                Kind::RenameProfile => Self::RenameProfile,
                Kind::DeleteProfile => Self::DeleteProfile,
                Kind::File => Self::AddMapping,
                Kind::EditFile => Self::EditMapping,
                Kind::Startup => Self::Startup,
                Kind::Forget => Self::Forget,
                Kind::Reconnect => Self::Reconnect,
                Kind::Repair => Self::OpenRepair,
            },
            Form::Repair => Self::Repair,
            Form::Generated => Self::GeneratedDraft,
            Form::Check => Self::CheckForm,
            Form::Choose => Self::ChoosePath,
            Form::Clone => Self::Clone,
            Form::Init => Self::Init,
            Form::KeepId => Self::KeepId,
            Form::RemoveId => Self::RemoveId,
        })
    }

    fn tools(app: &App) -> Self {
        use super::tools::Page;
        use dort_core::tools::{declaration::Change, inspection::Query};
        let tools = &app.tools_view;
        match tools.page() {
            Page::Installed => {
                return if tools.installed.preview.is_some() {
                    Self::InstalledReview
                } else if tools.installed.waiting {
                    Self::Task
                } else if tools.installed.error.is_some() {
                    Self::Error
                } else if tools.installed.details {
                    Self::InstalledDetails
                } else {
                    Self::Installed
                };
            }
            Page::Declaration => {
                return match tools.declaration.change {
                    Some(Change::Remove) => Self::ToolRemove,
                    Some(Change::Edit) => Self::ToolEdit,
                    None => Self::ToolAdd,
                };
            }
            Page::History => {
                return match tools.installation.mode {
                    Mode::Attempts if tools.installation.details => Self::InstallRecord,
                    Mode::Attempts => Self::InstallHistory,
                    Mode::CloseInputs if tools.installation.close_npm => Self::CloseNpmRecord,
                    Mode::CloseInputs => Self::CloseBrewRecord,
                    Mode::LoadingClose => Self::Task,
                    Mode::CloseReview => Self::CloseRecordReview,
                };
            }
            Page::Manifest => return Self::ToolManifest,
            Page::Settings => {
                return match tools
                    .settings
                    .editor
                    .as_ref()
                    .map(|editor| editor.request.query)
                {
                    Some(Query::NpmGlobals) => Self::NpmSettings,
                    Some(Query::BrewFormulae) | None => Self::BrewSettings,
                };
            }
            Page::List | Page::Details => {}
        }
        let declared = &tools.declared;
        if declared.standalone_result.is_some() {
            Self::StandaloneToolResults
        } else if declared.result_open {
            Self::InstallResults
        } else if let Some(preview) = &declared.check_preview {
            if preview.saves_environment_check() {
                Self::EnvironmentCheck
            } else if preview.standalone() {
                Self::StandaloneToolCheck
            } else {
                Self::ToolCheck
            }
        } else if let Some(preview) = &declared.install_preview {
            if preview.is_empty() {
                Self::InstallBlocked
            } else {
                Self::InstallReview
            }
        } else if tools.declared_details {
            Self::DeclaredDetails
        } else {
            Self::Declared
        }
    }

    fn for_app(app: &App) -> Self {
        if let Some(choice) = &app.choice {
            return choice.topic;
        }
        if app.more.is_some() {
            return Self::More;
        }
        if app.error.is_some() {
            return Self::Error;
        }
        if app.busy {
            return Self::Task;
        }
        if let Some(preview) = &app.preview {
            use super::full_check::Destination;
            return match &preview.pending {
                Pending::Environment(_) => Self::Environment,
                Pending::FullCheck(check) => match check.destination {
                    Destination::Environment { .. } => Self::EnvironmentCheck,
                    Destination::Manifest(_, true) => Self::StandaloneToolCheck,
                    Destination::Manifest(_, false) => Self::ToolCheck,
                },
                _ => Self::Preview,
            };
        }
        if app.show_result {
            return Self::Result;
        }
        if let Some(editor) = app.batch_editor.as_ref().filter(|e| e.open) {
            return if editor.item_form {
                Self::BatchItem
            } else {
                Self::Batch
            };
        }
        if let Some(overlay) = &app.shell.overlay {
            let topic = match overlay {
                Overlay::Form(form) => {
                    Self::form(*form, app.shell.configuration.as_ref().map(|d| d.kind))
                }
                Overlay::Ignored { .. } => Some(Self::Ignored),
                Overlay::Generated(_) => Some(Self::Generated),
                Overlay::About => Some(Self::About),
                Overlay::Repositories { .. } => Some(Self::Repositories),
                Overlay::Profiles(picker) if picker.standalone => Some(Self::StandaloneProfiles),
                Overlay::Profiles(_) => Some(Self::TemporaryProfiles),
            };
            if let Some(topic) = topic {
                return topic;
            }
        }
        match app.tab {
            Tab::Overview if app.profiles.open => Self::Profiles,
            Tab::Overview if app.overview_details => Self::OverviewDetails,
            Tab::Overview => Self::Overview,
            Tab::Files if app.files_page == super::inventory::Page::Generated => {
                Self::GeneratedFiles
            }
            Tab::Files if app.discovering() && app.content.focused => Self::Content,
            Tab::Files if app.discovering() && app.inventory.details => Self::DiscoverDetails,
            Tab::Files if app.discovering() => Self::Discover,
            Tab::Files if app.content.focused => Self::ManagedContent,
            Tab::Files if app.inventory.details => Self::ManagedDetails,
            Tab::Files => Self::Managed,
            Tab::Recovery if app.history_region == super::workspace::Region::Details => {
                Self::RecoveryDetails
            }
            Tab::Recovery => Self::Recovery,
            Tab::Checks if app.showing_diagnostics => Self::Diagnostics,
            Tab::Checks if app.check.is_some() => Self::CheckResults,
            Tab::Checks => Self::Checks,
            Tab::Git if app.git_view.editing_root => Self::GitRoot,
            Tab::Git if app.git_view.reading_patch() => Self::GitPatch,
            Tab::Git if app.git_view.browsing_details() => Self::GitMetadata,
            Tab::Git => Self::Git(app.git_view.comparison),
            Tab::Tools => Self::tools(app),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Reader {
    offset: usize,
    end: usize,
    page: usize,
}

pub struct State {
    topic: Topic,
    reader: Cell<Reader>,
    pub content_focus: bool,
    pub content_scroll: u16,
}

impl State {
    pub fn open(app: &App) -> Self {
        Self {
            topic: Topic::for_app(app),
            reader: Cell::default(),
            content_focus: app.content.focused,
            content_scroll: app.content.scroll,
        }
    }

    pub fn scroll(&self, key: KeyCode) {
        let mut reader = self.reader.get();
        reader.offset = match key {
            KeyCode::Home => 0,
            KeyCode::End => reader.end,
            KeyCode::Down | KeyCode::Char('j') => reader.offset.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => reader.offset.saturating_sub(1),
            KeyCode::PageDown => reader.offset.saturating_add(reader.page),
            KeyCode::PageUp => reader.offset.saturating_sub(reader.page),
            _ => return,
        }
        .min(reader.end);
        self.reader.set(reader);
    }
}

pub fn text_input(app: &App) -> bool {
    if app.more.is_some() || app.choice.is_some() {
        return false;
    }
    if app.busy || app.error.is_some() || app.show_result || app.preview.is_some() {
        return false;
    }
    if app.focused_reference().is_some()
        || (app.form_page() == Some("declaration")
            && app.tools_view.declaration.focus == super::tool_declaration::PROVIDER_FIELD)
    {
        return false;
    }
    app.text_entry()
}

pub fn draw(frame: &mut Frame, mut area: Rect, app: &App, help: &State) {
    if let Some(notice) = app.interaction_notice {
        let height = (1 + notice.len().div_ceil(usize::from(area.width).max(1)) as u16)
            .min(3)
            .min(area.height);
        ui::design::notice(
            frame,
            Rect::new(area.x, area.y, area.width, height),
            "Navigation paused",
            notice,
            ui::design::Tone::Attention,
        );
        area.y += height;
        area.height -= height;
    }
    let rows = ui::help::draw(frame, area, help.topic.guide(), help.reader.get().offset);
    help.reader.set(Reader {
        offset: rows.start,
        end: rows.end,
        page: usize::from(rows.area.height).saturating_sub(1).max(1),
    });
}

#[path = "help/guides.rs"]
mod guides;
