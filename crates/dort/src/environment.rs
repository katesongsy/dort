//! Environment discovery at the application boundary only.
use dort_core::Error;
use std::path::PathBuf;

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
