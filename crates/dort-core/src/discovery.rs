//! Bounded metadata discovery under hidden HOME entries; selected content is a separate private read.
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
pub struct SelectedContent(String);
impl SelectedContent {
    pub fn text(&self) -> &str {
        &self.0
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
        || !same_parents(&current.parents, &expected.parents)
        || current.kind != Kind::Regular
        || current.sensitive
    {
        return Err("Changed or ineligible file; content withheld");
    }
    let fp = current.fingerprint.as_ref().ok_or("File unavailable")?;
    if fp.links != 1 || fp.uid != rustix::process::geteuid().as_raw() || fp.mode & 0o7000 != 0 {
        return Err("Only owned, single-link ordinary files can be previewed");
    }
    if fp.size < 0 || fp.size > 65536 {
        return Err("Content withheld: exceeds 64 KiB preview limit");
    }
    let handle = observation::parent_handle(path).map_err(|_| "Unsafe parent; content withheld")?;
    if !same_parents(&handle.parents, &current.parents) {
        return Err("Parent changed; content withheld");
    }
    let bytes = observation::read_checked_bounded(&handle, path, fp, 65536)
        .map_err(|_| "File changed or unavailable; content withheld")?;
    if observation::inspect(path, false).ok().is_none_or(|after| {
        after.kind != current.kind
            || after.fingerprint != current.fingerprint
            || !same_parents(&after.parents, &current.parents)
    }) {
        return Err("File or parent changed; content withheld");
    }
    if observation::sensitive_content(&bytes) {
        return Err("Sensitive-content heuristic matched; content withheld");
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Binary/non-UTF-8 content withheld")?;
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{1b}'))
    {
        return Err("Binary content withheld");
    }
    Ok(SelectedContent(text.to_owned()))
}

fn same_parents(a: &[observation::Parent], b: &[observation::Parent]) -> bool {
    // Directory timestamps/size/link counts change when unrelated siblings change;
    // retain identity, ownership and permissions, as in existing approval guards.
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            let (x, y) = (&a.fingerprint, &b.fingerprint);
            a.path == b.path
                && x.device == y.device
                && x.inode == y.inode
                && x.mode == y.mode
                && x.uid == y.uid
                && x.gid == y.gid
        })
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
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
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
