use crate::{
    Error, Manifest, Result,
    adoption::transaction_id,
    context::HostContext,
    error::io_error,
    journal,
    local::{LocalConfig, repository_state_path},
    model::FileMode,
    observation::{self, Entry, Fingerprint, Parent, ParentHandle},
    state::{PrivateDir, WriterLock},
    template::{SOURCE_LIMIT, Template},
};
use rustix::{fs, io::Errno, process};
use std::{
    collections::{BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    fs::File,
    io::Write,
    os::unix::ffi::OsStringExt,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

const DIRECTORY: &str = "templates";
const BEFORE: &str = "reviewed-before";
const DISPLACED: &str = "previous-entry";
const INVENTORY_LIMIT: usize = 4096;
const DIRECTORY_FLAGS: fs::OFlags = fs::OFlags::RDONLY
    .union(fs::OFlags::DIRECTORY)
    .union(fs::OFlags::NOFOLLOW)
    .union(fs::OFlags::CLOEXEC);

#[derive(Clone)]
pub struct Source {
    row: Row,
    text: String,
    fingerprint: Fingerprint,
    parents: Vec<Parent>,
}

impl Source {
    pub fn row(&self) -> &Row {
        &self.row
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn permissions(&self) -> u32 {
        self.fingerprint.mode & 0o777
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub mappings: usize,
    /// Path permits edits. Open it with `show` to validate its contents.
    pub editable: bool,
}

struct Repository<'a> {
    root: &'a Path,
    manifest: Entry,
    mappings: Manifest,
    protected: Vec<PathBuf>,
}

impl<'a> Repository<'a> {
    fn open(host: &HostContext, root: &'a Path) -> Result<Self> {
        if root.parent().is_none()
            || may_overlap(root, &host.config_dir)
            || may_overlap(root, &host.state_dir)
            || root == host.home
        {
            return Err(invalid(
                "Use a repository separate from Home and Dort state.",
            ));
        }
        let path = root.join("dort.toml");
        let handle = trusted_parent(root, &path)?;
        entry(&handle, &path)?.ok_or_else(|| invalid("Repository manifest is missing."))?;
        let (mappings, manifest) = observation::read_manifest(&path)?;
        if !observation::same_parent_identity_and_access(&handle.parents, &manifest.parents)
            || entry(&handle, &path)? != manifest.fingerprint
        {
            return Err(stale());
        }
        let mut protected = vec![path, root.join("recipients.toml")];
        for file in &mappings.files {
            protected.push(
                file.target
                    .strip_prefix("~")
                    .map_or_else(|_| file.target.clone(), |path| host.home.join(path)),
            );
            if file.mode != FileMode::Template {
                protected.extend(file.source_paths().map(|path| root.join(path)));
            }
        }
        Ok(Self {
            root,
            manifest: manifest.stable_parents(),
            mappings,
            protected,
        })
    }

    fn references(&self, path: &Path) -> usize {
        self.mappings
            .files
            .iter()
            .filter(|file| {
                file.mode == FileMode::Template && file.source_paths().any(|source| source == path)
            })
            .count()
    }

    fn checked_path(&self, path: &Path) -> Result<PathBuf> {
        let valid = path.to_str().is_some_and(|text| {
            !text.is_empty()
                && safe_text(text)
                && !text.chars().any(char::is_control)
                && text.split('/').all(|part| {
                    !matches!(part, "" | "." | "..") && !part.eq_ignore_ascii_case(".git")
                })
        }) && !path.starts_with("~")
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
        let absolute = self.root.join(path);
        if !valid
            || observation::sensitive_path(&absolute)
            || (absolute.extension() == Some(OsStr::new("jinja"))
                && observation::sensitive_path(&absolute.with_extension("")))
        {
            return Err(invalid("Use a safe repository-relative template path."));
        }
        if self
            .protected
            .iter()
            .any(|other| may_overlap(other, &absolute))
        {
            return Err(invalid(
                "Choose a path outside reserved files and other file mappings.",
            ));
        }
        Ok(absolute)
    }

    fn inspect(&self, path: &Path) -> Result<(Row, ParentHandle, Fingerprint)> {
        let absolute = self.checked_path(path)?;
        let mappings = self.references(path);
        let editable = writable_name(path);
        if !editable && mappings == 0 {
            return Err(invalid(
                "Choose a declared template or a .jinja file under templates/.",
            ));
        }
        let handle = trusted_parent(self.root, &absolute)?;
        let fingerprint =
            entry(&handle, &absolute)?.ok_or_else(|| invalid("Template file is missing."))?;
        if !(0..=SOURCE_LIMIT as i64).contains(&fingerprint.size) {
            return Err(invalid("Template exceeds 64 KiB."));
        }
        Ok((
            Row {
                path: path.to_owned(),
                mappings,
                editable,
            },
            handle,
            fingerprint,
        ))
    }

    fn read(&self, path: &Path) -> Result<Source> {
        let (row, handle, fingerprint) = self.inspect(path)?;
        let bytes = observation::read_checked_bounded(
            &handle,
            &self.root.join(path),
            &fingerprint,
            SOURCE_LIMIT as u64,
        )?;
        if observation::sensitive_content(&bytes) {
            return Err(invalid("Template may contain sensitive content."));
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid("Template must be UTF-8."))?;
        if !safe_text(&text) {
            return Err(invalid("Template must be plain text."));
        }
        Ok(Source {
            row,
            text,
            fingerprint,
            parents: handle.parents,
        })
    }
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn stale() -> Error {
    invalid("Repository, mappings or template changed. Open the template and review again.")
}

fn registered<'a>(config: &'a LocalConfig, root: &Path) -> Result<&'a str> {
    config
        .repository_id(root)
        .ok_or_else(|| invalid("Register the repository before opening templates."))
}

fn writable_name(path: &Path) -> bool {
    path.starts_with(DIRECTORY) && path.extension() == Some(OsStr::new("jinja"))
}

// Fail closed on aliases on case- or normalization-insensitive volumes.
fn may_overlap(a: &Path, b: &Path) -> bool {
    a.components().zip(b.components()).all(|(a, b)| {
        let a = a.as_os_str().as_encoded_bytes();
        let b = b.as_os_str().as_encoded_bytes();
        !a.is_ascii() || !b.is_ascii() || a.eq_ignore_ascii_case(b)
    })
}

fn safe_text(text: &str) -> bool {
    text.chars().all(|c| {
        (!c.is_control() || matches!(c, '\n' | '\r' | '\t'))
            && !matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    })
}

fn trusted_parent(root: &Path, path: &Path) -> Result<ParentHandle> {
    observation::parent_handle_with(path, |dir, path| {
        let stat = fs::fstat(dir).map_err(|e| io_error("inspect folder", path, e))?;
        let uid = process::geteuid().as_raw();
        let trusted = if path.starts_with(root) {
            stat.st_uid == uid && stat.st_mode & 0o7022 == 0
        } else {
            (stat.st_uid == uid || stat.st_uid == 0)
                && (stat.st_mode & 0o022 == 0 || stat.st_mode & 0o1000 != 0)
        };
        if !trusted {
            return Err(invalid(
                "Use an owned repository and Dort state with trusted folders.",
            ));
        }
        Ok(())
    })
}

fn entry(handle: &ParentHandle, path: &Path) -> Result<Option<Fingerprint>> {
    if handle.missing {
        return Ok(None);
    }
    match fs::statat(
        &handle.directory,
        &handle.name,
        fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Ok(stat)
            if fs::FileType::from_raw_mode(stat.st_mode) == fs::FileType::RegularFile
                && stat.st_uid == process::geteuid().as_raw()
                && stat.st_nlink == 1
                && stat.st_mode & 0o7022 == 0 =>
        {
            Ok(Some(Fingerprint::from_stat(&stat)))
        }
        Ok(_) => Err(invalid(
            "Use an owned regular file without links or unsafe permissions.",
        )),
        Err(Errno::NOENT) => Ok(None),
        Err(e) => Err(io_error("inspect file", path, e)),
    }
}

pub fn list(host: &HostContext, root: &Path) -> Result<Vec<Row>> {
    registered(&LocalConfig::load(host)?, root)?;
    let repo = Repository::open(host, root)?;
    let mut candidates = BTreeSet::new();
    for file in repo
        .mappings
        .files
        .iter()
        .filter(|file| file.mode == FileMode::Template)
    {
        for path in file.source_paths() {
            candidates.insert(path.clone());
            if candidates.len() > INVENTORY_LIMIT {
                return Err(invalid("Too many template paths to list."));
            }
        }
    }
    let mut folders = VecDeque::from([PathBuf::from(DIRECTORY)]);
    let mut budget = INVENTORY_LIMIT;
    while let Some(folder) = folders.pop_front() {
        let absolute = root.join(&folder);
        let handle = match trusted_parent(root, &absolute.join(".template-probe")) {
            Ok(handle) if handle.missing => continue,
            Ok(handle) => handle,
            Err(e) if folder == Path::new(DIRECTORY) => return Err(e),
            Err(_) => continue,
        };
        let dir = fs::Dir::read_from(&handle.directory)
            .map_err(|e| io_error("list template folder", &absolute, e))?;
        for item in dir {
            let item = item.map_err(|e| io_error("list template folder", &absolute, e))?;
            let name = item.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            budget = budget
                .checked_sub(1)
                .ok_or_else(|| invalid("Too many entries under templates/ to list."))?;
            let name = OsString::from_vec(name.to_vec());
            let candidate = folder.join(&name);
            if repo.checked_path(&candidate).is_err() {
                continue;
            }
            let Ok(stat) = fs::statat(&handle.directory, &name, fs::AtFlags::SYMLINK_NOFOLLOW)
            else {
                continue;
            };
            match fs::FileType::from_raw_mode(stat.st_mode) {
                fs::FileType::Directory => folders.push_back(candidate),
                fs::FileType::RegularFile if writable_name(&candidate) => {
                    candidates.insert(candidate);
                }
                _ => (),
            }
            if candidates.len() > INVENTORY_LIMIT {
                return Err(invalid("Too many template paths to list."));
            }
        }
    }
    Ok(candidates
        .into_iter()
        .filter_map(|path| repo.inspect(&path).ok().map(|(row, _, _)| row))
        .collect())
}

pub fn show(host: &HostContext, root: &Path, path: &Path) -> Result<Source> {
    registered(&LocalConfig::load(host)?, root)?;
    Repository::open(host, root)?.read(path)
}

pub enum Change {
    Create(PathBuf),
    Edit(Source),
}

pub struct Preview {
    root: PathBuf,
    home: PathBuf,
    state_dir: PathBuf,
    config: Entry,
    manifest: Entry,
    backup: PathBuf,
    change: Change,
    text: String,
    parents: Vec<Parent>,
    mappings: usize,
}

pub struct Review<'a> {
    pub path: &'a Path,
    pub before: Option<&'a str>,
    pub after: &'a str,
    pub permissions: u32,
    pub mappings: usize,
    pub retained: Option<Retained>,
}

#[derive(Debug)]
pub struct Retained {
    pub reviewed: PathBuf,
    pub displaced: PathBuf,
}

pub struct Approval(Preview);

#[derive(Debug)]
pub enum Outcome {
    Created,
    Edited(Retained),
    NeedsAttention {
        retained: Option<Retained>,
        competing_change: bool,
        durability_unknown: bool,
    },
}

impl Preview {
    pub fn prepare(host: &HostContext, root: &Path, change: Change, text: String) -> Result<Self> {
        let (config, binding) = LocalConfig::load_bound(host)?;
        let id = registered(&config, root)?;
        let repo = Repository::open(host, root)?;
        let path = match &change {
            Change::Create(path) => path,
            Change::Edit(before) => &before.row.path,
        };
        let absolute = repo.checked_path(path)?;
        if !writable_name(path) {
            return Err(invalid(
                "Only .jinja files under templates/ can be changed.",
            ));
        }
        let handle = trusted_parent(root, &absolute)?;
        if handle.missing {
            return Err(invalid(
                "Template folder is missing. Create templates/ and any needed subfolder in the repository, then review again.",
            ));
        }
        check_entry(&repo, &change, &handle, &absolute)?;
        if !safe_text(&text) {
            return Err(invalid("Template must be plain text."));
        }
        Template::compile(path.to_str().expect("checked path"), &text)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let backup = repository_state_path(host, id)?
            .join("template-edits")
            .join(transaction_id());
        let mappings = repo.references(path);
        Ok(Self {
            root: root.to_owned(),
            home: host.home.clone(),
            state_dir: host.state_dir.clone(),
            config: binding.stable_parents(),
            manifest: repo.manifest,
            backup,
            change,
            text,
            parents: handle.parents,
            mappings,
        })
    }

    pub fn review(&self) -> Review<'_> {
        let (path, before, permissions, retained) = match &self.change {
            Change::Create(path) => (path.as_path(), None, 0o644, None),
            Change::Edit(source) => (
                source.row.path.as_path(),
                Some(source.text()),
                source.permissions(),
                Some(self.retained()),
            ),
        };
        Review {
            path,
            before,
            after: &self.text,
            permissions,
            mappings: self.mappings,
            retained,
        }
    }

    fn retained(&self) -> Retained {
        Retained {
            reviewed: self.backup.join(BEFORE),
            displaced: self.backup.join(DISPLACED),
        }
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }

    fn recheck<'a>(
        &'a self,
        host: &HostContext,
        path: &Path,
        absolute: &Path,
    ) -> Result<(Repository<'a>, ParentHandle)> {
        let (config, binding) = LocalConfig::load_bound(host)?;
        registered(&config, &self.root)?;
        let repo = Repository::open(host, &self.root)?;
        if binding.stable_parents() != self.config || repo.manifest != self.manifest {
            return Err(stale());
        }
        repo.checked_path(path)?;
        let handle = trusted_parent(&self.root, absolute)?;
        if handle.missing
            || !observation::same_parent_identity_and_access(&handle.parents, &self.parents)
        {
            return Err(stale());
        }
        Ok((repo, handle))
    }
}

fn check_entry(
    repo: &Repository<'_>,
    change: &Change,
    handle: &ParentHandle,
    absolute: &Path,
) -> Result<()> {
    match change {
        Change::Create(_) if entry(handle, absolute)?.is_none() => Ok(()),
        Change::Create(_) => Err(invalid(
            "A file already exists at this path. Choose a different path.",
        )),
        Change::Edit(before) => {
            let current = repo.read(&before.row.path)?;
            if before.fingerprint != current.fingerprint
                || before.text != current.text
                || !observation::same_parent_identity_and_access(&before.parents, &current.parents)
                || !observation::same_parent_identity_and_access(&before.parents, &handle.parents)
            {
                return Err(stale());
            }
            Ok(())
        }
    }
}

impl Approval {
    pub fn execute(self, host: &HostContext) -> Result<Outcome> {
        self.execute_before_publish(host, || {})
    }

    fn execute_before_publish(
        self,
        host: &HostContext,
        before_publish: impl FnOnce(),
    ) -> Result<Outcome> {
        let p = self.0;
        if host.home != p.home
            || host.state_dir != p.state_dir
            || host.local_config_path() != p.config.path
        {
            return Err(stale());
        }
        let _lock = WriterLock::acquire(host)?;
        journal::ensure_writable(host)?;
        let review = p.review();
        let absolute = p.root.join(review.path);
        let check = || {
            let (repo, handle) = p.recheck(host, review.path, &absolute)?;
            check_entry(&repo, &p.change, &handle, &absolute)?;
            Ok::<_, Error>(handle)
        };
        let handle = check()?;
        let state = stage_dir(host, &p.backup, &handle)?;
        let old = match &p.change {
            Change::Edit(old) => Some(old),
            _ => None,
        };
        if let Some(old) = old {
            state.create(BEFORE, old.text.as_bytes())?;
        }
        let mut file = File::from(
            fs::openat(
                &state.file,
                DISPLACED,
                fs::OFlags::WRONLY
                    | fs::OFlags::CREATE
                    | fs::OFlags::EXCL
                    | fs::OFlags::NOFOLLOW
                    | fs::OFlags::CLOEXEC,
                fs::Mode::RUSR | fs::Mode::WUSR,
            )
            .map_err(|e| io_error("stage template", &absolute, e))?,
        );
        file.write_all(p.text.as_bytes())
            .map_err(|e| io_error("write staged template", &absolute, e))?;
        if let Some(old) = old {
            fs::fchown(
                &file,
                None,
                Some(process::Gid::from_raw(old.fingerprint.gid)),
            )
            .map_err(|e| io_error("preserve template group", &absolute, e))?;
        }
        fs::fchmod(&file, fs::Mode::from_bits_truncate(review.permissions as _))
            .map_err(|e| io_error("set staged template permissions", &absolute, e))?;
        file.sync_all()
            .map_err(|e| io_error("sync staged template", &absolute, e))?;
        let staged =
            fs::fstat(&file).map_err(|e| io_error("inspect staged template", &absolute, e))?;
        let staged = Fingerprint::from_stat(&staged);
        state.sync()?;
        if !stage_matches(host, &state)
            || !matches_file(
                &state.file,
                OsStr::new(DISPLACED),
                &state.path.join(DISPLACED),
                &staged,
                &p.text,
                false,
            )
        {
            return Err(stale());
        }
        let handle = check()?;
        before_publish();
        let flag = if old.is_some() {
            fs::RenameFlags::EXCHANGE
        } else {
            fs::RenameFlags::NOREPLACE
        };
        match fs::renameat_with(
            &state.file,
            DISPLACED,
            &handle.directory,
            &handle.name,
            flag,
        ) {
            Ok(()) => (),
            Err(Errno::EXIST) if old.is_none() => {
                return Err(invalid(
                    "A file appeared at this path. Choose a different path.",
                ));
            }
            Err(e) => return Err(io_error("publish reviewed template", &absolute, e)),
        }
        let destination_synced = handle.directory.sync_all().is_ok();
        let stage_synced = state.sync().is_ok();
        let durability_unknown = !destination_synced || !stage_synced;
        let competing_change = !stage_matches(host, &state)
            || old.is_some_and(|old| {
                !matches_file(
                    &state.file,
                    OsStr::new(DISPLACED),
                    &state.path.join(DISPLACED),
                    &old.fingerprint,
                    &old.text,
                    true,
                )
            })
            || !p
                .recheck(host, review.path, &absolute)
                .is_ok_and(|(_, live)| {
                    matches_file(
                        &live.directory,
                        &live.name,
                        &absolute,
                        &staged,
                        &p.text,
                        true,
                    )
                });
        let retained = old.map(|_| p.retained());
        if competing_change || durability_unknown {
            Ok(Outcome::NeedsAttention {
                retained,
                competing_change,
                durability_unknown,
            })
        } else {
            Ok(retained.map_or(Outcome::Created, Outcome::Edited))
        }
    }
}

fn stage_dir(host: &HostContext, path: &Path, handle: &ParentHandle) -> Result<PrivateDir> {
    trusted_parent(&host.state_dir, path)?;
    let base = PrivateDir::open(path.parent().expect("stage parent"), true)?
        .expect("created stage parent");
    let parent = fs::fstat(&handle.directory).map_err(|e| {
        io_error(
            "inspect template folder",
            &handle.parents.last().expect("template parent").path,
            e,
        )
    })?;
    let stage =
        fs::fstat(&base.file).map_err(|e| io_error("inspect staging folder", &base.path, e))?;
    if parent.st_dev != stage.st_dev {
        return Err(invalid(
            "Templates and Dort state must be on the same filesystem.",
        ));
    }
    let name = path.file_name().expect("stage name");
    fs::mkdirat(
        &base.file,
        name,
        fs::Mode::RUSR | fs::Mode::WUSR | fs::Mode::XUSR,
    )
    .map_err(|e| io_error("create private template staging folder", path, e))?;
    let file = fs::openat(&base.file, name, DIRECTORY_FLAGS, fs::Mode::empty())
        .map(File::from)
        .map_err(|e| io_error("open private template staging folder", path, e))?;
    base.sync()?;
    let dir = PrivateDir {
        file,
        path: path.to_owned(),
    };
    if !stage_matches(host, &dir) {
        return Err(stale());
    }
    Ok(dir)
}

fn stage_matches(host: &HostContext, dir: &PrivateDir) -> bool {
    let Ok(handle) = trusted_parent(&host.state_dir, &dir.path.join(DISPLACED)) else {
        return false;
    };
    if handle.missing {
        return false;
    }
    let (Ok(open), Ok(named)) = (fs::fstat(&dir.file), fs::fstat(&handle.directory)) else {
        return false;
    };
    open.st_dev == named.st_dev
        && open.st_ino == named.st_ino
        && named.st_uid == process::geteuid().as_raw()
        && named.st_mode & 0o7777 == 0o700
}

fn matches_file(
    directory: &File,
    name: &OsStr,
    path: &Path,
    expected: &Fingerprint,
    text: &str,
    renamed: bool,
) -> bool {
    let Ok(stat) = fs::statat(directory, name, fs::AtFlags::SYMLINK_NOFOLLOW) else {
        return false;
    };
    let actual = Fingerprint::from_stat(&stat);
    let mut expected = expected.clone();
    if renamed {
        expected.changed = actual.changed;
    }
    actual == expected
        && observation::read_at_checked(directory, name, path, &actual, SOURCE_LIMIT as u64)
            .is_ok_and(|bytes| bytes == text.as_bytes())
}
