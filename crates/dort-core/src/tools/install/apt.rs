//! Root-native APT installation. Only the trusted sudo supervisor owns the root timeout.
use super::*;
#[cfg(test)]
#[path = "apt_process_tests.rs"]
mod process_tests;
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};

pub const WARNING: &str = "ROOT RISK: installs one missing literal Ubuntu package at the provider-current version, not an exact pin. Root hooks, maintainer scripts, triggers and dependency code CAN EXECUTE. Dependencies can install/upgrade; services can start/restart; packages, configuration, live links, databases, caches and data outside HOME can change. --no-upgrade protects only the named target; --no-remove only forbids removals. Trusted native root configuration, endpoints, sudo policy and PAM apply; cleared caller environment is NOT root isolation. No sandbox, atomicity, readiness or rollback. sudo -n refuses prompts; sudo -T 600 requires policy permission. No password handling, policy changes or untimed fallback. Only trusted sudo supervises privileged termination: if it has not exited, Dort keeps waiting joined, not detaching or claiming root descendants were killed. Closure cannot prove root termination or repair.";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub apt_cache: PathBuf,
    pub apt_get: PathBuf,
    pub sudo: PathBuf,
    pub policy: VersionPolicy,
    pub catalogs: Vec<npm::Catalog>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Probe {
    candidate: String,
    sudo_timeout_seconds: u64,
}
impl Probe {
    pub(super) fn valid(&self) -> bool {
        version(&self.candidate) && self.sudo_timeout_seconds == TIMEOUT_SECONDS
    }
}
/// Literal subset: no regex fallback, architecture/version/release/path syntax,
/// options or trailing +/- actions. Rejected spellings are never normalized.
pub fn package(name: &str) -> bool {
    name.len() >= 2
        && name.len() <= 128
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
/// A co-located physical program is only a metadata hint, never searched/executed.
pub fn program_hint(dpkg: &Path, name: &str) -> Option<PathBuf> {
    let path = dpkg.parent()?.join(name);
    npm::program(&path, true).ok().map(|_| path)
}
pub(super) fn valid(s: &super::Scope) -> bool {
    let Some(a) = &s.apt else { return false };
    s.platform == crate::platform::Platform::Ubuntu
        && s.npm.is_none()
        && s.brew.is_none()
        && package(&s.package)
        && matches!(
            a.policy,
            VersionPolicy::ProviderDefault {} | VersionPolicy::Latest {}
        )
        && s.version
            == if matches!(a.policy, VersionPolicy::Latest {}) {
                "latest"
            } else {
                "provider_default"
            }
        && [&a.apt_cache, &a.apt_get, &a.sudo]
            .into_iter()
            .all(|p| inspection::absolute(p))
        && valid_catalogs(s, a)
}
fn valid_catalogs(s: &super::Scope, a: &Scope) -> bool {
    let mut ids = BTreeSet::new();
    !a.catalogs.is_empty()
        && a.catalogs.len() <= 16
        && a.catalogs.iter().all(|c| {
            !c.tools.is_empty()
                && c.tools.len() <= 256
                && c.search_dirs.len() <= 256
                && c.preview(&s.home).is_ok()
                && c.tools.iter().all(|id| ids.insert(id.clone()))
                && (!c.tools.contains(&s.tool)
                    || (c.query == inspection::Query::DpkgPackages
                        && c.executable == s.executable
                        && c.search_dirs == s.search_dirs))
        })
        && ids == s.bound_tools
}
pub fn prepare(
    host: &HostContext,
    request: Request,
    apt: Scope,
    reports: &[&inspection::Report],
) -> Result<Preview> {
    let (manifest, scope, sources) = super::sources(host, &request, None, None, Some(&apt))?;
    super::bounded_metadata(&scope, &sources)?;
    catalogs(&scope, &apt, reports)?;
    let row = super::evaluate_catalogs(host, &manifest, &scope, reports)?;
    if row.action != planning::PackageAction::ReviewMissingPackage || !row.blockers.is_empty() {
        return Err(invalid(
            "only an observed missing literal APT package with complete dpkg inventory and clear explicit prerequisites can be reviewed",
        ));
    }
    Ok(Preview {
        request,
        scope,
        sources,
    })
}
pub(super) fn catalogs<'a>(
    s: &super::Scope,
    a: &Scope,
    reports: &[&'a inspection::Report],
) -> Result<BTreeMap<String, &'a inspection::Report>> {
    if a.catalogs.len() != reports.len() {
        return Err(invalid("APT catalog count differs"));
    }
    let mut out = BTreeMap::new();
    for (c, r) in a.catalogs.iter().zip(reports) {
        if !c.matches(&s.home, r) {
            return Err(invalid("APT prerequisite catalog scope differs"));
        }
        for id in &c.tools {
            if out.insert(id.clone(), *r).is_some() {
                return Err(invalid("duplicate APT catalog binding"));
            }
        }
    }
    Ok(out)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sources {
    bindings: npm::Sources,
    parents: Vec<observation::Parent>,
}
pub(super) fn same(a: &Sources, b: &Sources) -> bool {
    npm::same(&a.bindings, &b.bindings)
        && a.parents.len() == b.parents.len()
        && a.parents.iter().zip(&b.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}
pub(super) fn sources(s: &super::Scope) -> Result<Sources> {
    let a = s.apt.as_ref().expect("APT scope");
    let mut programs = vec![
        npm::program(&s.executable, true)?,
        npm::program(&a.apt_cache, true)?,
        npm::program(&a.apt_get, true)?,
        npm::program(&a.sudo, true)?,
    ];
    let mut lookup = BTreeSet::new();
    lookup.extend(s.search_dirs.iter().cloned());
    lookup.extend(s.tool_search_dirs.iter().cloned());
    for c in &a.catalogs {
        programs.push(npm::program(&c.executable, true)?);
        if let Some(n) = &c.npm_binding {
            programs.push(npm::program(&n.npm_cli, false)?);
        }
        lookup.extend(c.search_dirs.iter().cloned());
    }
    let tool_directories = lookup
        .iter()
        .map(|p| {
            let e = observation::inspect(p, false)?;
            if e.kind != Kind::Directory {
                return Err(invalid("APT lookup directory unavailable or unsafe"));
            }
            Ok(e)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut bindings = npm::Sources {
        programs,
        directories: vec![],
        tool_directories,
    };
    let mut parents = BTreeMap::<PathBuf, observation::Parent>::new();
    for e in bindings
        .programs
        .iter_mut()
        .chain(&mut bindings.tool_directories)
    {
        for p in std::mem::take(&mut e.parents) {
            if let Some(old) = parents.get(&p.path) {
                if !adoption::parent_identity(&old.fingerprint, &p.fingerprint) {
                    return Err(invalid("APT parents changed while binding"));
                }
            } else {
                parents.insert(p.path.clone(), p);
            }
        }
    }
    Ok(Sources {
        bindings,
        parents: parents.into_values().collect(),
    })
}
pub(super) fn arguments(s: &super::Scope) -> Vec<String> {
    let a = s.apt.as_ref().expect("APT scope");
    [
        "-n",
        "-T",
        "600",
        "-u",
        "root",
        "DEBIAN_FRONTEND=noninteractive",
        a.apt_get.to_str().expect("validated path"),
        "--assume-yes",
        "--no-remove",
        "--no-upgrade",
        "--no-install-recommends",
        "-o",
        "Dpkg::Use-Pty=0",
        "-o",
        "DPkg::Options::=--force-confdef",
        "-o",
        "DPkg::Options::=--force-confold",
        "install",
        &s.package,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn command(s: &super::Scope, executable: &Path) -> Command {
    let mut c = Command::new(executable);
    c.env_clear()
        .env("HOME", &s.home)
        .env("XDG_CONFIG_HOME", s.home.join(".config"))
        .env("XDG_CACHE_HOME", s.home.join(".cache"))
        .env("XDG_DATA_HOME", s.home.join(".local/share"))
        .env("XDG_STATE_HOME", s.home.join(".local/state"))
        .env(
            "PATH",
            std::env::join_paths(&s.search_dirs).expect("validated search scope"),
        )
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .current_dir("/");
    c
}
fn version(v: &str) -> bool {
    v.len() <= 256
        && v.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+~:-".contains(&b))
}
fn candidate(bytes: &[u8], package: &str) -> Result<String> {
    let fail = || invalid("APT candidate unavailable or invalid; provider output withheld");
    let text = std::str::from_utf8(bytes).map_err(|_| fail())?;
    let mut lines = text.lines();
    if lines.next() != Some(format!("{package}:").as_str()) {
        return Err(fail());
    }
    let (mut installed, mut candidate) = (None, None);
    for line in lines {
        if !line.starts_with(' ') {
            return Err(fail());
        }
        if let Some(v) = line.strip_prefix("  Installed: ")
            && (installed.replace(v).is_some() || v != "(none)")
        {
            return Err(fail());
        }
        if let Some(v) = line.strip_prefix("  Candidate: ")
            && (candidate.replace(v).is_some() || !version(v))
        {
            return Err(fail());
        }
    }
    if installed.is_none() {
        return Err(fail());
    }
    candidate.map(str::to_owned).ok_or_else(fail)
}
pub(super) fn probe(s: &super::Scope, mut validate: impl FnMut() -> Result<()>) -> Result<Probe> {
    validate()?;
    let a = s.apt.as_ref().expect("APT scope");
    let mut c = command(s, &a.apt_cache);
    c.args(["policy", &s.package]);
    let bytes = inspection::run_bounded(c, 4 * 1024 * 1024, Duration::from_secs(10))
        .map_err(|_| invalid("APT candidate query failed; output withheld"))?;
    validate()?;
    Ok(Probe {
        candidate: candidate(&bytes, &s.package)?,
        sudo_timeout_seconds: TIMEOUT_SECONDS,
    })
}

/// NEVER use the unprivileged process-group SIGKILL runner here. -T requests
/// sudo's privileged timeout. On overdue supervision send catchable TERM once
/// for documented sudo relay, then continue owning/waiting, however long it takes.
/// Reaping sudo does not prove every privileged descendant/service terminated.
pub(super) fn run(
    s: &super::Scope,
    deadline: Duration,
    mut waiting: impl FnMut(),
) -> ProcessOutcome {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use std::{io::Read, os::unix::process::CommandExt, process::Stdio, thread, time::Instant};
    let a = s.apt.as_ref().expect("APT scope");
    let mut c = command(s, &a.sudo);
    c.args(arguments(s))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = match c.spawn() {
        Ok(c) => Joined(c),
        Err(_) => return ProcessOutcome::Unavailable,
    };
    let mut pipes = [
        child.stdout.take().map(|p| Box::new(p) as Box<dyn ReadFd>),
        child.stderr.take().map(|p| Box::new(p) as Box<dyn ReadFd>),
    ];
    let mut fault = None;
    for pipe in &mut pipes {
        let ok = pipe.as_ref().is_some_and(|p| {
            fcntl_getfl(p)
                .and_then(|f| fcntl_setfl(p, f | OFlags::NONBLOCK))
                .is_ok()
        });
        if !ok {
            *pipe = None;
            fault = Some(ProcessOutcome::Pipe);
        }
    }
    let start = Instant::now();
    let mut overdue = false;
    loop {
        if !overdue && start.elapsed() >= deadline {
            overdue = true;
            waiting();
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
            }
        }
        // One bounded read per pipe per turn. Never store output or await EOF
        // from a service inheriting the pipes after the owned supervisor exits.
        for pipe in &mut pipes {
            if let Some(p) = pipe {
                let mut buf = [0u8; 8192];
                match p.read(&mut buf) {
                    Ok(0) => *pipe = None,
                    Ok(_) => (),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => {
                        *pipe = None;
                        fault = Some(ProcessOutcome::Pipe);
                    }
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return if overdue {
                    ProcessOutcome::Timeout
                } else {
                    fault.unwrap_or(if status.success() {
                        ProcessOutcome::Success
                    } else {
                        ProcessOutcome::Failed
                    })
                };
            }
            Ok(None) => (),
            Err(_) => {
                // Keep ownership even when nonblocking wait fails; do not detach.
                drop(pipes);
                let _ = child.wait();
                return ProcessOutcome::Wait;
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}
// Unwinding a metadata observer must not detach an owned root supervisor.
struct Joined(std::process::Child);
impl std::ops::Deref for Joined {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Joined {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Joined {
    fn drop(&mut self) {
        let _ = self.0.wait();
    }
}
trait ReadFd: std::io::Read + std::os::fd::AsFd {}
impl<T: std::io::Read + std::os::fd::AsFd> ReadFd for T {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_and_private_candidate_metadata() {
        for p in ["ripgrep", "libssl3", "a1", "gcc-14"] {
            assert!(package(p));
        }
        for p in [
            "a",
            "foo.bar",
            "foo+",
            "foo-",
            "foo:amd64",
            "foo=1",
            "foo/stable",
            "-foo",
            "a.*",
            "foo?",
            "Foo",
        ] {
            assert!(!package(p));
        }
        assert_eq!(candidate(b"ripgrep:\n  Installed: (none)\n  Candidate: 14.1-1~ubuntu1\n  Version table:\n     14.1-1~ubuntu1 500\n        500 https://private.invalid SECRET\n", "ripgrep").unwrap(), "14.1-1~ubuntu1");
        for text in [
            "other:\n  Installed: (none)\n  Candidate: 1\n",
            "ripgrep:\n  Installed: 1\n  Candidate: 2\n",
            "ripgrep:\n  Installed: (none)\n  Candidate: (none)\n",
            "ripgrep:\n  Installed: (none)\n  Candidate: 1\n  Candidate: 2\n",
        ] {
            let e = candidate(text.as_bytes(), "ripgrep")
                .err()
                .unwrap()
                .to_string();
            assert!(!e.contains(text));
        }
    }
}
