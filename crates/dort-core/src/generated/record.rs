use super::{DriftStatus, Mode, OUTPUT_LIMIT};
use crate::{
    Error, Result,
    context::HostContext,
    error::io_error,
    journal::digest,
    local::repository_state_path,
    observation::{self, Fingerprint, ParentHandle},
    state::PrivateDir,
};
use rustix::{fs, io::Errno, process};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs::File,
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;
const LIMIT: u64 = 32 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub(super) repository_id: String,
    pub(super) mapping_id: String,
    pub(super) destination: String,
}

pub(super) struct Slot {
    folder: PathBuf,
    name: String,
    pub(super) identity: Identity,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stamp {
    sha256: String,
    pub(super) mode: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeploymentRecord {
    pub(super) deployed: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) retained: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) recreation: Option<String>,
}

impl DeploymentRecord {
    pub(super) fn valid(&self) -> bool {
        self.deployed.valid()
            && self
                .recreation
                .as_deref()
                .is_none_or(|id| self.retained.is_none() && transaction_id_valid(id))
    }
}

pub(super) fn transaction_id_valid(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    identity: Identity,
    deployed: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retained: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recreation: Option<String>,
}

impl Slot {
    pub(super) fn new(
        host: &HostContext,
        repo_id: &str,
        mapping_id: &str,
        path: &Path,
    ) -> Result<Self> {
        observation::validate_output_path(path)?;
        let normalized: PathBuf = path.components().collect();
        let identity = Identity {
            repository_id: repo_id.to_owned(),
            mapping_id: mapping_id.to_owned(),
            destination: normalized
                .to_str()
                .expect("validated UTF-8 path")
                .to_owned(),
        };
        let key = serde_json::to_vec(&identity).map_err(|_| invalid())?;
        if key.len() as u64 > LIMIT / 2 {
            return Err(invalid());
        }
        Ok(Self {
            folder: repository_state_path(host, repo_id)?.join("generated"),
            name: format!("{}.json", digest(&key)),
            identity,
        })
    }

    pub(super) fn load(&self) -> Result<Option<DeploymentRecord>> {
        let Some(dir) = PrivateDir::open(&self.folder, false)? else {
            return Ok(None);
        };
        let name = OsStr::new(&self.name);
        let path = self.folder.join(name);
        let stat = match fs::statat(&dir.file, name, fs::AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Ok(None),
            Err(error) => return Err(io_error("inspect generated deployment record", path, error)),
        };
        let fp = Fingerprint::from_stat(&stat);
        if !owned_regular(&stat, &fp) || fp.mode & 0o7077 != 0 {
            return Err(invalid());
        }
        let bytes = observation::read_at_checked(&dir.file, name, &path, &fp, LIMIT)?;
        let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let deployment = DeploymentRecord {
            deployed: receipt.deployed,
            retained: receipt.retained,
            recreation: receipt.recreation,
        };
        if receipt.schema != SCHEMA || receipt.identity != self.identity || !deployment.valid() {
            return Err(invalid());
        }
        Ok(Some(deployment))
    }

    pub(super) fn save(self, deployed: Stamp) -> Result<()> {
        let deployment = DeploymentRecord {
            deployed,
            retained: None,
            recreation: None,
        };
        let text = self.encode(deployment)?;
        let dir = PrivateDir::open(&self.folder, true)?.expect("created deployment records folder");
        dir.create(&self.name, text.as_bytes())
    }

    pub(super) fn replace(&self, deployment: DeploymentRecord) -> Result<()> {
        let text = self.encode(deployment)?;
        self.directory()?.replace(&self.name, &text)
    }

    pub(super) fn sync(&self) -> Result<()> {
        self.directory()?.sync()
    }

    fn directory(&self) -> Result<PrivateDir> {
        PrivateDir::open(&self.folder, false)?.ok_or_else(invalid)
    }

    fn encode(&self, deployment: DeploymentRecord) -> Result<String> {
        let receipt = Receipt {
            schema: SCHEMA,
            identity: self.identity.clone(),
            deployed: deployment.deployed,
            retained: deployment.retained,
            recreation: deployment.recreation,
        };
        let text = serde_json::to_string(&receipt).map_err(|_| invalid())?;
        if text.len() as u64 > LIMIT {
            return Err(invalid());
        }
        Ok(text)
    }
}

pub(super) fn invalid() -> Error {
    Error::Invalid(
        "Cannot verify the deployment record. Check Dort's local state before trying again.".into(),
    )
}

pub(super) fn owned_regular(stat: &fs::Stat, fp: &Fingerprint) -> bool {
    fs::FileType::from_raw_mode(stat.st_mode) == fs::FileType::RegularFile
        && fp.links == 1
        && fp.uid == process::geteuid().as_raw()
}

impl Stamp {
    pub(super) fn new(bytes: &[u8], mode: Mode) -> Self {
        Self {
            sha256: digest(bytes),
            mode: mode.0,
        }
    }

    pub(super) fn valid(&self) -> bool {
        Self::mode_safe(self.mode) && transaction_id_valid(&self.sha256)
    }

    fn mode_safe(mode: u32) -> bool {
        mode <= 0o777 && Mode(mode).permits_deployment()
    }

    pub(super) fn observe(
        &self,
        path: &Path,
        handle: &ParentHandle,
        stat: &fs::Stat,
        proposed: &Self,
    ) -> DriftStatus {
        let (fp, bytes) = match read_tracked(path, handle, stat) {
            Ok(observed) => observed,
            Err(status) => return status,
        };
        self.compare(&Self::new(&bytes, Mode(fp.mode & 0o7777)), proposed)
    }

    pub(super) fn compare(&self, live: &Self, proposed: &Self) -> DriftStatus {
        match (live.sha256 != self.sha256, live.mode != self.mode) {
            (true, true) => DriftStatus::LocallyEditedAndPermissionsChanged,
            (true, false) => DriftStatus::LocallyEdited,
            (false, true) => DriftStatus::LocalPermissionsChanged,
            (false, false) if self == proposed => DriftStatus::UpToDate,
            (false, false) => DriftStatus::UpdateAvailable,
        }
    }
}

pub(super) fn check_file_access(
    dir: &File,
    name: &OsStr,
    path: &Path,
    fp: &Fingerprint,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let file = fs::openat(
            dir,
            name,
            fs::OFlags::RDONLY | fs::OFlags::NOFOLLOW | fs::OFlags::CLOEXEC | fs::OFlags::NONBLOCK,
            fs::Mode::empty(),
        )
        .map(File::from)
        .map_err(|err| io_error("check generated file access", path, err))?;
        let stat =
            fs::fstat(&file).map_err(|error| io_error("inspect generated file", path, error))?;
        if Fingerprint::from_stat(&stat) != *fp {
            return Err(Error::Invalid(
                "The generated file changed. Open a new preview.".into(),
            ));
        }
        super::deploy::acl::require_private_stage(&file, path)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (dir, name, path, fp);
    Ok(())
}

pub(super) fn read_tracked(
    path: &Path,
    handle: &ParentHandle,
    stat: &fs::Stat,
) -> std::result::Result<(Fingerprint, Vec<u8>), DriftStatus> {
    let fp = Fingerprint::from_stat(stat);
    if !owned_regular(stat, &fp) {
        return Err(DriftStatus::Unsafe);
    }
    if fp.mode & 0o400 == 0 {
        return Err(DriftStatus::Unverifiable);
    }
    if !Stamp::mode_safe(fp.mode & 0o7777) {
        return Err(DriftStatus::Unsafe);
    }
    check_file_access(&handle.directory, &handle.name, path, &fp)
        .map_err(|_| DriftStatus::Unsafe)?;
    let bytes = observation::read_checked_bounded(handle, path, &fp, OUTPUT_LIMIT as u64)
        .map_err(|_| DriftStatus::Unverifiable)?;
    if observation::inspect(path, false).ok().is_none_or(|after| {
        after.fingerprint.as_ref() != Some(&fp)
            || !observation::same_parent_identity_and_access(&handle.parents, &after.parents)
    }) {
        return Err(DriftStatus::Unverifiable);
    }
    if observation::sensitive_content(&bytes) {
        return Err(DriftStatus::Unsafe);
    }
    Ok((fp, bytes))
}
