use super::template_buffer::validate_plain_text;
use dort_core::{Error, Result, generated::TEMPLATE_LIMIT};
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read, Write},
    os::fd::OwnedFd,
    process::{Command, Stdio},
};

const NAME: &str = "template.txt";

pub(super) struct Edit {
    _temp: tempfile::TempDir,
    directory: OwnedFd,
    command: Command,
}

fn problem(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

impl Edit {
    pub fn prepare(text: &str) -> Result<Self> {
        Self::with_config(std::env::var_os("VISUAL"), std::env::var_os("EDITOR"), text)
    }

    pub fn attach_tty(&mut self, tty: &File) -> io::Result<()> {
        self.command
            .stdin(Stdio::from(tty.try_clone()?))
            .stdout(Stdio::from(tty.try_clone()?))
            .stderr(Stdio::from(tty.try_clone()?));
        Ok(())
    }

    pub(super) fn with_config(
        visual: Option<OsString>,
        editor: Option<OsString>,
        text: &str,
    ) -> Result<Self> {
        let selected = [visual, editor]
            .into_iter()
            .flatten()
            .find(|value| value.to_str().is_none_or(|value| !value.trim().is_empty()));
        let Some(selected) = selected else {
            return Err(problem(
                "Set VISUAL or EDITOR before starting Dort, or edit the Template field here.",
            ));
        };
        let args = selected
            .to_str()
            .and_then(shlex::split)
            .filter(|args| args.first().is_some_and(|program| !program.is_empty()))
            .ok_or_else(|| {
                problem("Check the command and quotes in VISUAL or EDITOR, then restart Dort.")
            })?;
        let storage_error = |_| {
            problem("Could not create a private template file. Check your temporary directory.")
        };
        let temp = tempfile::Builder::new()
            .prefix("dort-template-")
            .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .tempdir()
            .map_err(storage_error)?;
        let directory = fs::open(
            temp.path(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| storage_error(e.into()))?;
        let fd = fs::openat(
            &directory,
            NAME,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|e| storage_error(e.into()))?;
        File::from(fd)
            .write_all(text.as_bytes())
            .map_err(storage_error)?;
        let mut command = Command::new(&args[0]);
        command.args(&args[1..]).arg(temp.path().join(NAME));
        Ok(Self {
            _temp: temp,
            directory,
            command,
        })
    }

    pub fn run(mut self) -> Result<String> {
        let status = self
            .command
            .status()
            .map_err(|_| problem("Could not start the editor. Check VISUAL or EDITOR."))?;
        if !status.success() {
            return Err(problem(format!(
                "Editor exited unsuccessfully ({status}). The draft was kept."
            )));
        }
        self.read()
    }

    fn read(&self) -> Result<String> {
        let invalid =
            || problem("The editor must save a regular template file. The draft was kept.");
        let regular = |stat: &fs::Stat| {
            FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile && stat.st_nlink == 1
        };
        let before =
            fs::statat(&self.directory, NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| invalid())?;
        if !regular(&before) {
            return Err(invalid());
        }
        let file = fs::openat(
            &self.directory,
            NAME,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| invalid())?;
        let opened = fs::fstat(&file).map_err(|_| invalid())?;
        if !regular(&opened) || before.st_dev != opened.st_dev || before.st_ino != opened.st_ino {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        File::from(file)
            .take(TEMPLATE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| problem("Could not read the template file. The draft was kept."))?;
        if bytes.len() > TEMPLATE_LIMIT {
            return Err(problem("Template exceeds 64 KiB. The draft was kept."));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| problem("Save the template as UTF-8. The draft was kept."))?;
        validate_plain_text(&text)?;
        Ok(text)
    }
}

pub(super) struct ForegroundSignals(Vec<(libc::c_int, libc::sigaction)>);

impl ForegroundSignals {
    pub fn retain_parent() -> io::Result<Self> {
        extern "C" fn leave_to_editor(_: libc::c_int) {}
        let mut signals = Self(Vec::new());
        for signal in [libc::SIGINT, libc::SIGQUIT] {
            // A caught signal stays local to Dort; exec resets the child's handler.
            let previous = unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = leave_to_editor as *const () as libc::sighandler_t;
                libc::sigemptyset(&mut action.sa_mask);
                let mut previous = std::mem::MaybeUninit::uninit();
                if libc::sigaction(signal, &action, previous.as_mut_ptr()) != 0 {
                    return Err(io::Error::last_os_error());
                }
                previous.assume_init()
            };
            signals.0.push((signal, previous));
        }
        Ok(signals)
    }
}

impl Drop for ForegroundSignals {
    fn drop(&mut self) {
        for (signal, previous) in &self.0 {
            // Restore dispositions saved by sigaction when handing off the foreground.
            unsafe { libc::sigaction(*signal, previous, std::ptr::null_mut()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn editor_receives_quoted_arguments_and_only_the_private_template() {
        let fixture = tempfile::tempdir().unwrap();
        let editor = fixture.path().join("fake editor");
        std::fs::write(
            &editor,
            r#"#!/bin/sh
set -eu
[ "$#" = 2 ]
[ "$1" = 'quoted; $value' ]
[ "$(cat "$2")" = 'original' ]
printf 'first\r\nsecond\n' > "$2.new"
mv "$2.new" "$2"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o700)).unwrap();
        let command = format!("\"{}\" 'quoted; $value'", editor.display());
        for (visual, fallback) in [
            (command.clone(), "/no-such-editor".into()),
            (" ".into(), command),
        ] {
            let edit = Edit::with_config(Some(visual.into()), Some(fallback.into()), "original\n")
                .unwrap();
            let dir = edit._temp.path().to_path_buf();
            assert_eq!(
                std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(dir.join(NAME))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            assert_eq!(edit.run().unwrap(), "first\r\nsecond\n");
            assert!(!dir.exists());
        }
    }

    #[test]
    fn editor_failure_or_invalid_output_never_returns_a_replacement_and_cleans_up() {
        for (script, message) in [
            (
                r#"printf new > "$1"; exit 4"#,
                "Editor exited unsuccessfully",
            ),
            (
                r#"rm "$1" && ln -s /etc/hosts "$1""#,
                "regular template file",
            ),
            (r#"rm "$1" && mkdir "$1""#, "regular template file"),
            (r#"rm "$1" && mkfifo "$1""#, "regular template file"),
            (
                r#"mv "$1" "$1.copy" && ln "$1.copy" "$1""#,
                "regular template file",
            ),
            (r#"printf '\377' > "$1""#, "as UTF-8"),
            (r#"printf '\033' > "$1""#, "plain text"),
            (r#"head -c 65537 /dev/zero > "$1""#, "exceeds 64 KiB"),
        ] {
            let quoted = format!("/bin/sh -c {} editor", shlex::try_quote(script).unwrap());
            let edit = Edit::with_config(None, Some(quoted.into()), "original\n").unwrap();
            let dir = edit._temp.path().to_path_buf();
            let error = edit.run().unwrap_err().to_string();
            assert!(error.contains(message), "{script}: {error}");
            assert!(!dir.exists());
        }
        for (command, message) in [
            ("'no closing quote", "Check the command and quotes"),
            ("/no-such-editor", "Could not start the editor"),
        ] {
            let result = Edit::with_config(Some(command.into()), Some("/bin/true".into()), "old")
                .and_then(Edit::run);
            assert!(result.unwrap_err().to_string().contains(message));
        }
    }
}
