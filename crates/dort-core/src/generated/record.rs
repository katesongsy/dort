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
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;
const LIMIT: u64 = 32 * 1024;

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    repository_id: String,
    mapping_id: String,
    destination: String,
}

pub(super) struct Slot {
    folder: PathBuf,
    name: String,
    identity: Identity,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stamp {
    sha256: String,
    mode: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    identity: Identity,
    deployed: Stamp,
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

    pub(super) fn load(&self) -> Result<Option<Stamp>> {
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
        if receipt.schema != SCHEMA
            || receipt.identity != self.identity
            || !receipt.deployed.valid()
        {
            return Err(invalid());
        }
        Ok(Some(receipt.deployed))
    }

    pub(super) fn save(self, deployed: Stamp) -> Result<()> {
        let receipt = Receipt {
            schema: SCHEMA,
            identity: self.identity,
            deployed,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
        let dir = PrivateDir::open(&self.folder, true)?.expect("created deployment records folder");
        dir.create(&self.name, &bytes)
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

    fn valid(&self) -> bool {
        self.mode <= 0o777
            && Mode(self.mode).permits_deployment()
            && self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    pub(super) fn observe(
        &self,
        path: &Path,
        handle: &ParentHandle,
        stat: &fs::Stat,
        proposed: &Self,
    ) -> DriftStatus {
        let fp = Fingerprint::from_stat(stat);
        if !owned_regular(stat, &fp) {
            return DriftStatus::Unsafe;
        }
        if fp.mode & 0o400 == 0 {
            return DriftStatus::Unverifiable;
        }
        let Ok(bytes) = observation::read_checked_bounded(handle, path, &fp, OUTPUT_LIMIT as u64)
        else {
            return DriftStatus::Unverifiable;
        };
        if observation::inspect(path, false).ok().is_none_or(|after| {
            after.fingerprint.as_ref() != Some(&fp)
                || !observation::same_parent_identity_and_access(&handle.parents, &after.parents)
        }) {
            return DriftStatus::Unverifiable;
        }
        if observation::sensitive_content(&bytes) {
            return DriftStatus::Unsafe;
        }
        match (digest(&bytes) != self.sha256, fp.mode & 0o7777 != self.mode) {
            (true, true) => DriftStatus::LocallyEditedAndPermissionsChanged,
            (true, false) => DriftStatus::LocallyEdited,
            (false, true) => DriftStatus::LocalPermissionsChanged,
            (false, false) if self == proposed => DriftStatus::UpToDate,
            (false, false) => DriftStatus::UpdateAvailable,
        }
    }
}
