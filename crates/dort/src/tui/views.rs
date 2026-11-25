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
        Overlay::Ignored { .. } | Overlay::Profiles(_) | Overlay::Repositories { .. } => {
            unreachable!("typed overlay renderer")
        }
        Overlay::Form(Form::Adopt) => format!(
            "New link\n\nTarget: {:?}\nID: {:?}\nSource: {:?}\nRepository: {:?}\nProfiles: {:?}\n\nSource is relative to the repository. Only owned regular files with one link can be adopted. The review shows the exact effects.",
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
            "Mapping ID: {:?}\nAction: {}\n\nUses this exact ID, not the visible filter. Both actions keep the repository source. Keep writes a regular file; delete removes the link.",
            app.shell.unmanage_id,
            if *form == Form::KeepId {
                "Keep local file"
            } else {
                "Delete managed link"
            }
        ),
        Overlay::Form(Form::Check) => format!(
            "Manifest: {:?}\nExtra profiles: {:?}\nRelative paths from: {:?}\n\nAn independent manifest needs no registration or Git. Base and local profiles still apply. This checks declarations, dependencies and support. It does not read files or check installed tools.",
            app.shell.manifest_draft, app.shell.check_profiles, app.launch_directory
        ),
        Overlay::Form(Form::Clone) => format!(
            "Git source: {:?}\nNew folder: {:?}\n\nUse an HTTPS URL or an absolute local path.\nThe clone is registered only if dort.toml passes. Home files and tools stay in place.",
            app.shell.clone_fields[0], app.shell.clone_fields[1]
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
                    "This root applies to this session. It is not saved."
                } else {
                    "An absent path creates a repository. An existing Git repository with dort.toml attaches. The parent must exist. Relative paths resolve from the launch directory. The next review saves the change."
                }
            )
        }
        Overlay::About => format!(
            "Version: {}\nLicense: MIT\nHosts: macOS, Linux and WSL, ARM64 / x86_64\n\nWorks here:\nLocal repository create/attach, file discovery and links, recovery, manifest checks, read-only Git, and Homebrew/npm checks and installs.\n\nTool effects:\nChecks and installs can run package-manager code and change shared state. Homebrew installs its current formula; npm installs an exact package. No sandbox or rollback. Installs stop after 10 minutes.\n\nSafety:\nWrites need a fresh review. A failed write may leave partial effects. Git patches and selected-file previews show content on purpose. Backups are private, not encrypted. Recovery is manual.\n\nNot supported:\nTemplates, secrets, sync, Git writes and other package managers.",
            env!("CARGO_PKG_VERSION")
        ),
    }
}
