//! Bounded discovery under hidden HOME entries.
use crate::{
    Result,
    context::HostContext,
    observation::{self, Entry, Kind, LinkState},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub entry: Entry,
    pub link_state: Option<LinkState>,
    pub supported_for_adoption: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Discovery {
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
    pub sensitivity_notice: &'static str,
}

/// Ephemeral selected-file disclosure. Deliberately not Debug, Clone or Serialize.
pub struct SelectedContent {
    text: String,
    followed: Option<PathBuf>,
}
impl SelectedContent {
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Link target whose content these bytes came from, when preview followed one.
    pub fn followed(&self) -> Option<&Path> {
        self.followed.as_deref()
    }
}

fn preview(
    path: &Path,
    entry: &observation::Entry,
) -> std::result::Result<SelectedContent, &'static str> {
    let fp = entry.fingerprint.as_ref().ok_or("File unavailable")?;
    if fp.mode & 0o7000 != 0 {
        return Err("Special permission bits; content withheld");
    }
    if fp.size < 0 || fp.size > 65536 {
        return Err("Content withheld: exceeds 64 KiB preview limit");
    }
    let handle = observation::parent_handle(path).map_err(|_| "Unsafe parent; content withheld")?;
    if !observation::same_parent_identity_and_access(&handle.parents, &entry.parents) {
        return Err("Parent changed; content withheld");
    }
    let bytes = observation::read_checked_bounded(&handle, path, fp, 65536)
        .map_err(|_| "File changed or unavailable; content withheld")?;
    if observation::inspect(path, false).ok().is_none_or(|after| {
        after.kind != Kind::Regular
            || after.fingerprint.as_ref() != Some(fp)
            || !observation::same_parent_identity_and_access(&after.parents, &entry.parents)
    }) {
        return Err("File or parent changed; content withheld");
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Binary/non-UTF-8 content withheld")?;
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{1b}'))
    {
        return Err("Binary content withheld");
    }
    Ok(SelectedContent {
        text: text.to_owned(),
        followed: None,
    })
}

/// Preview any selected path, following one link so its target content is shown.
pub fn selected_content_path(
    host: &HostContext,
    repository: Option<&Path>,
    path: &Path,
) -> std::result::Result<SelectedContent, &'static str> {
    let _ = (host, repository);
    let entry =
        observation::inspect(path, false).map_err(|_| "File unavailable; content withheld")?;
    match entry.kind {
        Kind::Regular => preview(path, &entry),
        Kind::Symlink => {
            let target =
                observation::link_path(&entry).ok_or("Unreadable link; content withheld")?;
            let followed =
                observation::inspect(&target, false).map_err(|_| "Link target unavailable")?;
            if followed.kind != Kind::Regular {
                return Err("Link target is not a regular file");
            }
            let mut content = preview(&target, &followed)?;
            content.followed = Some(target);
            Ok(content)
        }
        Kind::Missing => Err("File missing; content withheld"),
        _ => Err("Only regular files or links to them can be previewed"),
    }
}

/// Only the selected eligible discovery entry may be read. Errors never contain bytes.
pub fn selected_content(
    host: &HostContext,
    repository: Option<&Path>,
    expected: &Entry,
) -> std::result::Result<SelectedContent, &'static str> {
    let path = &expected.path;
    let relative = path.strip_prefix(&host.home).map_err(|_| "Outside HOME")?;
    if crate::discovery_ignore::Rules::load(host)
        .map_err(|_| "Discovery preferences unavailable; content withheld")?
        .excludes(relative)
    {
        return Err("Ignored path; content withheld");
    }
    let count = relative.components().count();
    if count == 0
        || count > MAX_DEPTH
        || !relative.as_os_str().as_encoded_bytes().starts_with(b".")
        || excluded(path, &host.home)
        || path.starts_with(&host.config_dir)
        || path.starts_with(&host.state_dir)
        || host.target_path(path, repository).is_err()
    {
        return Err("Protected or excluded path; content withheld");
    }
    let current =
        observation::inspect(path, false).map_err(|_| "File unavailable; content withheld")?;
    if current.path != expected.path
        || current.kind != expected.kind
        || current.fingerprint != expected.fingerprint
        || current.link_target != expected.link_target
        || !observation::same_parent_identity_and_access(&current.parents, &expected.parents)
    {
        return Err("Changed or ineligible file; content withheld");
    }
    match current.kind {
        Kind::Regular => {
            let fp = current.fingerprint.as_ref().ok_or("File unavailable")?;
            if fp.links != 1 || fp.uid != rustix::process::geteuid().as_raw() {
                return Err("Only owned, single-link ordinary files can be previewed");
            }
            preview(path, &current)
        }
        Kind::Symlink => {
            let target =
                observation::link_path(&current).ok_or("Unreadable link; content withheld")?;
            let followed =
                observation::inspect(&target, false).map_err(|_| "Link target unavailable")?;
            if followed.kind != Kind::Regular {
                return Err("Link target is not a regular file");
            }
            let mut content = preview(&target, &followed)?;
            content.followed = Some(target);
            Ok(content)
        }
        _ => Err("Changed or ineligible file; content withheld"),
    }
}

pub fn discover(host: &HostContext, repository: Option<&Path>) -> Result<Discovery> {
    let ignores = crate::discovery_ignore::Rules::load(host)?;
    let mut report = Discovery {
        candidates: vec![],
        warnings: vec![],
        sensitivity_notice: "Metadata-only exclusions are a heuristic, not a guarantee that files contain no secrets. Adoption requires a separate content review.",
    };
    let mut remaining = MAX_ENTRIES;
    walk(
        &host.home,
        0,
        host,
        repository,
        &ignores,
        &mut remaining,
        &mut report,
    )?;
    report
        .candidates
        .sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    report.warnings.sort();
    Ok(report)
}

fn excluded(path: &Path, home: &Path) -> bool {
    observation::sensitive_path(path)
        || path
            .strip_prefix(home)
            .unwrap_or(path)
            .components()
            .any(|part| {
                let name = part.as_os_str().to_string_lossy().to_ascii_lowercase();
                [
                    ".cache",
                    "cache",
                    "caches",
                    ".local",
                    ".git",
                    ".cargo",
                    ".rustup",
                    ".npm",
                    "node_modules",
                    "target",
                    "dist",
                    "build",
                    "logs",
                    "history",
                    ".zsh_history",
                    ".bash_history",
                    ".docker",
                    "google",
                    "chromium",
                    "chrome",
                    "firefox",
                    "bravesoftware",
                    "microsoft-edge",
                    "opera",
                    "vivaldi",
                    "librewolf",
                    "safari",
                    ".mozilla",
                    ".vscode",
                    "browser",
                    "browsers",
                    "keyring",
                    "keyrings",
                    ".ds_store",
                    "generated",
                    "__pycache__",
                    ".venv",
                    "venv",
                    "tmp",
                    "temp",
                ]
                .contains(&name.as_str())
            })
}

fn walk(
    root: &Path,
    depth: usize,
    host: &HostContext,
    repository: Option<&Path>,
    ignores: &crate::discovery_ignore::Rules,
    remaining: &mut usize,
    report: &mut Discovery,
) -> Result<()> {
    for name in observation::names(root, MAX_ENTRIES)? {
        if *remaining == 0 {
            return Err(crate::Error::Invalid(
                "discovery entry limit exceeded".into(),
            ));
        }
        *remaining -= 1;
        if depth == 0 && !name.as_encoded_bytes().starts_with(b".") {
            continue;
        }
        let path: PathBuf = root.join(name);
        if ignores.excludes(path.strip_prefix(&host.home).expect("discovery under HOME"))
            || excluded(&path, &host.home)
            || path.starts_with(&host.config_dir)
            || path.starts_with(&host.state_dir)
            || repository.is_some_and(|repo| path.starts_with(repo))
        {
            continue;
        }
        let protected_container = host.target_path(&path, repository).is_err();
        observation::validate_output_path(&path)?;
        let entry = match observation::inspect(&path, false) {
            Ok(entry) => entry,
            Err(error) => {
                report.warnings.push(error.to_string());
                continue;
            }
        };
        let recurse = entry.kind == Kind::Directory && depth + 1 < MAX_DEPTH;
        let link_state = if entry.kind == Kind::Symlink {
            Some(observation::link_state(&entry)?)
        } else {
            None
        };
        let supported_for_adoption = entry.kind == Kind::Regular
            && entry.fingerprint.as_ref().is_some_and(|f| {
                f.links == 1 && f.uid == rustix::process::geteuid().as_raw() && f.mode & 0o7000 == 0
            });
        if !protected_container {
            report.candidates.push(Candidate {
                entry,
                link_state,
                supported_for_adoption,
            });
        }
        if recurse {
            // A substituted parent is refused by the next descriptor walk, never traversed.
            if let Err(error) = walk(
                &path,
                depth + 1,
                host,
                repository,
                ignores,
                remaining,
                report,
            ) {
                report.warnings.push(error.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Architecture, Platform};

    #[test]
    fn larger_scan_still_enforces_directory_and_shared_recursive_budgets() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = sandbox.path().canonicalize().unwrap().join("home");
        for directory in [".a", ".b"] {
            std::fs::create_dir_all(home.join(directory)).unwrap();
            std::fs::write(home.join(directory).join("settings"), "ordinary settings").unwrap();
        }
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        let mut report = discover(&host, None).unwrap();
        assert_eq!(report.candidates.len(), 4);
        report.candidates.clear();
        // Exercise exhaustion with a small shared budget, not 100,001 fixture files.
        let mut remaining = 3;
        walk(
            &host.home,
            0,
            &host,
            None,
            &crate::discovery_ignore::Rules::default(),
            &mut remaining,
            &mut report,
        )
        .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(report.candidates.len(), 3);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("limit"))
        );
        // The descriptor-relative directory reader independently enforces its cap.
        assert!(observation::names(&host.home, 1).is_err());
        assert_eq!(observation::names(&host.home, 2).unwrap().len(), 2);
    }
}
