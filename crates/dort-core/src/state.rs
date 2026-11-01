//! Descriptor-relative local state access. This is not a live-dotfile deployment engine.
use crate::{
    Error, Result,
    context::{HostContext, validate_absolute},
    error::io_error,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    fs::{File, TryLockError},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct PrivateDir {
    file: File,
    path: PathBuf,
}

impl PrivateDir {
    pub(crate) fn open(path: &Path, create: bool) -> Result<Option<Self>> {
        validate_absolute(path)?;
        let mut directory: File = fs::open(
            "/",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(|e| io_error("open filesystem root", "/", e))?;
        let mut walked = PathBuf::from("/");
        for component in path.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            walked.push(name);
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let next = match fs::openat(&directory, name, flags, Mode::empty()) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) if create => {
                    match fs::mkdirat(&directory, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                        Err(e) => return Err(io_error("create private directory", &walked, e)),
                    }
                    directory
                        .sync_all()
                        .map_err(|e| io_error("sync directory parent", &walked, e))?;
                    fs::openat(&directory, name, flags, Mode::empty())
                        .map_err(|e| io_error("open private directory", &walked, e))?
                }
                Err(rustix::io::Errno::NOENT) => return Ok(None),
                Err(e) => {
                    return Err(io_error(
                        "open state directory without following symlinks",
                        &walked,
                        e,
                    ));
                }
            };
            directory = File::from(next);
        }
        let metadata = directory
            .metadata()
            .map_err(|e| io_error("inspect private directory", path, e))?;
        if metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(Error::Invalid(format!(
                "private directory must be owned by this user with mode 0700: {}",
                path.display()
            )));
        }
        Ok(Some(Self {
            file: directory,
            path: path.to_owned(),
        }))
    }

    fn check_file(&self, file: &File, name: &str) -> Result<()> {
        let metadata = file
            .metadata()
            .map_err(|e| io_error("inspect state file", self.path.join(name), e))?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(Error::Invalid(format!(
                "state file must be a private, owned regular file without hard links: {}",
                self.path.join(name).display()
            )));
        }
        Ok(())
    }

    pub(crate) fn read(&self, name: &str) -> Result<Option<String>> {
        let fd = match fs::openat(
            &self.file,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(io_error("open state file", self.path.join(name), e)),
        };
        let mut file = File::from(fd);
        self.check_file(&file, name)?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| io_error("read state file", self.path.join(name), e))?;
        Ok(Some(text))
    }

    pub(crate) fn replace(&self, name: &str, content: &str) -> Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut created = None;
        for _ in 0..100 {
            let temporary = format!(
                ".{name}.{}.{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            match fs::openat(
                &self.file,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(fd) => {
                    created = Some((temporary, File::from(fd)));
                    break;
                }
                Err(rustix::io::Errno::EXIST) => continue,
                Err(e) => {
                    return Err(io_error(
                        "create state replacement",
                        self.path.join(name),
                        e,
                    ));
                }
            }
        }
        let (temporary, mut file) = created
            .ok_or_else(|| Error::Invalid("could not allocate state replacement file".into()))?;
        let result = (|| {
            file.write_all(content.as_bytes())
                .map_err(|e| io_error("write state replacement", self.path.join(name), e))?;
            file.sync_all()
                .map_err(|e| io_error("sync state replacement", self.path.join(name), e))?;
            fs::renameat(&self.file, temporary.as_str(), &self.file, name)
                .map_err(|e| io_error("replace state file", self.path.join(name), e))?;
            self.file
                .sync_all()
                .map_err(|e| io_error("sync state directory", &self.path, e))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.file, temporary.as_str(), fs::AtFlags::empty());
        }
        result
    }
}

/// Advisory process lock. The lock inode is never removed; OS closure releases it.
pub struct WriterLock {
    _file: File,
    config_dir: PathBuf,
    state_dir: PathBuf,
}

impl WriterLock {
    pub fn acquire(host: &HostContext) -> Result<Self> {
        let directory = PrivateDir::open(&host.state_dir, true)?.expect("created state directory");
        let file = fs::openat(
            &directory.file,
            "lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        )
        .map(File::from)
        .map_err(|e| io_error("open application lock", host.state_dir.join("lock"), e))?;
        directory.check_file(&file, "lock")?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                _file: file,
                config_dir: host.config_dir.clone(),
                state_dir: host.state_dir.clone(),
            }),
            Err(TryLockError::WouldBlock) => Err(Error::Locked),
            Err(TryLockError::Error(e)) => Err(io_error(
                "acquire application lock",
                host.state_dir.join("lock"),
                e,
            )),
        }
    }

    pub(crate) fn check_context(&self, host: &HostContext) -> Result<()> {
        if self.config_dir != host.config_dir || self.state_dir != host.state_dir {
            return Err(Error::Invalid(
                "writer lock belongs to a different host context".into(),
            ));
        }
        Ok(())
    }
}
