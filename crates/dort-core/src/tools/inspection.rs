//! Deliberate provider queries, never called by passive observation/planning.
//! Provider configuration/plugins may execute and caches may change. Not a sandbox.
use crate::model::Provider;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

mod catalog;

const LIMIT: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);
pub const WARNING: &str = "Provider inspection executes a trusted local program. Global configuration and plugins may execute code; caches/logs may change. Offline settings are hardening, not an OS sandbox or a guarantee against network access. No packages are installed by Dort. Results are provider claims, not Dort ownership or verified binary identity.";

/// Only these fixed query protocols are implemented. Apt uses dpkg-query, not apt.
/// npm requires an explicit declared-runtime/Node/CLI/prefix binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    BrewFormulae,
    DpkgPackages,
    MiseInstalled,
    NpmGlobals,
}
impl Query {
    pub fn provider(self) -> Provider {
        match self {
            Self::BrewFormulae => Provider::Brew,
            Self::DpkgPackages => Provider::Apt,
            Self::MiseInstalled => Provider::Mise,
            Self::NpmGlobals => Provider::Npm,
        }
    }
    fn arguments(self) -> Vec<String> {
        let args: &[&str] = match self {
            Self::BrewFormulae => &["info", "--json=v2", "--installed", "--formula"],
            Self::DpkgPackages => &[
                "--no-pager",
                "--show",
                "--showformat=${binary:Package}\\t${Version}\\t${db:Status-Status}\\t${db:Status-Eflag}\\n",
            ],
            Self::MiseInstalled => &["ls", "--installed", "--json"],
            Self::NpmGlobals => &[
                "ls",
                "--global",
                "--json",
                "--depth=0",
                "--offline",
                "--ignore-scripts",
                "--logs-max=0",
                "--update-notifier=false",
                "--audit=false",
                "--fund=false",
            ],
        };
        args.iter().map(|s| (*s).to_owned()).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    #[error("inspection requires absolute UTF-8 paths without dot components")]
    InvalidPath,
    #[error("inspection requires an explicit nonempty trusted executable search path")]
    InvalidSearchPath,
    #[error(
        "npm inspection requires an explicit declared runtime, Node, npm CLI and prefix binding"
    )]
    InvalidNpmBinding,
    #[error("provider executable unavailable or could not start")]
    Unavailable,
    #[error("provider pipe setup/read failed")]
    Pipe,
    #[error("provider wait failed")]
    Wait,
    #[error("provider query exceeded time limit")]
    Timeout,
    #[error("provider query exceeded output limit")]
    OutputLimit,
    #[error("provider query failed; raw output withheld")]
    QueryFailed,
    #[error("provider returned malformed or unsupported metadata; raw output withheld")]
    Malformed,
    #[error("package database contains incomplete or repair-required packages")]
    IncompletePackages,
}

/// Whitelisted metadata only. No raw stdout/stderr, configuration or environment values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Installation {
    /// Provider-reported identifier, not inferred from a binary name.
    pub package: String,
    pub version: String,
}

/// A failed observation is never represented as a healthy empty catalog.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Observation {
    Complete { installations: Vec<Installation> },
    Unavailable { reason: Failure },
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub description: Description,
    pub observation: Observation,
}
/// Explicit caller-selected scope, not proof that Node is installed/owned by Dort.
/// The npm launcher is never run: the selected Node executes the selected CLI script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpmBinding {
    pub runtime_tool: String,
    pub node: PathBuf,
    pub npm_cli: PathBuf,
    pub prefix: PathBuf,
}

/// Inspectable description; deserializing a report never grants execution authority.
#[derive(Debug, Clone, Serialize)]
pub struct Description {
    pub query: Query,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: &'static str,
    pub home: PathBuf,
    pub trusted_search_path: Vec<PathBuf>,
    pub npm_binding: Option<NpmBinding>,
    pub warning: &'static str,
    pub output_limit_bytes: usize,
    pub timeout_seconds: u64,
}
/// An in-memory capability with fixed arguments and no arbitrary environment/command API.
/// Constructing/describing it does not discover paths, read files or start a process.
pub struct Preview {
    description: Description,
}
/// Single-use authority, deliberately not Clone/Debug/Serialize/Deserialize.
pub struct Approval(Preview);

pub(super) fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.to_str().is_some()
        && path.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && !path.as_os_str().as_encoded_bytes().contains(&0)
        // Path::components normalizes interior '.'; reject it in the supplied spelling too.
        && !path.to_str().unwrap_or("").split('/').any(|c| c == "." || c == "..")
}
impl Preview {
    /// All paths are explicit application-boundary trust decisions, never manifest strings.
    /// The query runs at /, not in a repository. The caller must not probe PATH candidates
    /// to decide whether they are trustworthy. No trust is persisted here.
    pub fn new(
        query: Query,
        executable: PathBuf,
        home: PathBuf,
        trusted_search_path: Vec<PathBuf>,
    ) -> Result<Self, Failure> {
        if query == Query::NpmGlobals {
            return Err(Failure::InvalidNpmBinding);
        }
        Self::with_binding(query, executable, home, trusted_search_path, None)
    }
    /// The binding must name a runtime declared in this manifest. No version,
    /// executable identity or installed-runtime association is inferred from it.
    pub fn npm(
        manifest: &crate::Manifest,
        binding: NpmBinding,
        home: PathBuf,
        trusted_search_path: Vec<PathBuf>,
    ) -> Result<Self, Failure> {
        if !manifest.tools.iter().any(|t| t.id == binding.runtime_tool) {
            return Err(Failure::InvalidNpmBinding);
        }
        Self::npm_probe(binding, home, trusted_search_path)
    }
    /// Historical metadata inspection only. Does not assert a current declaration,
    /// runtime satisfaction or installation eligibility. Never an install capability.
    pub(crate) fn npm_probe(
        binding: NpmBinding,
        home: PathBuf,
        trusted_search_path: Vec<PathBuf>,
    ) -> Result<Self, Failure> {
        if binding.runtime_tool.is_empty()
            || binding.runtime_tool.len() > 256
            || !binding
                .runtime_tool
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(Failure::InvalidNpmBinding);
        }
        if !absolute(&binding.node) || !absolute(&binding.npm_cli) || !absolute(&binding.prefix) {
            return Err(Failure::InvalidPath);
        }
        Self::with_binding(
            Query::NpmGlobals,
            binding.node.clone(),
            home,
            trusted_search_path,
            Some(binding),
        )
    }
    fn with_binding(
        query: Query,
        executable: PathBuf,
        home: PathBuf,
        trusted_search_path: Vec<PathBuf>,
        npm_binding: Option<NpmBinding>,
    ) -> Result<Self, Failure> {
        if !absolute(&executable) || !absolute(&home) {
            return Err(Failure::InvalidPath);
        }
        if trusted_search_path.is_empty()
            || trusted_search_path.iter().any(|p| !absolute(p))
            || std::env::join_paths(&trusted_search_path).is_err()
        {
            return Err(Failure::InvalidSearchPath);
        }
        let mut arguments = query.arguments();
        if let Some(binding) = &npm_binding {
            arguments.insert(
                0,
                binding
                    .npm_cli
                    .to_str()
                    .ok_or(Failure::InvalidPath)?
                    .to_owned(),
            );
            arguments.push("--prefix".into());
            arguments.push(
                binding
                    .prefix
                    .to_str()
                    .ok_or(Failure::InvalidPath)?
                    .to_owned(),
            );
        }
        Ok(Self {
            description: Description {
                query,
                executable,
                arguments,
                working_directory: "/",
                home,
                trusted_search_path,
                npm_binding,
                warning: WARNING,
                output_limit_bytes: LIMIT,
                timeout_seconds: TIMEOUT.as_secs(),
            },
        })
    }
    pub fn description(&self) -> &Description {
        &self.description
    }
    /// Presentation calls this only after deliberate approval of this exact live preview.
    /// This is inspection authority only, never installation/mutation approval.
    pub fn approve(self) -> Approval {
        Approval(self)
    }
}
impl Approval {
    pub fn inspect(self) -> Report {
        let d = self.0.description;
        let mut command = Command::new(&d.executable);
        command
            .env_clear()
            .env("HOME", &d.home)
            .env("XDG_CONFIG_HOME", d.home.join(".config"))
            .env("XDG_CACHE_HOME", d.home.join(".cache"))
            .env("XDG_DATA_HOME", d.home.join(".local/share"))
            .env("XDG_STATE_HOME", d.home.join(".local/state"))
            .env(
                "PATH",
                std::env::join_paths(&d.trusted_search_path).expect("validated search path"),
            )
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("NO_COLOR", "1")
            .current_dir(d.working_directory)
            .args(&d.arguments);
        match d.query {
            Query::BrewFormulae => {
                command
                    .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                    .env("HOMEBREW_NO_ANALYTICS", "1")
                    .env("HOMEBREW_NO_ENV_HINTS", "1")
                    .env("HOMEBREW_NO_INSTALL_FROM_API", "1");
            }
            Query::MiseInstalled => {
                command
                    .env("MISE_OFFLINE", "1")
                    .env("MISE_AUTO_INSTALL", "0")
                    .env("MISE_YES", "0");
            }
            Query::DpkgPackages => {
                command.env("DPKG_PAGER", "");
            }
            Query::NpmGlobals => {}
        }
        let result =
            run_bounded(command, LIMIT, TIMEOUT).and_then(|bytes| catalog::parse(d.query, &bytes));
        Report {
            description: d,
            observation: match result {
                Ok(installations) => Observation::Complete { installations },
                Err(reason) => Observation::Unavailable { reason },
            },
        }
    }
}

// Private raw payloads never implement Debug/Serialize or escape on errors. Concurrent,
// nonblocking drains bound combined stdout/stderr even if a descendant holds a pipe.
fn run_bounded(mut command: Command, limit: usize, timeout: Duration) -> Result<Vec<u8>, Failure> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().map_err(|_| Failure::Unavailable)?;
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or(Failure::Pipe)?;
        let mut stderr = child.stderr.take().ok_or(Failure::Pipe)?;
        use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
        for pipe in [&stdout as &dyn std::os::fd::AsFd, &stderr] {
            let flags = fcntl_getfl(pipe).map_err(|_| Failure::Pipe)?;
            fcntl_setfl(pipe, flags | OFlags::NONBLOCK).map_err(|_| Failure::Pipe)?;
        }
        let start = Instant::now();
        let mut bytes = Vec::new();
        let mut total = 0usize;
        let (mut out_done, mut err_done) = (false, false);
        loop {
            if start.elapsed() >= timeout {
                return Err(Failure::Timeout);
            }
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
                            return Err(Failure::OutputLimit);
                        }
                        if keep {
                            bytes.extend_from_slice(&buffer[..count]);
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(Failure::Pipe),
                }
            }
            if let Some(status) = child.try_wait().map_err(|_| Failure::Wait)?
                && out_done
                && err_done
            {
                return if status.success() {
                    Ok(bytes)
                } else {
                    Err(Failure::QueryFailed)
                };
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    // Kill remaining descendants on success too; an inspection never owns a background job.
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests;
