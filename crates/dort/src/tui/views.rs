use super::{
    app::App,
    shell::{Form, Overlay},
};
// Escape all non-ASCII/control metadata before terminal rendering, including bidi controls.
pub fn safe(text: &str) -> String {
    crate::services::presentation::safe(text)
}
pub fn draw(frame: &mut ratatui::Frame, app: &App) {
    super::workbench::draw(frame, app);
    app.appearance.apply(frame.buffer_mut());
}
/// Metadata-only reading views. Controls and instructions belong to the shared strip/Help.
pub(super) fn overlay_text(app: &App, overlay: &Overlay, _height: u16, _width: u16) -> String {
    match overlay {
        Overlay::Ignored { .. }
        | Overlay::Profiles(_)
        | Overlay::Repositories { .. }
        | Overlay::Actions { .. } => unreachable!("typed overlay renderer"),
        Overlay::Form(Form::Adopt) => format!(
            "Desired base-profile link\n\nTARGET: {:?}\nID: {:?}\nSOURCE: {:?}\nRepository: {:?}\nEffective profiles: {:?}\n\nSOURCE is repository-relative. Only owned, single-link ordinary files are supported. Core path, kind, sensitivity and collision refusals remain authoritative. The draft is not approval; the real review defines the exact effects.",
            app.shell.adoption.target,
            app.shell.adoption.id,
            app.shell.adoption.source,
            app.data
                .as_ref()
                .and_then(|d| d.repository.as_ref())
                .or(app.repository.as_ref()),
            app.data.as_ref().map(|d| &d.profiles)
        ),
        Overlay::Form(form @ (Form::KeepId | Form::RemoveId)) => format!(
            "Exact mapping ID: {:?}\nOperation: {}\n\nThe preview uses this exact identity, not the display filter. Both modes retain the repository source. Keep materializes an ordinary file; deletion removes the managed link. Any deletion acknowledgement binds the exact selected ID internally.",
            app.shell.unmanage_id,
            if *form == Form::KeepId {
                "Keep local file"
            } else {
                "Delete managed link"
            }
        ),
        Overlay::Form(Form::Check) => format!(
            "Manifest path: {:?}\nRequested profile additions: {:?}\nRelative path base: {:?}\n\nThe independent manifest path needs no registration or Git. Mandatory/local profiles still contribute. Structural checks validate declarations, dependencies and capabilities; live files and installed-tool readiness are separate observations.",
            app.shell.manifest_draft, app.shell.check_profiles, app.launch_directory
        ),
        Overlay::Form(form) => {
            let draft = app.shell.draft(*form);
            let root = (!draft.is_empty()).then(|| {
                crate::services::resolve_root(std::path::Path::new(draft), &app.launch_directory)
            });
            format!(
                "Full path draft: {draft:?}\nResolved path: {root:?}\nLaunch directory: {:?}\n\n{}",
                app.launch_directory,
                if *form == Form::Choose {
                    "The exact root changes only this session's selection; no registration is persisted."
                } else {
                    "An absent destination creates a repository; an existing valid Git repository with dort.toml attaches it. An existing safe parent is required. Relative literal paths resolve from the launch directory. Preparation is read-only; registration/selection requires the separately reviewed real approval."
                }
            )
        }
        Overlay::About => format!(
            "Version: {}\nLicense: MIT\nSupported hosts: macOS, Linux and WSL, ARM64 / x86_64\n\nLocal capabilities:\nGuarded create/attach, file discovery/adoption, apply/unmanage, recovery, structural checks and read-only Git. Tools has Declared, Installed and History pages for brew/npm packages, desired declarations and bounded missing-package installation.\n\nProvider effects:\nProvider/backend/package code and downloads may change data and caches. npm/Homebrew native links can expose commands. Homebrew build/dependency code can execute and dependencies may upgrade. Unprivileged installation has a requested ten-minute deadline; no rollback.\n\nApproval and recovery:\nExecution needs a reviewed real approval. Failed writes may have partial or ongoing effects. Tools attempts cover installation metadata; Recovery covers file/bootstrap changes. Closure is not installation success, repair or rollback.\n\nPrivacy and storage:\nGit patch viewing has a separate deliberate content-disclosure gate. Backups are private, not encrypted; keep independent hardware/power-failure backups.\n\nUnsupported operations:\nOther package installations, templates, secret execution, synchronization and Git writes.",
            env!("CARGO_PKG_VERSION")
        ),
    }
}
