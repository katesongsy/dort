//! Local discovery preferences, separate from repository configuration and journals.
//! These paths only suppress discovery; they never authorize access to live files.
use crate::{
    Error, Result,
    context::HostContext,
    error::io_error,
    state::{PrivateDir, WriterLock},
};
use rustix::fs::{self, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const NAME: &str = "discovery-ignore.toml";
const MAX_BYTES: usize = 262144;
const MAX_PATHS: usize = 4096;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    paths: BTreeSet<PathBuf>,
}
impl Rules {
    pub fn load(host: &HostContext) -> Result<Self> {
        Ok(read(host)?.0)
    }
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.paths.iter()
    }
    pub fn excludes(&self, relative: &Path) -> bool {
        relative.ancestors().any(|path| self.paths.contains(path))
    }
    fn validate(&self) -> Result<()> {
        if self.paths.len() > MAX_PATHS {
            return Err(invalid());
        }
        for path in &self.paths {
            validate_path(path)?;
        }
        Ok(())
    }
}
fn invalid() -> Error {
    Error::Invalid("invalid discovery ignore preferences (values redacted)".into())
}
fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_str().ok_or_else(invalid)?;
    if text.is_empty()
        || text.len() > 4096
        || text.chars().any(|c| c.is_control() || "*?[]".contains(c))
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path.components().collect::<PathBuf>().as_os_str() != path.as_os_str()
    {
        return Err(invalid());
    }
    Ok(())
}
#[derive(PartialEq, Eq)]
struct Binding {
    text: String,
    identity: (u64, u64, i64, i64, i64, i64),
}
fn identity(file: &File) -> Result<(u64, u64, i64, i64, i64, i64)> {
    let m = file.metadata().map_err(|_| invalid())?;
    Ok((
        m.dev(),
        m.ino(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ))
}
fn read(host: &HostContext) -> Result<(Rules, Option<Binding>)> {
    let Some(dir) = PrivateDir::open(&host.config_dir, false)? else {
        return Ok((Rules::default(), None));
    };
    let fd = match fs::openat(
        &dir.file,
        NAME,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok((Rules::default(), None)),
        Err(e) => {
            return Err(io_error(
                "open discovery ignore preferences",
                dir.path.join(NAME),
                e,
            ));
        }
    };
    let mut file = File::from(fd);
    dir.check_file(&file, NAME)?;
    let before = identity(&file)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > MAX_BYTES || before != identity(&file)? {
        return Err(invalid());
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let rules: Rules = toml::from_str(&text).map_err(|_| invalid())?;
    rules.validate()?;
    Ok((
        rules,
        Some(Binding {
            text,
            identity: before,
        }),
    ))
}

/// Opaque, non-serializable preview. Approval consumes this exact in-memory value.
pub struct Preview {
    host: HostContext,
    before: Option<Binding>,
    rules: Rules,
    path: PathBuf,
    ignored: bool,
}
impl Preview {
    pub fn prepare(host: &HostContext, path: &Path, ignored: bool) -> Result<Self> {
        validate_path(path)?;
        let (mut rules, before) = read(host)?;
        if ignored {
            rules.paths.insert(path.to_owned());
        } else {
            rules.paths.remove(path);
        }
        rules.validate()?;
        Ok(Self {
            host: host.clone(),
            before,
            rules,
            path: path.to_owned(),
            ignored,
        })
    }
    pub fn human(&self) -> String {
        format!(
            "{} discovery path: {:?}\nScope: this HOME, across repositories. Exact path and descendants; no globs.\nOnly private local discovery preferences change. No live files, repository mappings or journals change.\nUnignore is available in Actions > Manage ignored paths. Other ancestor rules and safety exclusions still apply.",
            if self.ignored { "Ignore" } else { "Unignore" },
            self.path
        )
    }
    pub fn approve(self) -> Approval {
        Approval(self)
    }
}
pub struct Approval(Preview);
impl Approval {
    pub fn execute(self, host: &HostContext, dry_run: bool) -> Result<()> {
        if dry_run {
            return Err(Error::Invalid(
                "dry-run cannot change discovery preferences".into(),
            ));
        }
        let preview = self.0;
        if preview.host.home != host.home
            || preview.host.config_dir != host.config_dir
            || preview.host.state_dir != host.state_dir
        {
            return Err(Error::Invalid(
                "discovery preference approval belongs to a different host".into(),
            ));
        }
        let lock = WriterLock::acquire(host)?;
        lock.check_context(host)?;
        crate::journal::ensure_writable(host)?;
        if read(host)?.1 != preview.before {
            return Err(Error::Invalid(
                "discovery preferences changed; request a fresh preview".into(),
            ));
        }
        let text = toml::to_string(&preview.rules).map_err(|_| invalid())?;
        if text.len() > MAX_BYTES {
            return Err(invalid());
        }
        let dir = PrivateDir::open(&host.config_dir, true)?.expect("created config directory");
        dir.replace(NAME, &text)
    }
}
