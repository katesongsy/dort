use super::{DestinationStatus, Preview, find_file};
use crate::{
    Error, Result, adoption,
    context::HostContext,
    error::io_error,
    journal,
    local::LocalConfig,
    manifest::paths_overlap,
    model::FileMode,
    observation::{self, Entry, Parent, ParentHandle},
    profiles,
    state::WriterLock,
};
use rustix::{fs, io::Errno, process};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Deployment {
    host: HostContext,
    explicit: Option<PathBuf>,
    temporary_profiles: Vec<String>,
    scope: Scope,
    preview: Preview,
    mode: Mode,
}

#[derive(PartialEq, Eq)]
struct Scope {
    repository_id: String,
    profiles: BTreeSet<String>,
    local: Entry,
    manifest: Entry,
    source: Option<Entry>,
    missing_destination: Vec<Parent>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Mode(u32);
impl Mode {
    fn label(self) -> String {
        format!("{:04o}", self.0)
    }
}

#[derive(Serialize)]
pub struct DeploymentReview<'a> {
    pub repository: &'a Path,
    pub active_profiles: &'a BTreeSet<String>,
    pub id: &'a str,
    pub source: Option<&'a Path>,
    pub destination: &'a Path,
    pub rendered: &'a str,
    pub permissions: String,
}

pub struct Approval(Deployment);

#[derive(Serialize)]
pub struct Published {
    pub destination: PathBuf,
    pub permissions: String,
    pub directory_synced: bool,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

impl Deployment {
    pub fn prepare(
        host: &HostContext,
        explicit: Option<&Path>,
        id: &str,
        temporary_profiles: &[String],
    ) -> Result<Self> {
        let (scope, preview, mode) = read_scope(host, explicit, id, temporary_profiles)?;
        Ok(Self {
            host: host.clone(),
            explicit: explicit.map(Path::to_owned),
            temporary_profiles: temporary_profiles.to_vec(),
            scope,
            preview,
            mode,
        })
    }

    pub fn review(&self) -> DeploymentReview<'_> {
        DeploymentReview {
            repository: self.scope.manifest.path.parent().expect("manifest parent"),
            active_profiles: &self.scope.profiles,
            id: &self.preview.id,
            source: self.preview.source.as_deref(),
            destination: &self.preview.destination,
            rendered: &self.preview.rendered,
            permissions: self.mode.label(),
        }
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }
}

impl Approval {
    pub fn execute(self) -> Result<Published> {
        let p = self.0;
        let _lock = WriterLock::acquire(&p.host)?;
        journal::ensure_writable(&p.host)?;
        let path = &p.preview.destination;
        let handle = missing_parent_handle(path)?;
        if !observation::same_parent_identity_and_access(
            &handle.parents,
            &p.scope.missing_destination,
        ) {
            return Err(stale());
        }
        let verify_review = || {
            let (scope, preview, mode) = read_scope(
                &p.host,
                p.explicit.as_deref(),
                &p.preview.id,
                &p.temporary_profiles,
            )
            .map_err(|_| stale())?;
            if scope != p.scope
                || mode != p.mode
                || preview.destination != *path
                || preview.rendered != p.preview.rendered
            {
                return Err(stale());
            }
            Ok(())
        };
        verify_review()?;
        let directory_synced = publish(
            &handle,
            path,
            p.mode,
            p.preview.rendered.as_bytes(),
            verify_review,
        )?;
        Ok(Published {
            destination: p.preview.destination,
            permissions: p.mode.label(),
            directory_synced,
        })
    }
}

fn stale() -> Error {
    invalid("Deployment changed. Open a new review. No generated file was written.")
}

fn read_scope(
    host: &HostContext,
    explicit: Option<&Path>,
    id: &str,
    temporary_profiles: &[String],
) -> Result<(Scope, Preview, Mode)> {
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
                invalid("Invalid dort.toml. Correct the declaration before deployment.")
            }
            other => other,
        })?;
    trusted_parents(&entry.parents)?;
    let file = find_file(&manifest, id)?;
    if file.mode != FileMode::Template {
        return Err(invalid("Choose a file with template mode."));
    }
    let profiles = profiles::active(&local.profile_rows(
        &manifest.profiles,
        host.platform,
        Some(repository),
        temporary_profiles,
    )?);
    if !profiles::selected(&file.profiles, &profiles) {
        return Err(invalid(
            "This file is in an inactive profile. Activate its profile before deployment.",
        ));
    }
    if !file.requires.is_empty() {
        return Err(invalid(
            "This file requires declared tools. First deployment cannot verify tool requirements.",
        ));
    }
    manifest.validate_selection(host, &profiles, Some(repository))?;
    let destination = host.target_path(&file.target, Some(repository))?;
    if observation::sensitive_path(&destination)
        || destination
            .components()
            .any(|part| part.as_os_str().eq_ignore_ascii_case(".git"))
        || local
            .repositories
            .values()
            .any(|record| paths_overlap(&destination, &record.root))
    {
        return Err(invalid(
            "Destination is protected. Choose a non-sensitive path outside registered repositories.",
        ));
    }
    let handle = missing_parent_handle(&destination)?;
    let protected = [&host.config_dir, &host.state_dir]
        .into_iter()
        .chain(local.repositories.values().map(|record| &record.root));
    for root in protected {
        if let Ok(entry) = observation::inspect(root, false)
            && let Some(fp) = entry.fingerprint
            && handle.parents.iter().any(|parent| {
                parent.fingerprint.device == fp.device && parent.fingerprint.inode == fp.inode
            })
        {
            return Err(invalid(
                "Destination overlaps Dort state or a registered repository.",
            ));
        }
    }
    let mode = Mode(match &file.permissions {
        Some(value) => u32::from_str_radix(value, 8).expect("validated manifest permissions"),
        None => 0o600,
    });
    if mode.0 & 0o022 != 0 {
        return Err(invalid(
            "Generated permissions must not allow group or other users to write the file.",
        ));
    }
    let (mut preview, source) =
        Preview::render_file(host, repository, &manifest, file, destination)?;
    if let Some(source) = &source {
        trusted_parents(&source.parents)?;
    }
    if preview.destination_status != DestinationStatus::Missing {
        return Err(stale());
    }
    preview.permissions = Some(mode.label());
    let mut missing_destination = handle.parents;
    observation::stabilize_parents(&mut missing_destination);
    Ok((
        Scope {
            repository_id: repository_id.clone(),
            profiles,
            local: config.stable_parents(),
            manifest: entry.stable_parents(),
            source: source.map(Entry::stable_parents),
            missing_destination,
        },
        preview,
        mode,
    ))
}

fn missing_parent_handle(path: &Path) -> Result<ParentHandle> {
    let handle = observation::parent_handle(path)?;
    if handle.missing {
        return Err(invalid(format!(
            "Destination folder {} is missing. Create it, then review again.",
            path.parent().expect("destination parent").display()
        )));
    }
    trusted_parents(&handle.parents)?;
    match fs::statat(
        &handle.directory,
        &handle.name,
        fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Err(Errno::NOENT) => Ok(handle),
        Ok(_) => Err(invalid(
            "Destination already exists. Choose a missing destination; Dort will not replace it.",
        )),
        Err(error) => Err(io_error("inspect destination", path, error)),
    }
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

fn publish(
    handle: &ParentHandle,
    path: &Path,
    mode: Mode,
    bytes: &[u8],
    verify: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    let temporary = format!(".dort-generated-{}", adoption::transaction_id());
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
        .map_err(|error| io_error("stage generated file", path, error))?,
    );
    let result = (|| {
        file.write_all(bytes)
            .map_err(|error| io_error("write staged generated file", path, error))?;
        fs::fchmod(&file, fs::Mode::from_bits_truncate(mode.0 as _))
            .map_err(|error| io_error("set generated file permissions", path, error))?;
        file.sync_all()
            .map_err(|error| io_error("sync staged generated file", path, error))?;
        let staged = fs::fstat(&file)
            .map_err(|error| io_error("inspect staged generated file", path, error))?;
        verify()?;
        let named = fs::statat(directory, temporary.as_str(), fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| io_error("recheck staged generated file", path, error))?;
        if observation::Fingerprint::from_stat(&named)
            != observation::Fingerprint::from_stat(&staged)
        {
            return Err(stale());
        }
        fs::renameat_with(
            directory,
            temporary.as_str(),
            directory,
            &handle.name,
            fs::RenameFlags::NOREPLACE,
        )
        .map_err(|error| match error {
            Errno::EXIST => {
                invalid("Destination appeared during deployment. Dort did not replace it.")
            }
            other => io_error(
                "publish generated file without replacing a destination",
                path,
                other,
            ),
        })
    })();
    if let Err(error) = result {
        if let Err(cleanup) = fs::unlinkat(directory, temporary.as_str(), fs::AtFlags::empty())
            && cleanup != Errno::NOENT
        {
            return Err(invalid(format!(
                "{error} The staged file {temporary} could not be removed. Check the destination folder."
            )));
        }
        return Err(error);
    }
    Ok(directory.sync_all().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_created_at_the_last_check_is_not_replaced_and_the_stage_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let path = root.join("generated");
        let handle = missing_parent_handle(&path).unwrap();
        let result = publish(&handle, &path, Mode(0o600), b"reviewed", || {
            std::fs::write(&path, b"existing").unwrap();
            Ok(())
        });
        assert!(matches!(result, Err(Error::Invalid(_))));
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
    }
}
