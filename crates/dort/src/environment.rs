//! Environment discovery at the application boundary only.
use dort_core::Error;
use std::path::PathBuf;

pub fn terminal_appearance() -> crate::tui::appearance::Appearance {
    // Non-Unicode locale values remain nonempty and conservatively request ASCII.
    let value = |name| std::env::var_os(name).map(|s| s.into_string().unwrap_or("C".into()));
    let term = value("TERM");
    let all = value("LC_ALL");
    let ctype = value("LC_CTYPE");
    let lang = value("LANG");
    let no_color = value("NO_COLOR");
    crate::tui::appearance::Appearance::classify(
        term.as_deref(),
        all.as_deref(),
        ctype.as_deref(),
        lang.as_deref(),
        no_color.as_deref(),
    )
}

// Ignore empty/relative PATH entries so a repository-local `git` is not selected.
pub fn resolve_git() -> dort_core::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path).filter(|p| p.is_absolute()) {
            let candidate = directory.join("git");
            if let Ok(metadata) = candidate.metadata()
                && metadata.is_file()
                && metadata.permissions().mode() & 0o111 != 0
                && let Ok(absolute) = candidate.canonicalize()
            {
                return Ok(absolute);
            }
        }
    }
    Err(Error::Invalid(
        "Git executable not found in absolute PATH entries; install Git separately".into(),
    ))
}

pub fn launch_directory() -> dort_core::Result<PathBuf> {
    std::env::current_dir().map_err(|source| Error::Io {
        operation: "resolve current directory",
        path: PathBuf::from("."),
        source,
    })
}
