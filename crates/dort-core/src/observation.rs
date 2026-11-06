//! Read-only, descriptor-relative filesystem snapshots. No symlinked parent is followed.
use crate::{Error, Result, context::validate_absolute, error::io_error};
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::File,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

const MAX_CONTENT: u64 = 16 * 1024 * 1024;
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    pub device: u64,
    pub inode: u64,
    pub links: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: i64,
    pub modified: (i64, i64),
    pub changed: (i64, i64),
}

impl Fingerprint {
    #[allow(clippy::unnecessary_cast)] // stat field widths differ across Unix platforms.
    pub(crate) fn from_stat(s: &fs::Stat) -> Self {
        Self {
            device: s.st_dev as u64,
            inode: s.st_ino as u64,
            links: s.st_nlink as u64,
            mode: s.st_mode as u32,
            uid: s.st_uid,
            gid: s.st_gid,
            size: s.st_size as i64,
            modified: (s.st_mtime as i64, s.st_mtime_nsec as i64),
            changed: (s.st_ctime as i64, s.st_ctime_nsec as i64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Missing,
    Regular,
    Directory,
    Symlink,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parent {
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
    pub fingerprint: Option<Fingerprint>,
    /// Includes every existing parent, even when a later component is absent.
    pub parents: Vec<Parent>,
    pub link_target: Option<PathBuf>,
    pub sha256: Option<String>,
    pub sensitive: bool,
}

pub(crate) struct ParentHandle {
    pub(crate) directory: File,
    pub(crate) name: OsString,
    pub(crate) parents: Vec<Parent>,
    pub(crate) missing: bool,
}

pub(crate) fn parent_handle(path: &Path) -> Result<ParentHandle> {
    validate_output_path(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::Invalid("cannot inspect root as a file".into()))?
        .to_owned();
    let mut directory = File::from(
        fs::open("/", DIRECTORY_FLAGS, Mode::empty())
            .map_err(|e| io_error("open filesystem root", path, e))?,
    );
    let mut walked = PathBuf::from("/");
    let mut parents = vec![];
    for component in path.parent().unwrap().components() {
        let Component::Normal(name) = component else {
            continue;
        };
        parents.push(parent_record(&directory, &walked)?);
        walked.push(name);
        match fs::openat(&directory, name, DIRECTORY_FLAGS, Mode::empty()) {
            Ok(fd) => directory = File::from(fd),
            Err(rustix::io::Errno::NOENT) => {
                return Ok(ParentHandle {
                    directory,
                    name: path.file_name().unwrap().to_owned(),
                    parents,
                    missing: true,
                });
            }
            Err(e) => {
                return Err(io_error(
                    "inspect parent without following symlinks (symlink or non-directory parents are forbidden)",
                    &walked,
                    e,
                ));
            }
        }
    }
    parents.push(parent_record(&directory, &walked)?);
    Ok(ParentHandle {
        directory,
        name,
        parents,
        missing: false,
    })
}

fn parent_record(file: &File, path: &Path) -> Result<Parent> {
    Ok(Parent {
        path: path.to_owned(),
        fingerprint: Fingerprint::from_stat(
            &fs::fstat(file).map_err(|e| io_error("inspect directory", path, e))?,
        ),
    })
}

/// Output formats intentionally reject non-UTF-8 paths rather than corrupting their identity.
pub fn validate_output_path(path: &Path) -> Result<()> {
    validate_absolute(path)?;
    if path.to_str().is_none() {
        return Err(Error::Invalid(
            "non-UTF-8 paths cannot be represented in Dort reports".into(),
        ));
    }
    Ok(())
}

/// Hash only owned, single-link regular files. Never read directory/special/link contents.
pub fn inspect(path: &Path, hash: bool) -> Result<Entry> {
    let handle = parent_handle(path)?;
    let mut entry = Entry {
        path: path.to_owned(),
        kind: Kind::Missing,
        fingerprint: None,
        parents: handle.parents.clone(),
        link_target: None,
        sha256: None,
        sensitive: sensitive_path(path),
    };
    if handle.missing {
        return Ok(entry);
    }
    let stat = match fs::statat(&handle.directory, &handle.name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(rustix::io::Errno::NOENT) => return Ok(entry),
        Err(e) => return Err(io_error("inspect file", path, e)),
    };
    entry.kind = match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => Kind::Regular,
        FileType::Directory => Kind::Directory,
        FileType::Symlink => Kind::Symlink,
        _ => Kind::Special,
    };
    let fingerprint = Fingerprint::from_stat(&stat);
    if entry.kind == Kind::Symlink {
        let target = fs::readlinkat(&handle.directory, &handle.name, Vec::new())
            .map_err(|e| io_error("read link", path, e))?;
        let target = PathBuf::from(OsString::from_vec(target.into_bytes()));
        if target.to_str().is_none() {
            return Err(Error::Invalid(
                "non-UTF-8 link targets cannot be represented in Dort reports".into(),
            ));
        }
        entry.link_target = Some(target);
        let after = fs::statat(&handle.directory, &handle.name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| io_error("recheck link", path, e))?;
        if fingerprint != Fingerprint::from_stat(&after) {
            return Err(Error::Invalid("link changed during observation".into()));
        }
    }
    if hash
        && entry.kind == Kind::Regular
        && fingerprint.links == 1
        && fingerprint.uid == rustix::process::geteuid().as_raw()
        && !entry.sensitive
    {
        let bytes = read_checked(&handle, path, &fingerprint)?;
        entry.sensitive = sensitive_content(&bytes);
        if !entry.sensitive {
            entry.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
        }
    }
    entry.fingerprint = Some(fingerprint);
    Ok(entry)
}

use std::os::unix::ffi::OsStringExt;

pub(crate) fn read_checked(
    handle: &ParentHandle,
    path: &Path,
    expected: &Fingerprint,
) -> Result<Vec<u8>> {
    read_checked_bounded(handle, path, expected, MAX_CONTENT)
}

pub(crate) fn read_checked_bounded(
    handle: &ParentHandle,
    path: &Path,
    expected: &Fingerprint,
    limit: u64,
) -> Result<Vec<u8>> {
    if expected.size < 0 || expected.size as u64 > limit {
        return Err(Error::Invalid(
            "file exceeds the 16 MiB observation limit".into(),
        ));
    }
    let mut file = File::from(
        fs::openat(
            &handle.directory,
            &handle.name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| io_error("open regular file without following links", path, e))?,
    );
    let before = fs::fstat(&file).map_err(|e| io_error("inspect open file", path, e))?;
    if &Fingerprint::from_stat(&before) != expected {
        return Err(Error::Invalid("file changed before observation".into()));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read regular file", path, e))?;
    let after = fs::fstat(&file).map_err(|e| io_error("recheck open file", path, e))?;
    let named = fs::statat(&handle.directory, &handle.name, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|e| io_error("recheck named file", path, e))?;
    if bytes.len() as u64 > limit
        || Fingerprint::from_stat(&after) != *expected
        || Fingerprint::from_stat(&named) != *expected
    {
        return Err(Error::Invalid("file changed during observation".into()));
    }
    Ok(bytes)
}

/// Read a manifest through the same no-follow boundary. Contents are never in snapshots.
pub fn read_manifest(path: &Path) -> Result<(crate::Manifest, Entry)> {
    let entry = inspect(path, false)?;
    let fp = entry
        .fingerprint
        .as_ref()
        .filter(|f| {
            entry.kind == Kind::Regular
                && f.links == 1
                && f.uid == rustix::process::geteuid().as_raw()
        })
        .ok_or_else(|| {
            Error::Invalid("manifest must be an owned regular file without hard links".into())
        })?;
    let handle = parent_handle(path)?;
    let bytes = read_checked(&handle, path, fp)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("manifest must be UTF-8".into()))?;
    let manifest = crate::Manifest::parse(text)?;
    let mut entry = entry;
    entry.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
    Ok((manifest, entry))
}

/// Normalize a link lexically; this does not grant permission to follow it.
pub fn link_path(entry: &Entry) -> Option<PathBuf> {
    let raw = entry.link_target.as_ref()?;
    let path = if raw.is_absolute() {
        raw.clone()
    } else {
        entry.path.parent()?.join(raw)
    };
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir => {
                normalized.pop();
            }
            _ => {}
        }
    }
    Some(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Regular,
    Directory,
    Special,
    Broken,
    Loop,
    UnsafeParent,
}

/// Resolve terminal symlink chains only. Symlinked parents remain forbidden, not followed.
pub fn link_state(entry: &Entry) -> Result<LinkState> {
    let mut current = entry.clone();
    let mut seen = BTreeSet::new();
    for _ in 0..40 {
        if !seen.insert(current.path.clone()) {
            return Ok(LinkState::Loop);
        }
        let Some(path) = link_path(&current) else {
            return Ok(match current.kind {
                Kind::Missing => LinkState::Broken,
                Kind::Regular => LinkState::Regular,
                Kind::Directory => LinkState::Directory,
                _ => LinkState::Special,
            });
        };
        current = match inspect(&path, false) {
            Ok(entry) => entry,
            Err(_) => return Ok(LinkState::UnsafeParent),
        };
    }
    Ok(LinkState::Loop)
}

/// Conservative filename heuristic, not a proof that other files contain no secrets.
pub fn sensitive_path(path: &Path) -> bool {
    path.components().any(|part| {
        let name = part.as_os_str().as_bytes().to_ascii_lowercase();
        let name = String::from_utf8_lossy(&name);
        [
            ".ssh",
            ".gnupg",
            ".aws",
            ".azure",
            ".kube",
            ".netrc",
            ".npmrc",
            ".pypirc",
            ".git-credentials",
            ".env",
            "keychains",
            "keyring",
            "keyrings",
            "gh",
            "gcloud",
            "vault",
            "rclone",
            "age",
            "sops",
            "credentials",
            "credential",
            "secrets",
            "secret",
            "privatekeys",
            "passwords",
            "auth",
            "auth.json",
            ".auth.json",
            ".auth",
            ".sessions",
            "authentication",
            "session",
            "sessions",
            "session-store",
            "sessionstore",
            "cookies",
            "cookies.sqlite",
            "login data",
            "1password",
            "bitwarden",
            "keepass",
            "keepassxc",
            "lastpass",
            ".password-store",
        ]
        .contains(&name.as_ref())
            || [
                "auth",
                "authentication",
                "credential",
                "credentials",
                "secret",
                "secrets",
                "session",
                "sessions",
                "cookies",
            ]
            .contains(&name.trim_start_matches('.').split('.').next().unwrap_or(""))
            || name.starts_with(".env.")
            || name.starts_with("id_rsa")
            || name.starts_with("id_ed25519")
            || name.starts_with("id_ecdsa")
            || name.starts_with("id_dsa")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.contains("token")
    })
}

pub(crate) fn sensitive_content(bytes: &[u8]) -> bool {
    let lower = bytes.to_ascii_lowercase();
    [
        b"private key".as_slice(),
        b"\"password\"",
        b"\"token\"",
        b"\"secret\"",
        b"\"credentials\"",
        b"accesstoken",
        b"apikey",
        b"clientsecret",
        b"authorization:",
        b"bearer ",
        b"password=",
        b"password =",
        b"token=",
        b"token =",
        b"secret=",
        b"secret =",
        b"aws_access_key_id",
        b"oauth_token",
        b"access_token",
        b"api_key",
        b"api-key",
        b"client_secret",
        b"private_key",
        b"age-secret-key-",
        b"secret_access_key",
    ]
    .iter()
    .any(|needle| lower.windows(needle.len()).any(|part| part == *needle))
}

/// Enumerate names through a retained no-follow directory descriptor, with a hard limit.
pub(crate) fn names(path: &Path, limit: usize) -> Result<Vec<OsString>> {
    let handle = parent_handle(path)?;
    if handle.missing {
        return Ok(vec![]);
    }
    let directory = File::from(
        fs::openat(
            &handle.directory,
            &handle.name,
            DIRECTORY_FLAGS,
            Mode::empty(),
        )
        .map_err(|e| io_error("open discovery directory without following links", path, e))?,
    );
    let mut names = vec![];
    let dir = fs::Dir::read_from(&directory)
        .map_err(|e| io_error("read discovery directory", path, e))?;
    for item in dir {
        let item = item.map_err(|e| io_error("read discovery entry", path, e))?;
        let name = item.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if names.len() >= limit {
            return Err(Error::Invalid("discovery entry limit exceeded".into()));
        }
        names.push(OsString::from_vec(name.to_vec()));
    }
    names.sort();
    Ok(names)
}
