//! Bounded, read-only Git executable adapter. This is hardening, not OS sandboxing.
//! The caller supplies a trusted absolute executable; no environment is discovered here.
use crate::{Error, Result};
use serde::Serialize;
use std::{
    io::Read,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const LIMIT: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

fn failure(message: &'static str) -> Error {
    Error::Invalid(message.into())
}

/// Fixed-operation backend; intentionally no arbitrary arguments/revisions or write API.
pub struct Git {
    executable: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub schema: u32,
    pub read_only: bool,
    pub branch: Vec<String>,
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Serialize)]
pub struct StatusEntry {
    pub kind: String,
    pub xy: Option<String>,
    pub path: String,
    pub original_path: Option<String>,
}

#[derive(Serialize)]
pub struct Diff {
    pub schema: u32,
    pub read_only: bool,
    pub staged: bool,
    pub entries: Vec<DiffEntry>,
    /// Only populated after an explicit content-disclosure request. Bytes are escaped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

// Debug output must not accidentally turn an explicitly disclosed report into
// patch content in diagnostics/logs.
impl std::fmt::Debug for Diff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Diff")
            .field("schema", &self.schema)
            .field("read_only", &self.read_only)
            .field("staged", &self.staged)
            .field("entries", &self.entries)
            .field("patch_requested", &self.patch.is_some())
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct DiffEntry {
    pub change: String,
    pub old_mode: String,
    pub new_mode: String,
    pub path: String,
}

impl Git {
    pub fn new(executable: PathBuf) -> Result<Self> {
        if !executable.is_absolute() {
            return Err(failure("Git executable must be an absolute trusted path"));
        }
        Ok(Self { executable })
    }

    pub fn status(&self, repository: &Path) -> Result<Status> {
        let root = self.preflight(repository)?;
        let bytes = self.checked(
            &root,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
                "--ignore-submodules=all",
            ],
        )?;
        parse_status(&bytes)
    }

    pub fn diff(&self, repository: &Path, staged: bool, patch: bool) -> Result<Diff> {
        let root = self.preflight(repository)?;
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            "--ignore-submodules=all",
            "--submodule=short",
        ];
        if staged {
            args.push("--cached");
        }
        args.extend(["--raw", "-z", "--abbrev=64", "--"]);
        let entries = parse_diff(&self.checked(&root, &args)?)?;
        let patch = if patch {
            // Recheck configuration before the separate content read. Still not a
            // guarantee against a malicious same-user process racing Git's config.
            self.preflight(&root)?;
            args.truncate(args.len() - 4);
            // Do not request --binary; Git emits only a binary-change marker.
            args.extend(["--patch", "--unified=3", "--"]);
            Some(escape_patch(&self.checked(&root, &args)?))
        } else {
            None
        };
        Ok(Diff {
            schema: 1,
            read_only: true,
            staged,
            entries,
            patch,
        })
    }

    fn preflight(&self, repository: &Path) -> Result<PathBuf> {
        if !repository.is_absolute() {
            return Err(failure("Git repository must be an absolute path"));
        }
        let root = repository
            .canonicalize()
            .map_err(|_| failure("Git repository is not an accessible directory"))?;
        if !root.is_dir() {
            return Err(failure("Git repository is not a directory"));
        }
        // Reading configuration does not invoke the configured programs. Reject
        // filters even if unused/disabled, and promisor declarations even if false.
        let config = self.run(
            &root,
            &[
                "config",
                "--includes",
                "--null",
                "--name-only",
                "--get-regexp",
                "^filter\\.|^extensions\\.partialclone$|^remote\\..*\\.promisor$",
            ],
        )?;
        match config.code {
            Some(1) if config.stdout.is_empty() => {}
            Some(0) if !config.stdout.is_empty() => {
                return Err(failure(
                    "Git read refused: configured content filters or partial clone/promisor settings",
                ));
            }
            _ => return Err(failure("Git configuration inspection failed")),
        }
        let top = self.checked(&root, &["rev-parse", "--show-toplevel"])?;
        // Strip exactly Git's terminating LF, not whitespace in the actual path.
        let top = top
            .strip_suffix(b"\n")
            .ok_or_else(|| failure("invalid Git root response"))?;
        if top != root.as_os_str().as_bytes() {
            return Err(failure(
                "--repo must name the exact non-bare Git working-tree root",
            ));
        }
        Ok(root)
    }

    fn checked(&self, root: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = self.run(root, args)?;
        if output.code != Some(0) {
            // Never echo stderr: it may contain configuration values or file bodies.
            return Err(failure(
                "Git read failed (invalid repository, unsupported Git, or read error)",
            ));
        }
        Ok(output.stdout)
    }

    fn run(&self, root: &Path, args: &[&str]) -> Result<Output> {
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .env("HOME", "/no-external-programs")
            .env("XDG_CONFIG_HOME", "/no-external-programs")
            .env("PATH", "/no-external-programs")
            .env("GIT_EXEC_PATH", "/no-external-programs")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .current_dir(root)
            .args(["--no-pager", "--no-optional-locks"]);
        for setting in [
            "core.fsmonitor=false",
            "core.untrackedCache=false",
            "core.hooksPath=/dev/null",
            "core.attributesFile=/dev/null",
            "core.excludesFile=/dev/null",
            "diff.external=",
            "diff.autoRefreshIndex=false",
            "diff.ignoreSubmodules=all",
            "submodule.recurse=false",
            "core.quotePath=true",
            "color.ui=false",
        ] {
            command.args(["-c", setting]);
        }
        command.args(args);
        run_bounded(command, LIMIT, TIMEOUT)
    }
}

struct Output {
    code: Option<i32>,
    stdout: Vec<u8>,
}

// Nonblocking concurrent drains: neither pipe can deadlock the child. No reader
// thread can outlive a deadline if a descendant retains a pipe. Kill/reap the
// isolated process group on every unsuccessful completion (including limits).
fn run_bounded(mut command: Command, limit: usize, timeout: Duration) -> Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| failure("Git executable unavailable or could not start"))?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| failure("Git stdout unavailable"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| failure("Git stderr unavailable"))?;
        use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
        for pipe in [&stdout as &dyn std::os::fd::AsFd, &stderr] {
            let flags = fcntl_getfl(pipe).map_err(|_| failure("Git pipe setup failed"))?;
            fcntl_setfl(pipe, flags | OFlags::NONBLOCK)
                .map_err(|_| failure("Git pipe setup failed"))?;
        }
        let start = Instant::now();
        let mut bytes = Vec::new();
        let mut total = 0usize;
        let mut out_done = false;
        let mut err_done = false;
        loop {
            if start.elapsed() >= timeout {
                return Err(failure("Git read exceeded time limit"));
            }
            // One chunk from each per iteration prevents starvation by either stream.
            for (pipe, done, keep) in [
                (&mut stdout as &mut dyn Read, &mut out_done, true),
                (&mut stderr as &mut dyn Read, &mut err_done, false),
            ] {
                if *done {
                    continue;
                }
                let mut buffer = [0; 8192];
                match pipe.read(&mut buffer) {
                    Ok(0) => *done = true,
                    Ok(count) => {
                        total += count;
                        if total > limit {
                            return Err(failure("Git read exceeded output limit"));
                        }
                        if keep {
                            bytes.extend_from_slice(&buffer[..count]);
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(failure("Git pipe read failed")),
                }
            }
            if let Some(status) = child.try_wait().map_err(|_| failure("Git wait failed"))?
                && out_done
                && err_done
            {
                return Ok(Output {
                    code: status.code(),
                    stdout: bytes,
                });
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    // Kill the group even when the direct child already exited: descendants may
    // still hold pipes. We never use external kill helpers or a shell here.
    if result.is_err()
        && let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.wait();
    result
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| failure("Git report contains non-UTF-8 metadata/path"))
}

fn records(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(&[0]) {
        return Err(failure("malformed Git NUL protocol"));
    }
    Ok(bytes[..bytes.len() - 1].split(|b| *b == 0).collect())
}

fn parse_status(bytes: &[u8]) -> Result<Status> {
    let records = records(bytes)?;
    let mut branch = Vec::new();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = text(records[index])?;
        index += 1;
        if let Some(header) = record.strip_prefix("# branch.") {
            branch.push(header.to_owned());
            continue;
        }
        let (kind, fields) = match record.as_bytes().first() {
            Some(b'1') => ("ordinary", 8),
            Some(b'2') => ("rename_or_copy", 9),
            Some(b'u') => ("unmerged", 10),
            Some(b'?') => ("untracked", 1),
            _ => return Err(failure("malformed Git status record")),
        };
        let parts: Vec<_> = record.splitn(fields + 1, ' ').collect();
        if parts.len() != fields + 1 || parts[fields].is_empty() || parts[..fields].contains(&"") {
            return Err(failure("malformed Git status fields"));
        }
        let xy = if fields > 1 {
            let sub = parts[2].as_bytes();
            let valid_sub = sub == b"N..."
                || (sub.len() == 4
                    && sub[0] == b'S'
                    && b".C".contains(&sub[1])
                    && b".M".contains(&sub[2])
                    && b".U".contains(&sub[3]));
            let (modes, hashes) = if kind == "unmerged" {
                (&parts[3..7], &parts[7..10])
            } else {
                (&parts[3..6], &parts[6..8])
            };
            if !valid_sub
                || !modes.iter().all(|m| valid_mode(m))
                || !hashes.iter().all(|h| valid_hash(h))
            {
                return Err(failure("malformed Git status metadata"));
            }
            if kind == "rename_or_copy" {
                let score = parts[8];
                if !score.starts_with(['R', 'C'])
                    || score[1..].parse::<u8>().ok().is_none_or(|n| n > 100)
                {
                    return Err(failure("malformed Git rename score"));
                }
            }
            let xy = parts[1];
            if xy.len() != 2 || !xy.bytes().all(|b| b".MADRCUT?!".contains(&b)) {
                return Err(failure("malformed Git status change code"));
            }
            Some(xy.to_owned())
        } else {
            None
        };
        let original_path = if kind == "rename_or_copy" {
            let old = records
                .get(index)
                .ok_or_else(|| failure("missing Git rename path"))?;
            index += 1;
            let old = text(old)?;
            if old.is_empty() {
                return Err(failure("empty Git rename path"));
            }
            Some(old.to_owned())
        } else {
            None
        };
        entries.push(StatusEntry {
            kind: kind.into(),
            xy,
            path: parts[fields].into(),
            original_path,
        });
    }
    Ok(Status {
        schema: 1,
        read_only: true,
        branch,
        entries,
    })
}

fn valid_mode(mode: &str) -> bool {
    mode.len() == 6 && mode.bytes().all(|b| (b'0'..=b'7').contains(&b))
}

fn valid_hash(hash: &str) -> bool {
    matches!(hash.len(), 40 | 64) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_diff(bytes: &[u8]) -> Result<Vec<DiffEntry>> {
    let records = records(bytes)?;
    if records.len() % 2 != 0 {
        return Err(failure("malformed Git raw diff"));
    }
    records
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let header = text(pair[0])?
                .strip_prefix(':')
                .ok_or_else(|| failure("malformed Git raw header"))?;
            let parts: Vec<_> = header.split(' ').collect();
            if parts.len() != 5
                || !parts[0..2].iter().all(|m| valid_mode(m))
                || !parts[2..4].iter().all(|h| valid_hash(h))
                || parts[4].len() != 1
                || !b"ACDMTUXB".contains(&parts[4].as_bytes()[0])
            {
                return Err(failure("malformed Git raw diff fields"));
            }
            let path = text(pair[1])?;
            if path.is_empty() {
                return Err(failure("empty Git diff path"));
            }
            Ok(DiffEntry {
                change: parts[4].into(),
                old_mode: parts[0].into(),
                new_mode: parts[1].into(),
                path: path.into(),
            })
        })
        .collect()
}

/// Preserve patch line breaks, escape every other control/non-ASCII byte and
/// backslash. The same safe representation is used for JSON and terminal output.
fn escape_patch(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for byte in bytes {
        match byte {
            b'\n' => output.push('\n'),
            b'\\' => output.push_str("\\\\"),
            0x20..=0x7e => output.push(char::from(*byte)),
            _ => {
                let _ = write!(output, "\\x{byte:02x}");
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nul_parsers_and_escaping_fail_closed() {
        let status = parse_status(b"# branch.head (detached)\0? line\nname\0").unwrap();
        assert_eq!(status.entries[0].path, "line\nname");
        assert!(parse_status(b"? x").is_err());
        assert!(parse_status(b"? \xff\0").is_err());
        assert!(parse_status(b"2 R. N... 100644 100644 100644 a b R100 new\0").is_err());
        assert!(parse_diff(b":100644 100644 bad bad M\0x\0").is_err());
        assert_eq!(
            escape_patch(b"a\x1b\r\t\xff\\\n"),
            "a\\x1b\\x0d\\x09\\xff\\\\\n"
        );
    }

    #[test]
    fn runner_bounds_both_streams_deadlines_and_redacts_errors() {
        let command = |script: &str| {
            let mut command = Command::new("/bin/sh");
            command.env_clear().args(["-c", script]);
            command
        };
        for script in [
            "while :; do printf secret; done",
            "while :; do printf secret >&2; done",
        ] {
            let error = run_bounded(command(script), 1024, Duration::from_secs(2))
                .err()
                .unwrap();
            assert!(error.to_string().contains("output limit"));
            assert!(!error.to_string().contains("secret"));
        }
        let start = Instant::now();
        assert!(
            run_bounded(
                command("while :; do :; done"),
                1024,
                Duration::from_millis(30)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        // Descendant retains pipes after the direct child exits.
        assert!(
            run_bounded(
                command("(while :; do :; done) & exit 0"),
                1024,
                Duration::from_millis(30)
            )
            .is_err()
        );
    }
}
