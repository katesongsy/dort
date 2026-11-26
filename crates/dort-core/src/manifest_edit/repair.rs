use super::{Approval, Preview, invalid, same_entry};
use crate::{
    Error, Manifest, Result,
    context::HostContext,
    git::Git,
    local::LocalConfig,
    observation::{self, Entry, Kind},
};
use std::{fmt::Write, path::Path};
use toml_edit::{DocumentMut, Item, TableLike};

const MAX_REPAIR_BYTES: usize = 16 * 1024;
const MAX_REPAIR_LINES: usize = 256;

fn text(bytes: &[u8]) -> Result<&str> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("Edit non-UTF-8 manifests externally."))?;
    if bytes.len() > MAX_REPAIR_BYTES || text.split('\n').count() > MAX_REPAIR_LINES {
        return Err(invalid(
            "Edit manifests over 16 KiB or 256 lines externally.",
        ));
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
        || observation::sensitive_content(bytes)
    {
        return Err(invalid(
            "Edit manifests containing control characters or possible credentials externally.",
        ));
    }
    Ok(text)
}

pub(super) fn verify_repair_scope(
    host: &HostContext,
    repository: &Path,
    git: &Entry,
) -> Result<Entry> {
    crate::bootstrap::root_scope(host, repository)?;
    if repository.starts_with(&host.state_dir) {
        return Err(invalid("Choose a repository outside Dort state."));
    }
    let absent = observation::inspect(&host.local_config_path(), false)?;
    let (local, config) = if absent.kind == Kind::Missing {
        (LocalConfig::default(), absent)
    } else {
        LocalConfig::load_bound(host)?
    };
    if local.repositories.values().any(|r| r.root == repository) {
        return Err(invalid(
            "This repository is registered. Edit its invalid manifest externally.",
        ));
    }
    let root = observation::inspect(repository, false)?;
    if root.kind != Kind::Directory
        || root
            .fingerprint
            .as_ref()
            .is_none_or(|f| f.uid != rustix::process::geteuid().as_raw() || f.mode & 0o022 != 0)
    {
        return Err(invalid(
            "Use an exact, owned Git working-tree root, not a link or shared directory.",
        ));
    }
    if git.kind != Kind::Regular
        || git.fingerprint.as_ref().is_none_or(|f| f.mode & 0o111 == 0)
        || !same_entry(git, &observation::inspect(&git.path, false)?)
    {
        return Err(invalid(
            "Git executable changed or is not a regular executable. Restart Dort.",
        ));
    }
    if Git::new(git.path.clone())?.preflight(repository)? != repository {
        return Err(invalid("Choose the exact physical Git working-tree root."));
    }
    Ok(config)
}

fn rename_ubuntu(table: Option<&mut dyn TableLike>) {
    if let Some(table) = table
        && !table.contains_key("linux")
        && let Some(item) = table.remove("ubuntu")
    {
        table.insert("linux", item);
    }
}

fn legacy_draft(original: &str) -> Result<String> {
    let Ok(mut doc) = original.parse::<DocumentMut>() else {
        return Ok(original.to_owned());
    };
    if doc
        .get("schema")
        .is_some_and(|schema| schema.as_integer() != Some(1))
    {
        return Err(invalid(
            "Unsupported manifest schema. Use a compatible Dort version or migrate the manifest externally.",
        ));
    }
    rename_ubuntu(
        doc.get_mut("profiles")
            .and_then(|profiles| profiles.get_mut("platform"))
            .and_then(Item::as_table_like_mut),
    );
    if let Some(files) = doc.get_mut("files").and_then(Item::as_array_of_tables_mut) {
        for file in files.iter_mut() {
            rename_ubuntu(file.get_mut("overrides").and_then(Item::as_table_like_mut));
        }
    }
    if let Some(tools) = doc.get_mut("tools").and_then(Item::as_array_of_tables_mut) {
        for tool in tools.iter_mut() {
            if let Some(install) = tool.get_mut("install")
                && let Some(table) = install.as_table_like()
                && table.len() == 1
                && let Some(rule) = table.get("macos").or_else(|| table.get("ubuntu"))
                && let Some(rule_table) = rule.as_table_like()
                && matches!(
                    rule_table.get("provider").and_then(Item::as_str),
                    Some("brew" | "npm")
                )
            {
                *install = rule.clone();
            }
        }
    }
    Ok(doc.to_string())
}

fn replacement_manifest(text: &str, host: &HostContext, repository: &Path) -> Result<()> {
    let manifest = Manifest::parse(text).map_err(|error| match error {
        Error::Parse(error) => {
            if let Some(span) = error.span() {
                let line = text
                    .as_bytes()
                    .iter()
                    .take(span.start)
                    .filter(|b| **b == b'\n')
                    .count()
                    + 1;
                invalid(format!(
                    "Invalid manifest near line {line}. Edit the draft and review again."
                ))
            } else {
                invalid("Invalid manifest. Edit the draft and review again.")
            }
        }
        error => error,
    })?;
    manifest.validate_local_selection(host, &[], Some(repository))?;
    Ok(())
}

pub struct RepairDraft {
    config: Entry,
    manifest: Entry,
    original: Vec<u8>,
    git: Entry,
    suggested: String,
}

impl RepairDraft {
    pub fn open(host: &HostContext, repository: &Path, git: &Path) -> Result<Self> {
        let git = observation::inspect(git, false)?;
        let config = verify_repair_scope(host, repository, &git)?;
        let path = repository.join("dort.toml");
        let (original, manifest) = observation::read_manifest_bytes(&path)?;
        if manifest
            .fingerprint
            .as_ref()
            .is_none_or(|f| f.mode & 0o7000 != 0)
        {
            return Err(invalid(
                "Edit manifests with special file permissions externally.",
            ));
        }
        let original_text = text(&original)?;
        if replacement_manifest(original_text, host, repository).is_ok() {
            return Err(invalid(
                "The manifest is valid. Review attaching the repository instead.",
            ));
        }
        let suggested = legacy_draft(original_text)?;
        text(suggested.as_bytes())?;
        Ok(Self {
            config,
            manifest,
            original,
            git,
            suggested,
        })
    }

    pub fn repository(&self) -> &Path {
        self.manifest.path.parent().expect("manifest has parent")
    }

    pub fn suggested(&self) -> &str {
        &self.suggested
    }

    pub fn review(&self, host: &HostContext, replacement: String) -> Result<RepairPreview> {
        let repository = self.repository();
        if !same_entry(
            &self.config,
            &verify_repair_scope(host, repository, &self.git)?,
        ) || !same_entry(
            &self.manifest,
            &observation::read_manifest_bytes(&self.manifest.path)?.1,
        ) {
            return Err(invalid(
                "Manifest or registration changed. Open a new repair draft.",
            ));
        }
        let next = text(replacement.as_bytes())?;
        replacement_manifest(next, host, repository)?;
        let diff = diff(text(&self.original)?, next);
        Ok(RepairPreview(Preview {
            repository: repository.to_owned(),
            backup_root: host.state_dir.join("manifest-repairs"),
            repair_git: Some(self.git.clone()),
            config: self.config.clone(),
            manifest: self.manifest.clone(),
            watched_target: None,
            original: self.original.clone(),
            replacement,
            description: diff,
            generated: None,
        }))
    }
}

fn diff(before: &str, after: &str) -> String {
    let old: Vec<_> = before.split('\n').collect();
    let new: Vec<_> = after.split('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = format!(
        "Update dort.toml only. Keep a private backup in Dort state.\nNo registration, Home files or packages change. Review attach separately.\n\nChanges from line {} (before and after):",
        prefix + 1
    );
    for line in &old[prefix..old.len() - suffix] {
        let _ = write!(out, "\nBefore | {line}");
    }
    for line in &new[prefix..new.len() - suffix] {
        let _ = write!(out, "\nAfter  | {line}");
    }
    out
}

pub struct RepairPreview(Preview);

impl RepairPreview {
    pub fn human(&self) -> String {
        format!(
            "Repository: {:?}\n\n{}",
            self.0.repository, self.0.description
        )
    }

    pub fn repository(&self) -> &Path {
        &self.0.repository
    }

    pub fn approve(self) -> Approval {
        self.0.approve()
    }
}
