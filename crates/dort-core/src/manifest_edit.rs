use crate::{
    Error, Manifest, Result,
    adoption::{parent_identity, same, transaction_id},
    context::HostContext,
    error::io_error,
    generated, journal,
    local::{LocalConfig, repository_state_path},
    model::{FileMode, ManagedFile},
    observation::{self, Entry},
    platform::Platform,
    state::{PrivateDir, WriterLock},
};
use rustix::{fs, process};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use toml_edit::{Array, DocumentMut, Item, Table, Value, value};
mod repair;
use repair::verify_repair_scope;
pub use repair::{RepairDraft, RepairPreview};

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub default: bool,
    pub macos: bool,
    pub linux: bool,
}

impl Profile {
    pub fn from_declaration(profiles: &crate::profiles::Profiles, name: String) -> Self {
        let platform_default = |platform| {
            profiles
                .platform
                .get(&platform)
                .is_some_and(|names| names.contains(&name))
        };
        Self {
            default: profiles.default.contains(&name),
            macos: platform_default(Platform::Macos),
            linux: platform_default(Platform::Linux),
            name,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Change {
    Profile {
        old: Option<String>,
        new: Option<Profile>,
    },
    RenameProfile {
        old: String,
        new: String,
    },
    File(Box<FileChange>),
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub mapping: ManagedFile,
    pub before: Option<ManagedFile>,
}

/// What a file mapping uses to reach Home: a link source, template text in dort.toml
/// or a template file in the repository.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Content {
    Link,
    Inline,
    File,
}

impl Content {
    fn of(file: &ManagedFile) -> Option<Self> {
        Some(match file.mode {
            FileMode::Link => Self::Link,
            FileMode::Template if file.template.is_some() => Self::Inline,
            FileMode::Template => Self::File,
            FileMode::Secret => return None,
        })
    }

    /// Generated mappings render; link mappings point at an existing source file.
    fn renders(self) -> bool {
        !matches!(self, Self::Link)
    }
}

pub struct Preview {
    repository: PathBuf,
    backup_root: PathBuf,
    repair_git: Option<Entry>,
    config: Entry,
    manifest: Entry,
    watched_target: Option<Entry>,
    /// Template file read for the rendered output. Approval compares it again.
    watched_source: Option<Entry>,
    original: Vec<u8>,
    replacement: String,
    description: String,
    generated: Option<generated::DeclarationPreview>,
}

pub struct Approval(Preview);

fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid(reason.into())
}

fn list(table: &mut Table, key: &str, old: Option<&str>, new: Option<&str>) {
    if let Some(array) = table.get_mut(key).and_then(Item::as_array_mut) {
        if let Some(old) = old {
            array.retain(|item| item.as_str() != Some(old));
        }
        if let Some(new) = new
            && !array.iter().any(|item| item.as_str() == Some(new))
        {
            array.push(new);
        }
    } else if let Some(new) = new {
        let mut array = Array::new();
        array.push(new);
        table.insert(key, value(array));
    }
}

fn edit_profile(doc: &mut DocumentMut, old: Option<&str>, new: Option<&Profile>) -> Result<()> {
    if !doc.contains_key("profiles") {
        doc["profiles"] = Item::Table(Table::new());
    }
    let profiles = &mut doc["profiles"];
    let profiles = profiles
        .as_table_like_mut()
        .ok_or_else(|| invalid("Read the profile list again."))?;
    let name = new.map(|profile| profile.name.as_str());
    let mut defs = Table::new();
    for key in ["available", "default"] {
        if let Some(item) = profiles.remove(key) {
            defs.insert(key, item);
        }
    }
    list(&mut defs, "available", old, name);
    list(
        &mut defs,
        "default",
        old,
        new.filter(|p| p.default).map(|p| p.name.as_str()),
    );
    for (key, item) in defs {
        profiles.insert(&key, item);
    }
    let mut platform = Table::new();
    if let Some(item) = profiles.remove("platform") {
        match item {
            Item::Table(table) => platform = table,
            Item::Value(Value::InlineTable(table)) => platform = table.into_table(),
            _ => return Err(invalid("Read the platform profile list again.")),
        }
    }
    list(
        &mut platform,
        "macos",
        old,
        new.filter(|p| p.macos).map(|p| p.name.as_str()),
    );
    list(
        &mut platform,
        "linux",
        old,
        new.filter(|p| p.linux).map(|p| p.name.as_str()),
    );
    if !platform.is_empty() {
        profiles.insert("platform", Item::Table(platform));
    }
    if let (Some(old), Some(name)) = (old, name)
        && old != name
    {
        for section in ["files", "tools"] {
            if let Some(tables) = doc.get_mut(section).and_then(Item::as_array_of_tables_mut) {
                for table in tables.iter_mut() {
                    if let Some(names) = table.get_mut("profiles").and_then(Item::as_array_mut) {
                        for item in names.iter_mut().filter(|item| item.as_str() == Some(old)) {
                            let decor = item.decor().clone();
                            *item = Value::from(name);
                            *item.decor_mut() = decor;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn edit_file(doc: &mut DocumentMut, manifest: &Manifest, file: &ManagedFile) -> Result<()> {
    let fields: DocumentMut = toml::to_string(file)
        .map_err(|_| invalid("Cannot encode this file mapping."))?
        .parse()
        .map_err(|_| invalid("Cannot save this file mapping."))?;
    if let Some((index, before)) = manifest
        .files
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.id == file.id)
    {
        let existing = doc["files"]
            .as_array_of_tables_mut()
            .and_then(|files| files.get_mut(index))
            .ok_or_else(|| invalid("Use an array of file tables to edit this mapping."))?;
        if let Some((legacy, template)) = existing.remove_entry("recipe") {
            let mut key = toml_edit::Key::new("template");
            *key.leaf_decor_mut() = legacy.leaf_decor().clone();
            existing.insert_formatted(&key, template);
        }
        for (key, changed) in [
            ("source", before.source != file.source),
            ("template", before.template != file.template),
            ("inputs", before.inputs != file.inputs),
            ("host_inputs", before.host_inputs != file.host_inputs),
            ("local_inputs", before.local_inputs != file.local_inputs),
            ("target", before.target != file.target),
            ("profiles", before.profiles != file.profiles),
            ("requires", before.requires != file.requires),
            ("overrides", before.overrides != file.overrides),
        ] {
            if changed && let Some(field) = fields.get(key) {
                let mut field = field
                    .clone()
                    .into_value()
                    .map_err(|_| invalid("Could not prepare the file mapping."))?;
                if key == "template"
                    && let Some(previous) = existing.get_mut(key).and_then(Item::as_value_mut)
                {
                    *field.decor_mut() = previous.decor().clone();
                    *previous = field;
                } else {
                    existing.insert(key, Item::Value(field));
                }
            } else if changed {
                existing.remove(key);
            }
        }
    } else {
        if !doc.contains_key("files") {
            doc["files"] = Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        let mut mapping = Table::new();
        for (key, field) in fields.into_table() {
            let field = field
                .into_value()
                .map_err(|_| invalid("Could not prepare the file mapping."))?;
            mapping.insert(&key, Item::Value(field));
        }
        doc["files"]
            .as_array_of_tables_mut()
            .ok_or_else(|| invalid("Use an array of file tables to add a mapping."))?
            .push(mapping);
    }
    Ok(())
}

fn same_entry(before: &Entry, after: &Entry) -> bool {
    same(before, after, true)
        && before.parents.len() == after.parents.len()
        && before
            .parents
            .iter()
            .zip(&after.parents)
            .all(|(a, b)| a.path == b.path && parent_identity(&a.fingerprint, &b.fingerprint))
}

impl Preview {
    pub fn prepare(host: &HostContext, repository: &Path, change: &Change) -> Result<Self> {
        let (local, config) = LocalConfig::load_bound(host)?;
        let repository_id = local
            .repository_id(repository)
            .ok_or_else(|| invalid("Choose a registered repository."))?;
        let path = repository.join("dort.toml");
        let (manifest, entry) = observation::read_manifest(&path).map_err(|error| match error {
            Error::Parse(_) => invalid("Invalid manifest. Contents withheld."),
            other => other,
        })?;
        let original = crate::adoption::checked_bytes(&entry)?;
        let text =
            std::str::from_utf8(&original).map_err(|_| invalid("Manifest must be UTF-8."))?;
        let mut doc: DocumentMut = text
            .parse()
            .map_err(|_| invalid("Invalid manifest. Contents withheld."))?;
        let mut watched_target = None;
        let mut generated_id = None;
        let description = match change {
            Change::Profile { .. } | Change::RenameProfile { .. } => {
                let renamed;
                let (old, new) = match change {
                    Change::Profile { old, new } => (old.as_deref(), new.as_ref()),
                    Change::RenameProfile { old, new } => {
                        if old == new {
                            return Err(invalid("Enter a different profile name."));
                        }
                        renamed = Profile {
                            name: new.clone(),
                            ..Profile::from_declaration(&manifest.profiles, old.clone())
                        };
                        (Some(old.as_str()), Some(&renamed))
                    }
                    _ => unreachable!(),
                };
                let names = manifest.profiles.known();
                if old.is_none() && new.is_none()
                    || old == Some("base")
                    || new.is_some_and(|p| p.name == "base")
                {
                    return Err(invalid("Base is always on and cannot be changed."));
                }
                if old.is_some_and(|name| !names.contains(name)) {
                    return Err(invalid("Select an existing profile."));
                }
                if new.is_some_and(|p| Some(p.name.as_str()) != old && names.contains(&p.name)) {
                    return Err(invalid("That profile already exists."));
                }
                if let Some(name) = old.filter(|name| new.is_none_or(|next| next.name != *name)) {
                    if local.active_profiles.iter().any(|p| p == name) {
                        return Err(invalid(
                            "Clear the old profile from legacy local settings first.",
                        ));
                    }
                    if new.is_none() {
                        let used_by: BTreeSet<_> = manifest
                            .files
                            .iter()
                            .map(|f| (&f.id, &f.profiles))
                            .chain(manifest.tools.iter().map(|t| (&t.id, &t.profiles)))
                            .filter(|(_, profiles)| profiles.iter().any(|p| p == name))
                            .map(|(id, _)| id.as_str())
                            .collect();
                        if !used_by.is_empty() {
                            return Err(invalid(format!(
                                "Change the profile on these files and tools first: {}.",
                                used_by.into_iter().collect::<Vec<_>>().join(", ")
                            )));
                        }
                    }
                }
                edit_profile(&mut doc, old, new)?;
                (match (old, new) {
                    (None, Some(p)) => format!("Create profile {:?}.", p.name),
                    (Some(old), Some(p)) if old != p.name => format!(
                        "Rename {old:?} to {:?} in the manifest and its files and tools. Set the saved choice for the new name separately.",
                        p.name
                    ),
                    (Some(old), Some(_)) => {
                        format!("Change the repository and OS defaults for {old:?}.")
                    }
                    (Some(old), None) => format!("Delete profile {old:?}."),
                    _ => unreachable!(),
                }) + &new
                    .map(|p| {
                        format!(
                            "\nRepository default: {}\nmacOS default: {}\nLinux default: {}",
                            p.default, p.macos, p.linux
                        )
                    })
                    .unwrap_or_default()
            }
            Change::File(change) => {
                let FileChange {
                    mapping: file,
                    before,
                } = change.as_ref();
                let content = Content::of(file)
                    .ok_or_else(|| invalid("Choose a link mapping or a template."))?;
                let previous = manifest
                    .files
                    .iter()
                    .find(|existing| existing.id == file.id);
                match (before.is_some(), previous) {
                    (false, Some(_)) => {
                        return Err(invalid(
                            "That file ID already exists. Choose a different ID or edit the saved file.",
                        ));
                    }
                    (true, None) => {
                        return Err(invalid(
                            "That mapping no longer exists. Refresh and select it again.",
                        ));
                    }
                    _ => (),
                }
                if before.as_ref() != previous {
                    return Err(invalid("That mapping changed. Refresh and edit it again."));
                }
                if let Some(previous) = previous {
                    if Content::of(previous) != Some(content) {
                        return Err(invalid("Keep the existing file mode and source type."));
                    }
                    if content.renders() {
                        if previous.profiles != file.profiles
                            || previous.requires != file.requires
                            || previous.permissions != file.permissions
                        {
                            return Err(invalid(
                                "Keep the saved profiles, required tools, and permissions.",
                            ));
                        }
                    } else {
                        let source = |m: &ManagedFile| -> Result<PathBuf> {
                            let path = m
                                .source_for(host.platform)
                                .ok_or_else(|| invalid("Enter a repository source."))?;
                            Ok(repository.join(path))
                        };
                        let target = host.target_path(&previous.target, Some(repository))?;
                        let previous_source = source(previous)?;
                        if previous.target != file.target || previous_source != source(file)? {
                            let before = observation::inspect(&target, false)?;
                            if observation::link_path(&before).as_ref() == Some(&previous_source) {
                                return Err(invalid(
                                    "Keep the current managed file before changing its source or destination. Then add the mapping again.",
                                ));
                            }
                            watched_target = Some(before);
                        }
                    }
                }
                if previous != Some(file) {
                    edit_file(&mut doc, &manifest, file)?;
                }
                if content.renders() {
                    generated_id = Some(file.id.as_str());
                    let mut description = format!("Save generated file {:?}.", file.id);
                    if content == Content::File {
                        description.push_str(&format!(
                            "\nTemplate file: {:?}\nDort writes dort.toml only. The template file and Home file stay as they are.",
                            file.source_for(host.platform).unwrap_or(Path::new(""))
                        ));
                    }
                    if file.local_inputs.is_some()
                        || previous.is_some_and(|mapping| mapping.local_inputs.is_some())
                    {
                        let names = file.local_inputs.as_deref().unwrap_or_default();
                        description.push_str(&format!(
                            "\nHome destination: {:?}\nDeclared local inputs: {}",
                            file.target,
                            if names.is_empty() {
                                "None".into()
                            } else {
                                names.join(", ")
                            },
                        ));
                    }
                    description
                } else {
                    let override_path = |p: Platform| {
                        file.overrides
                            .get(&p)
                            .map_or_else(|| "Use source".into(), |p| format!("{p:?}"))
                    };
                    format!(
                        "Save file mapping {:?}.\nSource: {:?}\nHome destination: {:?}\nProfiles: {}\nRequires: {}\nmacOS source: {}\nLinux source: {}",
                        file.id,
                        file.source
                            .as_deref()
                            .ok_or_else(|| invalid("Enter a repository source."))?,
                        file.target,
                        file.profiles.join(", "),
                        if file.requires.is_empty() {
                            "None".into()
                        } else {
                            file.requires.join(", ")
                        },
                        override_path(Platform::Macos),
                        override_path(Platform::Linux)
                    )
                }
            }
        };
        let replacement = if matches!(change, Change::File(change) if change.before.as_ref() == Some(&change.mapping))
        {
            text.to_owned()
        } else {
            doc.to_string()
        };
        let parsed = Manifest::parse(&replacement).map_err(|error| match error {
            Error::Parse(_) => invalid(
                "Dort could not prepare this change to dort.toml. No changes were made. If it happens again, report it as a Dort bug.",
            ),
            error => error,
        })?;
        parsed.validate_file_targets(host, &parsed.profiles.known(), Some(repository), None)?;
        let (generated, watched_source) = match generated_id {
            Some(id) => {
                let (preview, source) = generated::Preview::from_manifest(
                    host,
                    repository,
                    Some(repository_id),
                    &parsed,
                    id,
                )?;
                (
                    Some(preview),
                    source.map(|mut entry| {
                        entry.sha256 = None;
                        entry
                    }),
                )
            }
            None => (None, None),
        };
        Ok(Self {
            generated,
            repository: repository.to_owned(),
            backup_root: repository_state_path(host, repository_id)?.join("manifest-edits"),
            repair_git: None,
            config,
            manifest: entry,
            watched_target,
            watched_source,
            original,
            replacement,
            description,
        })
    }

    pub fn generated(&self) -> Option<&generated::Preview> {
        self.generated.as_ref()?.as_ref().ok()
    }

    pub fn local_input_setup(&self) -> Option<&generated::LocalInputSetup> {
        self.generated.as_ref()?.as_ref().err()
    }

    pub fn unchanged(&self) -> bool {
        self.original == self.replacement.as_bytes()
    }

    pub fn human(&self) -> String {
        let mut review = format!(
            "Repository: {:?}\n\n{}\n\nUpdate dort.toml only. Home files and packages stay in place. The previous manifest is backed up privately.",
            self.repository, self.description
        );
        if let Some(setup) = self.local_input_setup() {
            review.push_str("\n\nNo rendered preview yet. Save the declaration only, then open Local inputs on this file in Generated.\n");
            review.push_str(setup.guidance());
        }
        review
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }
}

impl Approval {
    pub fn execute(self, host: &HostContext) -> Result<Option<PathBuf>> {
        let p = self.0;
        let lock = WriterLock::acquire(host)?;
        lock.check_context(host)?;
        journal::ensure_writable(host)?;
        let path = &p.manifest.path;
        let compare = || -> Result<()> {
            let config = match &p.repair_git {
                Some(git) => verify_repair_scope(host, &p.repository, git)?,
                None => LocalConfig::load_bound(host)?.1,
            };
            if !same_entry(&p.config, &config) {
                return Err(invalid("Repository settings changed. Review again."));
            }
            let (_, current) = observation::read_manifest_bytes(path)?;
            if !same_entry(&p.manifest, &current) {
                return Err(invalid("Manifest changed. Review again."));
            }
            if let Some(before) = &p.watched_target
                && !same_entry(before, &observation::inspect(&before.path, false)?)
            {
                return Err(invalid("The Home destination changed. Review again."));
            }
            if let Some(source) = &p.watched_source {
                let current = observation::inspect(&source.path, false).map_err(|_| {
                    invalid("Cannot read the template file. Review the generated file again.")
                })?;
                if !same_entry(source, &current) {
                    return Err(invalid(
                        "The template file changed. Review the generated file again.",
                    ));
                }
            }
            Ok(())
        };
        compare()?;
        if p.original == p.replacement.as_bytes() {
            return Ok(None);
        }
        let handle = observation::parent_handle(path)?;
        let fingerprint = p.manifest.fingerprint.as_ref().filter(|f| f.links == 1 && f.uid == process::geteuid().as_raw() && f.mode & 0o7000 == 0)
            .ok_or_else(|| invalid("The manifest must be an owned regular file without hard links or special permissions."))?;
        let parent = fs::fstat(&handle.directory)
            .map_err(|e| io_error("inspect manifest parent", path, e))?;
        if parent.st_uid != process::geteuid().as_raw() || parent.st_mode & 0o022 != 0 {
            return Err(invalid(
                "The repository folder must be owned and not group/world writable.",
            ));
        }
        let backup_path = p.backup_root.join(transaction_id());
        let backup = PrivateDir::open(&backup_path, true)?.expect("created backup directory");
        let device = fs::fstat(&backup.file)
            .map_err(|e| io_error("inspect backup device", &backup_path, e))?
            .st_dev;
        if fingerprint.device != device as u64 {
            return Err(invalid(
                "Manifest and Dort state must be on the same filesystem.",
            ));
        }
        backup.create("before.toml", &p.original)?;
        backup.create("replacement.toml", p.replacement.as_bytes())?;
        let file = std::fs::File::from(
            fs::openat(
                &backup.file,
                "replacement.toml",
                fs::OFlags::RDONLY | fs::OFlags::NOFOLLOW,
                fs::Mode::empty(),
            )
            .map_err(|e| io_error("open replacement", &backup_path, e))?,
        );
        fs::fchown(&file, None, Some(process::Gid::from_raw(fingerprint.gid)))
            .map_err(|e| io_error("preserve manifest group", path, e))?;
        fs::fchmod(
            &file,
            fs::Mode::from_bits_truncate((fingerprint.mode & 0o777) as _),
        )
        .map_err(|e| io_error("preserve manifest permissions", path, e))?;
        file.sync_all()
            .map_err(|e| io_error("sync replacement", path, e))?;
        compare()?;
        if handle.parents.len() != p.manifest.parents.len()
            || !handle
                .parents
                .iter()
                .zip(&p.manifest.parents)
                .all(|(a, b)| a.path == b.path && parent_identity(&a.fingerprint, &b.fingerprint))
        {
            return Err(invalid("Repository folder changed. Review again."));
        }
        fs::renameat_with(
            &backup.file,
            "replacement.toml",
            &handle.directory,
            &handle.name,
            fs::RenameFlags::EXCHANGE,
        )
        .map_err(|e| io_error("replace manifest", path, e))?;
        fs::renameat(
            &backup.file,
            "replacement.toml",
            &backup.file,
            "previous-entry.toml",
        )
        .map_err(|e| io_error("keep previous manifest", &backup_path, e))?;
        handle
            .directory
            .sync_all()
            .map_err(|e| io_error("sync manifest folder", path, e))?;
        backup.sync()?;
        Ok(Some(backup_path.join("before.toml")))
    }
}
