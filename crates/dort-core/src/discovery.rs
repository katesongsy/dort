//! Bounded metadata-only discovery: top-level hidden files and .config (three levels).
use crate::{
    Result,
    context::HostContext,
    observation::{self, Entry, Kind, LinkState},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

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

pub fn discover(host: &HostContext, repository: Option<&Path>) -> Result<Discovery> {
    let mut report = Discovery {
        candidates: vec![],
        warnings: vec![],
        sensitivity_notice: "Metadata-only exclusions are a heuristic, not a guarantee that files contain no secrets. Adoption requires a separate content review.",
    };
    walk(&host.home, 0, host, repository, &mut 4096, &mut report)?;
    report
        .candidates
        .sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    report.warnings.sort();
    Ok(report)
}

fn excluded(path: &Path) -> bool {
    observation::sensitive_path(path)
        || path.components().any(|part| {
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
            ]
            .contains(&name.as_str())
        })
}

fn walk(
    root: &Path,
    depth: usize,
    host: &HostContext,
    repository: Option<&Path>,
    remaining: &mut usize,
    report: &mut Discovery,
) -> Result<()> {
    for name in observation::names(root, 4096)? {
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
        if excluded(&path)
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
        if depth == 0
            && entry.kind == Kind::Directory
            && path.file_name().is_none_or(|name| name != ".config")
        {
            continue;
        }
        let recurse = entry.kind == Kind::Directory && depth < 3;
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
            if let Err(error) = walk(&path, depth + 1, host, repository, remaining, report) {
                report.warnings.push(error.to_string());
            }
        }
    }
    Ok(())
}
