//! Conversion of one link mapping into a generated file.
use super::{
    Mode, TEMPLATE_LIMIT,
    deploy::{checked_parent, destination_handle},
    find_file, read_template,
    record::{self, DeploymentRecord, Slot, Stamp},
};
use crate::{
    Error, Result, adoption,
    context::HostContext,
    error::io_error,
    journal,
    local::LocalConfig,
    manifest::paths_overlap,
    manifest_edit,
    model::{FileMode, ManagedFile},
    observation::{self, Entry, Fingerprint, Kind, LinkState, Parent, ParentHandle},
    profiles,
    state::{PrivateDir, WriterLock},
    template_files::Folder,
};
use rustix::{fs, io::Errno, process};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

const SCHEMA: u32 = 1;
const PENDING: &str = "generated-convert.json";
const INTENT_LIMIT: u64 = 256 * 1024;
const DIRECTORY: &str = "templates";
const FOLDER_PERMISSIONS: u32 = 0o755;
const TEMPLATE_PERMISSIONS: u32 = 0o644;
const STAGE: &str = ".dort-generated-convert-";
const DIRECTORY_FLAGS: fs::OFlags = fs::OFlags::RDONLY
    .union(fs::OFlags::DIRECTORY)
    .union(fs::OFlags::NOFOLLOW)
    .union(fs::OFlags::CLOEXEC);

/// One step of a conversion. Steps run in this order, and a finished step is never reversed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStep {
    TemplateFile,
    Destination,
    Mapping,
}

const STEPS: [ConversionStep; 3] = [
    ConversionStep::TemplateFile,
    ConversionStep::Destination,
    ConversionStep::Mapping,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    repository_id: String,
    profiles: BTreeSet<String>,
    local: Entry,
    manifest: Entry,
    destination_parents: Vec<Parent>,
    template_parents: Vec<Parent>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderRecord {
    path: PathBuf,
    permissions: u32,
}

#[derive(Debug)]
pub struct Convert {
    host: HostContext,
    explicit: Option<PathBuf>,
    temporary_profiles: Vec<String>,
    prepared: Prepared,
}

#[derive(Debug)]
struct Prepared {
    repository: PathBuf,
    scope: Scope,
    slot: Slot,
    file: ManagedFile,
    source: Entry,
    bytes: Vec<u8>,
    destination: PathBuf,
    template: PathBuf,
    permissions: u32,
    folder: Option<Folder>,
}

/// The reviewed conversion, with the source bytes and the bytes Dort will write.
#[derive(Serialize)]
pub struct ConvertReview<'a> {
    pub repository: &'a Path,
    pub active_profiles: &'a BTreeSet<String>,
    pub id: &'a str,
    pub source: &'a Path,
    pub destination: &'a Path,
    /// Where the symlink points. The deployment record keeps this path.
    pub retained: &'a Path,
    /// Repository-relative template file this conversion creates.
    pub template: &'a Path,
    /// Set when the review creates `templates/` first.
    pub folder: Option<&'a Path>,
    pub before: &'a str,
    pub after: &'a str,
    pub template_permissions: String,
    pub permissions: String,
}

pub struct ConvertApproval(Convert);

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Converted {
    pub destination: PathBuf,
    /// Absolute template file the conversion created.
    pub template: PathBuf,
    /// The old symlink target. Its contents are unchanged.
    pub retained: PathBuf,
    pub permissions: String,
}

/// A conversion Dort recorded in private state that may not have finished.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ConversionLocation {
    pub destination: PathBuf,
    pub template: PathBuf,
    pub retained: PathBuf,
    pub completed: Vec<ConversionStep>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[must_use]
pub enum ConvertOutcome {
    Converted(Converted),
    NeedsRecovery {
        location: ConversionLocation,
        completed: Vec<ConversionStep>,
        reason: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    repository: PathBuf,
    scope: Scope,
    file: ManagedFile,
    source: Entry,
    destination: PathBuf,
    template: PathBuf,
    permissions: u32,
    digest: String,
    size: u64,
    folder: Option<FolderRecord>,
    completed: Vec<ConversionStep>,
}

/// Test hook at each step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundary {
    Folder,
    TemplateFile,
    Exchanged,
    Mapping,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

fn stale() -> Error {
    invalid(
        "The conversion review changed. Open a new review. Dort keeps every step it already finished.",
    )
}

fn conflict() -> Error {
    invalid(
        "The destination, template file or dort.toml changed in a way this conversion did not record. Check all three, then run recovery.",
    )
}

impl Convert {
    pub fn prepare(
        host: &HostContext,
        explicit: Option<&Path>,
        id: &str,
        temporary_profiles: &[String],
    ) -> Result<Self> {
        ensure_no_pending_conversion(host)?;
        Ok(Self {
            host: host.clone(),
            explicit: explicit.map(Path::to_owned),
            temporary_profiles: temporary_profiles.to_vec(),
            prepared: inspect(host, explicit, id, temporary_profiles)?,
        })
    }

    pub fn review(&self) -> ConvertReview<'_> {
        let p = &self.prepared;
        let text = std::str::from_utf8(&p.bytes).expect("validated UTF-8 source");
        ConvertReview {
            repository: &p.repository,
            active_profiles: &p.scope.profiles,
            id: &p.file.id,
            source: &p.source.path,
            destination: &p.destination,
            retained: &p.source.path,
            template: &p.template,
            folder: p.folder.as_ref().map(|folder| folder.path.as_path()),
            before: text,
            after: text,
            template_permissions: Mode(TEMPLATE_PERMISSIONS).label(),
            permissions: Mode(p.permissions).label(),
        }
    }

    pub fn approve(self) -> ConvertApproval {
        ConvertApproval(self)
    }

    fn verify(&self) -> Result<()> {
        let p = &self.prepared;
        let now = inspect(
            &self.host,
            self.explicit.as_deref(),
            &p.file.id,
            &self.temporary_profiles,
        )
        .map_err(|_| stale())?;
        if now.scope != p.scope
            || now.file != p.file
            || now.source != p.source
            || now.bytes != p.bytes
            || now.destination != p.destination
            || now.template != p.template
            || now.permissions != p.permissions
            || now.folder != p.folder
        {
            return Err(stale());
        }
        Ok(())
    }
}

impl ConvertApproval {
    pub fn execute(self) -> Result<ConvertOutcome> {
        self.execute_at(|_| Ok(()))
    }

    fn execute_at(
        self,
        mut at: impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<ConvertOutcome> {
        let c = self.0;
        let _lock = WriterLock::acquire(&c.host)?;
        journal::ensure_writable(&c.host)?;
        c.verify()?;
        ensure_no_pending_conversion(&c.host)?;
        let state = open_locked_state(&c.host)?;
        let p = c.prepared;
        let mut intent = Intent {
            schema: SCHEMA,
            repository: p.repository,
            scope: p.scope,
            file: p.file,
            source: p.source,
            destination: p.destination,
            template: p.template,
            permissions: p.permissions,
            digest: journal::digest(&p.bytes),
            size: p.bytes.len() as u64,
            folder: p.folder.map(|folder| FolderRecord {
                path: folder.path,
                permissions: folder.permissions,
            }),
            completed: Vec::new(),
        };
        intent.persist(&state)?;
        let result = intent.run(&c.host, &state, &p.slot, &mut at);
        finish(&state, &intent, result)
    }
}

/// Record the outcome and clear the intent only when every step finished.
fn finish(
    state: &PrivateDir,
    intent: &Intent,
    result: Result<Converted>,
) -> Result<ConvertOutcome> {
    match result {
        Ok(converted) => {
            clear_pending(state)?;
            Ok(ConvertOutcome::Converted(converted))
        }
        Err(reason) => Ok(ConvertOutcome::NeedsRecovery {
            location: intent.location(),
            completed: intent.completed.clone(),
            reason: reason.to_string(),
        }),
    }
}

fn inspect(
    host: &HostContext,
    explicit: Option<&Path>,
    id: &str,
    temporary_profiles: &[String],
) -> Result<Prepared> {
    let (local, config) = LocalConfig::load_bound(host)?;
    let (repository_id, repository) = match explicit {
        Some(root) => {
            observation::validate_output_path(root)?;
            local.repositories.iter().find(|(_, r)| r.root == root)
        }
        None => local
            .active_repository
            .as_ref()
            .and_then(|id| local.repositories.get_key_value(id)),
    }
    .ok_or_else(|| invalid("Choose a registered repository by its physical path."))?;
    let repository = &repository.root;
    let (manifest, entry) =
        observation::read_manifest(&repository.join("dort.toml")).map_err(|error| match error {
            Error::Parse(_) | Error::Validation(_) => {
                invalid("Invalid dort.toml. Correct the declaration before conversion.")
            }
            other => other,
        })?;
    trusted_parents(&entry.parents)?;
    let file = find_file(&manifest, id)?;
    if file.mode != FileMode::Link {
        return Err(invalid(
            "Conversion needs an existing file with mode = \"link\".",
        ));
    }
    if !file.overrides.is_empty() {
        return Err(invalid(
            "This mapping uses one source per platform. Remove the platform sources before conversion.",
        ));
    }
    let profiles = profiles::active(&local.profile_rows(
        &manifest.profiles,
        host.platform,
        Some(repository),
        temporary_profiles,
    )?);
    if !profiles::selected(&file.profiles, &profiles) {
        return Err(invalid(
            "This file is in an inactive profile. Activate its profile before conversion.",
        ));
    }
    if !file.requires.is_empty() {
        return Err(invalid(
            "This file requires declared tools. Dort cannot verify those requirements for a generated file.",
        ));
    }
    manifest.validate_selection(host, &profiles, Some(repository))?;
    let source_path = repository.join(
        file.source_for(host.platform)
            .ok_or_else(|| invalid("A link mapping needs a repository source."))?,
    );
    let (text, source) =
        read_template(&source_path, repository, &manifest.files).map_err(|error| match error {
            Error::Invalid(message) => invalid(format!(
                "{message} A conversion needs an owned source file in the repository, linked to one name."
            )),
            other => other,
        })?;
    trusted_parents(&source.parents)?;
    render_error(id, &text)?;
    let permissions = source
        .fingerprint
        .as_ref()
        .expect("validated source fingerprint")
        .mode
        & 0o777;
    if !Mode(permissions).permits_update() {
        return Err(invalid(format!(
            "Source permissions {permissions:04o} must let you read the file and must not let group or others write it. Change the source permissions, then review again."
        )));
    }
    let destination = host.target_path(&file.target, Some(repository))?;
    let destination_handle = destination_handle(host, &local, &destination)?;
    require_link(&destination, &source_path)?;
    let template = template_path(id)?;
    check_template_path(&manifest, repository, &template)?;
    let absolute = repository.join(&template);
    let handle = trusted(repository, &absolute)?;
    let folder = handle.missing.then(|| Folder {
        path: PathBuf::from(DIRECTORY),
        permissions: FOLDER_PERMISSIONS,
    });
    if entry_in(&handle, &absolute)?.is_some() {
        return Err(invalid(format!(
            "A file already exists at {}. Move it away, then review again.",
            template.display()
        )));
    }
    let mut destination_parents = destination_handle.parents;
    observation::stabilize_parents(&mut destination_parents);
    let mut template_parents = handle.parents;
    observation::stabilize_parents(&mut template_parents);
    let bytes = text.into_bytes();
    Ok(Prepared {
        repository: repository.clone(),
        scope: Scope {
            repository_id: repository_id.clone(),
            profiles,
            local: config.stable_parents(),
            manifest: entry.stable_parents(),
            destination_parents,
            template_parents,
        },
        slot: Slot::new(host, repository_id, &file.id, &destination)?,
        file: file.clone(),
        source,
        bytes,
        destination,
        template,
        permissions,
        folder,
    })
}

/// Rendering must reproduce the source bytes, so template syntax refuses.
fn render_error(id: &str, text: &str) -> Result<()> {
    for delimiter in ["{{", "{%", "{#"] {
        if let Some(index) = text.find(delimiter) {
            let line = text[..index].matches('\n').count() + 1;
            return Err(invalid(format!(
                "Line {line} of the source contains {delimiter}. Rendering would change the bytes, so Dort refuses to convert {id}. Remove the template syntax, or declare the generated file by hand."
            )));
        }
    }
    Ok(())
}

fn require_link(destination: &Path, source: &Path) -> Result<()> {
    let entry = observation::inspect(destination, false)?;
    if entry.sensitive {
        return Err(invalid(
            "The destination path looks sensitive. Dort leaves it alone.",
        ));
    }
    if entry.kind != Kind::Symlink {
        return Err(invalid(format!(
            "Destination {} is not the managed symlink. Dort converts only the current link.",
            destination.display()
        )));
    }
    if entry
        .fingerprint
        .as_ref()
        .is_none_or(|fp| fp.links != 1 || fp.uid != process::geteuid().as_raw())
    {
        return Err(invalid(
            "The destination symlink must be owned by you and not hard-linked.",
        ));
    }
    if observation::link_path(&entry).as_deref() != Some(source) {
        return Err(invalid(
            "The destination symlink points somewhere else than the mapping source. Dort converts only the managed link.",
        ));
    }
    if observation::link_state(&entry)? != LinkState::Regular {
        return Err(invalid(
            "The destination symlink does not resolve to the source file. Repair the link, then review again.",
        ));
    }
    Ok(())
}

fn template_path(id: &str) -> Result<PathBuf> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.eq_ignore_ascii_case(".git");
    if !valid {
        return Err(invalid(
            "This file ID cannot name a template file. Use letters, digits, dots, dashes and underscores, and do not start with a dot.",
        ));
    }
    Ok(PathBuf::from(DIRECTORY).join(format!("{id}.jinja")))
}

fn check_template_path(
    manifest: &crate::Manifest,
    repository: &Path,
    template: &Path,
) -> Result<()> {
    if !template
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(invalid("Use a safe template path."));
    }
    let absolute = repository.join(template);
    if observation::sensitive_path(&absolute) {
        return Err(invalid(
            "The template path looks sensitive. Choose a different file ID.",
        ));
    }
    let mut reserved = vec![
        repository.join("dort.toml"),
        repository.join("recipients.toml"),
    ];
    for file in &manifest.files {
        reserved.extend(file.source_paths().map(|path| repository.join(path)));
    }
    if reserved.iter().any(|other| paths_overlap(other, &absolute)) {
        return Err(invalid(format!(
            "{} is already used by another mapping or a reserved file. Choose a different file ID.",
            template.display()
        )));
    }
    Ok(())
}

/// Trusted folders, as the template file rules require.
fn trusted(repository: &Path, path: &Path) -> Result<ParentHandle> {
    observation::parent_handle_with(path, |dir, path| {
        let stat = fs::fstat(dir).map_err(|error| io_error("inspect folder", path, error))?;
        let uid = process::geteuid().as_raw();
        let trusted = if path.starts_with(repository) {
            stat.st_uid == uid && stat.st_mode & 0o7022 == 0
        } else {
            (stat.st_uid == uid || stat.st_uid == 0)
                && (stat.st_mode & 0o022 == 0 || stat.st_mode & 0o1000 != 0)
        };
        if !trusted {
            return Err(invalid(format!(
                "Folder {} is not trusted. Use a repository owned by you and not writable by group or others.",
                path.display()
            )));
        }
        Ok(())
    })
}

fn trusted_parents(parents: &[Parent]) -> Result<()> {
    let user = process::geteuid().as_raw();
    for (index, parent) in parents.iter().enumerate() {
        let fp = &parent.fingerprint;
        let owned = fp.uid == user || fp.uid == 0;
        let sticky_ancestor = fp.mode & 0o1000 != 0 && index + 1 < parents.len();
        if !owned || (fp.mode & 0o022 != 0 && !sticky_ancestor) {
            return Err(invalid(format!(
                "Folder {} is not trusted. Use a folder owned by you or root and not writable by group or others.",
                parent.path.display()
            )));
        }
    }
    Ok(())
}

/// Live parents may include the folder this review creates.
fn reviewed_parents(reviewed: &[Parent], live: &[Parent]) -> bool {
    reviewed.len() <= live.len()
        && observation::same_parent_identity_and_access(reviewed, &live[..reviewed.len()])
}

fn entry_in(handle: &ParentHandle, path: &Path) -> Result<Option<Fingerprint>> {
    if handle.missing {
        return Ok(None);
    }
    match fs::statat(
        &handle.directory,
        &handle.name,
        fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Ok(stat) => Ok(Some(Fingerprint::from_stat(&stat))),
        Err(Errno::NOENT) => Ok(None),
        Err(error) => Err(io_error("inspect file", path, error)),
    }
}

enum Placement {
    Reviewed,
    Ours,
    Foreign,
}

/// Whether a path still holds what the review saw, or bytes Dort already wrote.
fn placement(handle: &ParentHandle, path: &Path, digest: &str, size: u64) -> Result<Placement> {
    if handle.missing {
        return Ok(Placement::Reviewed);
    }
    let stat = match fs::statat(
        &handle.directory,
        &handle.name,
        fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(Placement::Reviewed),
        Err(error) => return Err(io_error("inspect file", path, error)),
    };
    let fingerprint = Fingerprint::from_stat(&stat);
    if !record::owned_regular(&stat, &fingerprint)
        || fingerprint.size < 0
        || fingerprint.size as u64 != size
    {
        return Ok(Placement::Foreign);
    }
    let bytes =
        observation::read_checked_bounded(handle, path, &fingerprint, TEMPLATE_LIMIT as u64)?;
    Ok(if journal::digest(&bytes) == digest {
        Placement::Ours
    } else {
        Placement::Foreign
    })
}

impl Intent {
    fn bytes(&self) -> Result<Vec<u8>> {
        let fingerprint = self
            .source
            .fingerprint
            .as_ref()
            .ok_or_else(invalid_intent)?;
        let handle = observation::parent_handle(&self.source.path)?;
        if !observation::same_parent_identity_and_access(&handle.parents, &self.source.parents) {
            return Err(stale());
        }
        let bytes = observation::read_checked_bounded(
            &handle,
            &self.source.path,
            fingerprint,
            TEMPLATE_LIMIT as u64,
        )?;
        if bytes.len() as u64 != self.size || journal::digest(&bytes) != self.digest {
            return Err(stale());
        }
        Ok(bytes)
    }

    fn receipt(&self, bytes: &[u8]) -> DeploymentRecord {
        DeploymentRecord {
            deployed: Stamp::new(bytes, Mode(self.permissions)),
            retained: Some(self.source.path.clone()),
            recreation: None,
        }
    }

    fn location(&self) -> ConversionLocation {
        ConversionLocation {
            destination: self.destination.clone(),
            template: self.repository.join(&self.template),
            retained: self.source.path.clone(),
            completed: self.completed.clone(),
        }
    }

    fn persist(&self, state: &PrivateDir) -> Result<()> {
        let text = serde_json::to_string(self).map_err(|_| invalid_intent())?;
        if text.len() as u64 > INTENT_LIMIT {
            return Err(invalid_intent());
        }
        state.replace(PENDING, &text)
    }

    fn complete(&mut self, step: ConversionStep, state: &PrivateDir) -> Result<()> {
        self.completed.push(step);
        self.persist(state)
    }

    fn run(
        &mut self,
        host: &HostContext,
        state: &PrivateDir,
        slot: &Slot,
        at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<Converted> {
        if !self.completed.contains(&ConversionStep::TemplateFile) {
            self.ensure_template(at)?;
            self.complete(ConversionStep::TemplateFile, state)?;
        }
        if !self.completed.contains(&ConversionStep::Destination) {
            self.ensure_destination(slot, at)?;
            self.complete(ConversionStep::Destination, state)?;
        }
        if !self.completed.contains(&ConversionStep::Mapping) {
            self.ensure_mapping(host, at)?;
            self.complete(ConversionStep::Mapping, state)?;
        }
        Ok(Converted {
            destination: self.destination.clone(),
            template: self.repository.join(&self.template),
            retained: self.source.path.clone(),
            permissions: Mode(self.permissions).label(),
        })
    }

    fn ensure_template(
        &self,
        at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<()> {
        let absolute = self.repository.join(&self.template);
        let handle = trusted(&self.repository, &absolute)?;
        if !reviewed_parents(&self.scope.template_parents, &handle.parents) {
            return Err(stale());
        }
        match placement(&handle, &absolute, &self.digest, self.size)? {
            Placement::Ours => return Ok(()),
            Placement::Foreign => {
                return Err(invalid(format!(
                    "Something else is at {}. Move it away, then run recovery.",
                    self.template.display()
                )));
            }
            Placement::Reviewed => {}
        }
        if handle.missing {
            let folder = self.folder.as_ref().ok_or_else(|| {
                invalid(format!(
                    "The {DIRECTORY}/ folder is missing. Create it, then run recovery."
                ))
            })?;
            let path = self.repository.join(&folder.path);
            let folder_handle = trusted(&self.repository, &path)?;
            if entry_in(&folder_handle, &path)?.is_some() {
                return Err(invalid(format!(
                    "Something now exists at {}. Review the conversion again.",
                    path.display()
                )));
            }
            create_folder(&folder_handle, &path, folder.permissions)?;
            at(Boundary::Folder)
                .map_err(|error| io_error("create template folder", &path, error))?;
        }
        let handle = trusted(&self.repository, &absolute)?;
        if handle.missing
            || !reviewed_parents(&self.scope.template_parents, &handle.parents)
            || entry_in(&handle, &absolute)?.is_some()
        {
            return Err(stale());
        }
        let bytes = self.bytes()?;
        write_new(&handle, &absolute, &bytes, TEMPLATE_PERMISSIONS, at)
    }

    fn ensure_destination(
        &mut self,
        slot: &Slot,
        at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<()> {
        let handle = checked_parent(&self.destination)?;
        if handle.missing
            || !observation::same_parent_identity_and_access(
                &handle.parents,
                &self.scope.destination_parents,
            )
        {
            return Err(stale());
        }
        let stat = match fs::statat(
            &handle.directory,
            &handle.name,
            fs::AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Err(stale()),
            Err(error) => return Err(io_error("inspect destination", &self.destination, error)),
        };
        let bytes = self.bytes()?;
        if fs::FileType::from_raw_mode(stat.st_mode) == fs::FileType::Symlink {
            require_link(&self.destination, &self.source.path)?;
            exchange(&handle, &self.destination, &bytes, self.permissions, at)?;
        } else {
            let fingerprint = Fingerprint::from_stat(&stat);
            if !record::owned_regular(&stat, &fingerprint)
                || fingerprint.mode & 0o7777 != self.permissions
                || journal::digest(&bytes) != self.digest
                || !matches_live(&handle, &self.destination, &fingerprint, &bytes)?
            {
                return Err(conflict());
            }
        }
        ensure_receipt(slot, &self.receipt(&bytes))
    }

    fn ensure_mapping(
        &self,
        host: &HostContext,
        at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
    ) -> Result<()> {
        let (manifest, entry) = observation::read_manifest(&self.repository.join("dort.toml"))?;
        if entry.stable_parents() != self.scope.manifest {
            return Err(stale());
        }
        let file = find_file(&manifest, &self.file.id)?;
        let permissions = Mode(self.permissions).label();
        if file.mode == FileMode::Template {
            if file.source.as_deref() == Some(self.template.as_path())
                && file.permissions.as_deref() == Some(permissions.as_str())
            {
                return Ok(());
            }
            return Err(conflict());
        }
        if file != &self.file {
            return Err(stale());
        }
        let mut mapping = file.clone();
        mapping.mode = FileMode::Template;
        mapping.source = Some(self.template.clone());
        mapping.template = None;
        mapping.overrides = Default::default();
        mapping.inputs = Default::default();
        mapping.host_inputs = None;
        mapping.local_inputs = None;
        mapping.permissions = Some(permissions);
        let preview = manifest_edit::Preview::prepare_converted(host, &self.repository, &mapping)?;
        at(Boundary::Mapping).map_err(|error| {
            io_error(
                "rewrite the mapping",
                self.repository.join("dort.toml"),
                error,
            )
        })?;
        preview.approve().execute_locked(host)?;
        Ok(())
    }
}

fn matches_live(
    handle: &ParentHandle,
    path: &Path,
    fingerprint: &Fingerprint,
    expected: &[u8],
) -> Result<bool> {
    let bytes =
        observation::read_checked_bounded(handle, path, fingerprint, TEMPLATE_LIMIT as u64)?;
    Ok(bytes == expected)
}

fn ensure_receipt(slot: &Slot, receipt: &DeploymentRecord) -> Result<()> {
    match slot.load().map_err(|_| record::invalid())? {
        Some(current) if current == *receipt => Ok(()),
        Some(_) => slot.replace(receipt.clone()),
        None => slot.save_record(receipt.clone()),
    }
}

fn create_folder(handle: &ParentHandle, path: &Path, permissions: u32) -> Result<()> {
    let mode = fs::Mode::from_bits_truncate(permissions as _);
    fs::mkdirat(&handle.directory, &handle.name, mode).map_err(|error| match error {
        Errno::EXIST => invalid("Something now exists at templates/. Review the conversion again."),
        error => io_error("create template folder", path, error),
    })?;
    let directory = File::from(
        fs::openat(
            &handle.directory,
            &handle.name,
            DIRECTORY_FLAGS,
            fs::Mode::empty(),
        )
        .map_err(|error| io_error("open new template folder", path, error))?,
    );
    fs::fchmod(&directory, mode)
        .map_err(|error| io_error("set template folder permissions", path, error))?;
    let stat = fs::fstat(&directory)
        .map_err(|error| io_error("inspect new template folder", path, error))?;
    if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::Directory
        || stat.st_uid != process::geteuid().as_raw()
        || (stat.st_mode as u32) & 0o7777 != permissions
    {
        return Err(invalid(
            "The new templates/ folder has unexpected permissions or owner.",
        ));
    }
    let _ = handle.directory.sync_all();
    Ok(())
}

fn write_new(
    handle: &ParentHandle,
    path: &Path,
    bytes: &[u8],
    permissions: u32,
    at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
) -> Result<()> {
    let mut file = File::from(
        fs::openat(
            &handle.directory,
            &handle.name,
            fs::OFlags::WRONLY
                | fs::OFlags::CREATE
                | fs::OFlags::EXCL
                | fs::OFlags::NOFOLLOW
                | fs::OFlags::CLOEXEC,
            fs::Mode::RUSR | fs::Mode::WUSR,
        )
        .map_err(|error| match error {
            Errno::EXIST => invalid(format!("A file appeared at {}.", path.display())),
            error => io_error("create template file", path, error),
        })?,
    );
    file.write_all(bytes)
        .map_err(|error| io_error("write template file", path, error))?;
    fs::fchmod(&file, fs::Mode::from_bits_truncate(permissions as _))
        .map_err(|error| io_error("set template file permissions", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync template file", path, error))?;
    at(Boundary::TemplateFile).map_err(|error| io_error("write template file", path, error))?;
    let stat = fs::fstat(&file).map_err(|error| io_error("inspect template file", path, error))?;
    if Fingerprint::from_stat(&stat).mode & 0o7777 != permissions {
        return Err(stale());
    }
    let _ = handle.directory.sync_all();
    Ok(())
}

fn exchange(
    handle: &ParentHandle,
    destination: &Path,
    bytes: &[u8],
    permissions: u32,
    at: &mut impl FnMut(Boundary) -> rustix::io::Result<()>,
) -> Result<()> {
    let temporary = format!("{STAGE}{}", adoption::transaction_id());
    let directory = &handle.directory;
    let mut file = File::from(
        fs::openat(
            directory,
            temporary.as_str(),
            fs::OFlags::WRONLY
                | fs::OFlags::CREATE
                | fs::OFlags::EXCL
                | fs::OFlags::NOFOLLOW
                | fs::OFlags::CLOEXEC,
            fs::Mode::RUSR | fs::Mode::WUSR,
        )
        .map_err(|error| io_error("stage rendered file", destination, error))?,
    );
    let staged = (|| -> Result<()> {
        file.write_all(bytes)
            .map_err(|error| io_error("write staged rendered file", destination, error))?;
        fs::fchmod(&file, fs::Mode::from_bits_truncate(permissions as _))
            .map_err(|error| io_error("set rendered file permissions", destination, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync staged rendered file", destination, error))?;
        let stat = fs::fstat(&file)
            .map_err(|error| io_error("inspect staged rendered file", destination, error))?;
        if Fingerprint::from_stat(&stat).mode & 0o7777 != permissions {
            return Err(stale());
        }
        at(Boundary::Exchanged)
            .map_err(|error| io_error("prepare the destination exchange", destination, error))
    })();
    if let Err(error) = staged {
        let _ = fs::unlinkat(directory, temporary.as_str(), fs::AtFlags::empty());
        return Err(error);
    }
    fs::renameat_with(
        directory,
        temporary.as_str(),
        directory,
        &handle.name,
        fs::RenameFlags::EXCHANGE,
    )
    .map_err(|error| {
        io_error(
            "replace the symlink with the rendered file",
            destination,
            error,
        )
    })?;
    // The stage now holds the old symlink. The receipt keeps its target.
    let _ = fs::unlinkat(directory, temporary.as_str(), fs::AtFlags::empty());
    let _ = directory.sync_all();
    Ok(())
}

fn invalid_intent() -> Error {
    invalid(
        "Cannot verify the recorded conversion. Keep Dort's local state and the repository for inspection.",
    )
}

fn open_locked_state(host: &HostContext) -> Result<PrivateDir> {
    PrivateDir::open(&host.state_dir, false)?.ok_or_else(|| {
        invalid(format!(
            "Dort's state folder moved. Check {}.",
            host.state_dir.display()
        ))
    })
}

fn load_pending(state: &PrivateDir, host: &HostContext) -> Result<Option<(Intent, Slot)>> {
    let Some(text) = state.read(PENDING)? else {
        return Ok(None);
    };
    if text.len() as u64 > INTENT_LIMIT {
        return Err(invalid_intent());
    }
    let intent: Intent = serde_json::from_str(&text).map_err(|_| invalid_intent())?;
    let slot = Slot::new(
        host,
        &intent.scope.repository_id,
        &intent.file.id,
        &intent.destination,
    )
    .map_err(|_| invalid_intent())?;
    let known_repository = LocalConfig::load(host).ok().and_then(|config| {
        config
            .repositories
            .get(&intent.scope.repository_id)
            .cloned()
    });
    let ordered = intent
        .completed
        .iter()
        .copied()
        .eq(STEPS.iter().copied().take(intent.completed.len()));
    if intent.schema != SCHEMA
        || known_repository
            .as_ref()
            .is_none_or(|record| record.root != intent.repository)
        || template_path(&intent.file.id).map_or(true, |path| path != intent.template)
        || !record::transaction_id_valid(&intent.digest)
        || intent.size as usize > TEMPLATE_LIMIT
        || intent.source.fingerprint.is_none()
        || intent.permissions > 0o777
        || !Mode(intent.permissions).permits_update()
        || intent.completed.len() > STEPS.len()
        || !ordered
    {
        return Err(invalid_intent());
    }
    Ok(Some((intent, slot)))
}

/// The conversion Dort recorded but has not finished. Shows a partial state.
pub fn pending_conversion(host: &HostContext) -> Result<Option<ConversionLocation>> {
    let Some(state) = PrivateDir::open(&host.state_dir, false)? else {
        return Ok(None);
    };
    Ok(load_pending(&state, host)?.map(|(intent, _)| intent.location()))
}

fn ensure_no_pending_conversion(host: &HostContext) -> Result<()> {
    if let Some(location) = pending_conversion(host)? {
        return Err(invalid(format!(
            "A conversion for {} needs recovery before Dort starts another. Check the template file at {} and the destination.",
            location.destination.display(),
            location.template.display(),
        )));
    }
    Ok(())
}

/// Finish the recorded conversion. Performs only the steps that are not done.
pub fn recover_conversion(host: &HostContext) -> Result<Option<ConvertOutcome>> {
    let _lock = WriterLock::acquire_for_generated_recovery(host)?;
    let state = open_locked_state(host)?;
    let Some((mut intent, slot)) = load_pending(&state, host)? else {
        return Ok(None);
    };
    let mut at = |_: Boundary| Ok(());
    let result = intent.run(host, &state, &slot, &mut at);
    finish(&state, &intent, result).map(Some)
}

fn clear_pending(state: &PrivateDir) -> Result<()> {
    fs::unlinkat(&state.file, PENDING, fs::AtFlags::empty())
        .map_err(|error| io_error("clear recorded conversion", state.path.join(PENDING), error))?;
    state.sync()
}
