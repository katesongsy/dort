//! One missing canonical core formula; native build/dependency/link effects are explicit.
use super::*;
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};

pub const WARNING: &str = "Installs one missing Homebrew core formula at the provider-current version, not an exact pin. Formula/build/dependency code CAN EXECUTE; dependencies can be installed or upgraded and native links can expose commands on PATH. Skip-post-install is not a sandbox or a ban on build code. No atomicity, prefix-entry isolation, readiness or rollback guarantee. Trusted Homebrew can maintain caches and contact endpoints. A 10-minute deadline can leave partial effects.";
const CONTROLS: &[&str] = &[
    "HOMEBREW_NO_AUTO_UPDATE",
    "HOMEBREW_NO_ANALYTICS",
    "HOMEBREW_NO_ENV_HINTS",
    "HOMEBREW_NO_INSTALL_UPGRADE",
    "HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
    "HOMEBREW_NO_INSTALL_CLEANUP",
];
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub required_controls_advertised: bool,
    pub prefix: PathBuf,
    pub repository: PathBuf,
    pub cellar: PathBuf,
    pub stable_version: String,
    pub installed_versions: Vec<String>,
}
impl Probe {
    pub(super) fn valid(&self, b: &Scope) -> bool {
        self.required_controls_advertised
            && self.prefix == b.prefix
            && self.repository == b.repository
            && self.cellar == b.cellar
            && super::super::valid_observed_version(&self.stable_version)
            && self.installed_versions.len() <= 256
            && self
                .installed_versions
                .iter()
                .all(|v| super::super::valid_observed_version(v) && !v.starts_with("HEAD"))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub prefix: PathBuf,
    pub repository: PathBuf,
    pub cellar: PathBuf,
    pub policy: VersionPolicy,
    pub catalogs: Vec<npm::Catalog>,
}
/// Typical supported physical layout hint only. Never executes a candidate.
pub fn prefix_hint(executable: &Path) -> Option<PathBuf> {
    let repo = repository(executable)?;
    Some(if repo.file_name()? == "Homebrew" {
        repo.parent()?.to_owned()
    } else {
        repo
    })
}
fn repository(executable: &Path) -> Option<PathBuf> {
    (executable.file_name()? == "brew" && executable.parent()?.file_name()? == "bin")
        .then(|| {
            executable
                .parent()
                .and_then(Path::parent)
                .map(Path::to_owned)
        })
        .flatten()
}
/// Deliberately support the unqualified canonical core spelling only. Qualified
/// tap names retain the pure planner's ambiguous-identity rules, never normalized.
pub fn formula(package: &str) -> bool {
    !package.is_empty()
        && package.len() <= 128
        && package.as_bytes()[0].is_ascii_lowercase()
        && package
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+._-@".contains(&b))
        && !package.contains("..")
}
pub(super) fn valid(s: &super::Scope) -> bool {
    let Some(b) = &s.brew else {
        return false;
    };
    s.apt.is_none()
        && s.npm.is_none()
        && formula(&s.package)
        && matches!(
            b.policy,
            VersionPolicy::ProviderDefault {} | VersionPolicy::Latest {}
        )
        && s.version
            == if matches!(b.policy, VersionPolicy::Latest {}) {
                "latest"
            } else {
                "provider_default"
            }
        && inspection::absolute(&b.prefix)
        && b.prefix.ancestors().count() <= 32
        && b.prefix != Path::new("/")
        && b.cellar == b.prefix.join("Cellar")
        && repository(&s.executable).as_ref() == Some(&b.repository)
        && (b.repository == b.prefix || b.repository == b.prefix.join("Homebrew"))
        && valid_catalogs(s, b)
}
fn valid_catalogs(s: &super::Scope, b: &Scope) -> bool {
    let mut ids = BTreeSet::new();
    !b.catalogs.is_empty()
        && b.catalogs.len() <= 16
        && b.catalogs.iter().all(|c| {
            !c.tools.is_empty()
                && c.tools.len() <= 256
                && c.search_dirs.len() <= 256
                && c.preview(&s.home).is_ok()
                && c.tools.iter().all(|id| ids.insert(id.clone()))
                && (!c.tools.contains(&s.tool)
                    || (c.query == inspection::Query::BrewFormulae
                        && c.executable == s.executable
                        && c.search_dirs == s.search_dirs))
        })
        && ids == s.bound_tools
}
pub fn prepare(
    host: &HostContext,
    request: Request,
    brew: Scope,
    reports: &[&inspection::Report],
) -> Result<Preview> {
    let (manifest, scope, sources) = super::sources(host, &request, None, Some(&brew), None)?;
    super::bounded_metadata(&scope, &sources)?;
    catalogs(&scope, &brew, reports)?;
    let row = super::evaluate_catalogs(host, &manifest, &scope, reports)?;
    if row.action != planning::PackageAction::ReviewMissingPackage || !row.blockers.is_empty() {
        return Err(invalid(
            "only an observed missing canonical core formula with clear explicit prerequisites can be reviewed",
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
    b: &Scope,
    reports: &[&'a inspection::Report],
) -> Result<BTreeMap<String, &'a inspection::Report>> {
    if b.catalogs.len() != reports.len() {
        return Err(invalid("Homebrew catalog count differs"));
    }
    let mut out = BTreeMap::new();
    for (c, r) in b.catalogs.iter().zip(reports) {
        if !c.matches(&s.home, r) {
            return Err(invalid("Homebrew prerequisite catalog scope differs"));
        }
        for id in &c.tools {
            if out.insert(id.clone(), *r).is_some() {
                return Err(invalid("duplicate Homebrew catalog binding"));
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
fn writable_root(owner: u32, mode: u32, uid: u32, prefix: bool) -> bool {
    mode & 0o022 == 0
        && ((owner == uid && mode & 0o700 == 0o700)
            || (prefix && owner == 0 && mode & 0o222 == 0o200))
}
pub(super) fn sources(s: &super::Scope) -> Result<Sources> {
    let b = s.brew.as_ref().expect("brew scope");
    let mut programs = vec![npm::program(&s.executable, true)?];
    for c in &b.catalogs {
        programs.push(npm::program(&c.executable, true)?);
        if let Some(n) = &c.npm_binding {
            programs.push(npm::program(&n.npm_cli, false)?);
        }
    }
    // Existing native write roots only: no privilege, group-permission inference
    // or creation before intent. A safe root-owned prefix itself is allowed.
    let mut paths = BTreeSet::from([
        b.prefix.clone(),
        b.repository.clone(),
        b.repository.join("Library"),
    ]);
    for root in [
        "Cellar",
        "bin",
        "etc",
        "include",
        "lib",
        "opt",
        "sbin",
        "share",
        "var",
        "var/homebrew",
    ] {
        paths.insert(b.prefix.join(root));
    }
    let uid = rustix::process::geteuid().as_raw();
    let directories = paths.into_iter().map(|path| {
        let e = observation::inspect(&path, false)?;
        let safe = e.kind == Kind::Directory && e.fingerprint.as_ref().is_some_and(|f| {
            writable_root(f.uid, f.mode, uid, path == b.prefix)
        });
        if !safe || e.parents.iter().any(|p| {
            // The canonical disposable HOME boundary is already application-owned;
            // outside it, root/user-owned non-writable ancestors are required.
            (!b.prefix.starts_with(&s.home) || p.path.starts_with(&s.home))
                && (p.fingerprint.mode & 0o022 != 0 || (p.fingerprint.uid != uid && p.fingerprint.uid != 0))
        }) { return Err(invalid("Homebrew requires existing physical owned native write roots and safe prefix parents; no sudo or group inference")); }
        Ok(e)
    }).collect::<Result<Vec<_>>>()?;
    let mut lookup = BTreeSet::new();
    lookup.extend(s.tool_search_dirs.iter().cloned());
    lookup.extend(s.search_dirs.iter().cloned());
    for c in &b.catalogs {
        lookup.extend(c.search_dirs.iter().cloned());
    }
    let tool_directories = lookup
        .iter()
        .map(|p| {
            let e = observation::inspect(p, false)?;
            if e.kind != Kind::Directory {
                return Err(invalid("tool search directory unavailable or unsafe"));
            }
            Ok(e)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut bindings = npm::Sources {
        programs,
        directories,
        tool_directories,
    };
    // Schema 3 deduplicates repeated ancestor chains to keep native-layout
    // bindings inside the existing durable record bound. Every identity is kept.
    let mut parents = BTreeMap::<PathBuf, observation::Parent>::new();
    for e in bindings
        .programs
        .iter_mut()
        .chain(&mut bindings.directories)
        .chain(&mut bindings.tool_directories)
    {
        for p in std::mem::take(&mut e.parents) {
            if let Some(old) = parents.get(&p.path) {
                if !adoption::parent_identity(&old.fingerprint, &p.fingerprint) {
                    return Err(invalid("Homebrew parents changed while binding"));
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
pub(super) fn arguments(package: &str) -> Vec<String> {
    [
        "install".into(),
        "--formula".into(),
        "--no-ask".into(),
        "--skip-post-install".into(),
        format!("homebrew/core/{package}"),
    ]
    .into()
}
pub(super) fn probe_arguments(package: &str) -> Vec<Vec<String>> {
    vec![
        vec!["help".into(), "install".into()],
        vec!["--prefix".into()],
        vec!["--repository".into()],
        vec!["--cellar".into()],
        vec![
            "info".into(),
            "--json=v2".into(),
            "--formula".into(),
            format!("homebrew/core/{package}"),
        ],
    ]
}
pub(super) fn environment(command: &mut Command) {
    for key in CONTROLS {
        command.env(key, "1");
    }
}
fn command(executable: &Path, home: &Path, search_dirs: &[PathBuf], args: &[String]) -> Command {
    let mut c = Command::new(executable);
    c.env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env(
            "PATH",
            std::env::join_paths(search_dirs).expect("validated scope"),
        )
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .current_dir("/")
        .args(args);
    environment(&mut c);
    c
}
fn query(s: &super::Scope, args: &[String]) -> Result<Vec<u8>> {
    inspection::run_bounded(
        command(&s.executable, &s.home, &s.search_dirs, args),
        4 * 1024 * 1024,
        Duration::from_secs(10),
    )
    .map_err(|_| {
        invalid("Homebrew probe unavailable, failed or exceeded bounds; raw output withheld")
    })
}
pub(super) fn closure_prefix(
    executable: &Path,
    home: &Path,
    search_dirs: &[PathBuf],
    expected: &Path,
) -> Result<()> {
    let bytes = inspection::run_bounded(
        command(executable, home, search_dirs, &["--prefix".into()]),
        8192,
        Duration::from_secs(10),
    )
    .map_err(|_| invalid("Homebrew closure prefix probe unavailable; contents withheld"))?;
    if path(&bytes)? != expected {
        return Err(invalid(
            "Homebrew closure prefix differs from approved scope",
        ));
    }
    Ok(())
}
fn path(bytes: &[u8]) -> Result<PathBuf> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("invalid Homebrew layout metadata"))?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.len() > 4096 || text.bytes().any(|b| b < 32 || b == 127) {
        return Err(invalid("invalid Homebrew layout metadata"));
    }
    let p = PathBuf::from(text);
    // Never inspect a provider-supplied path before matching the explicit scope:
    // an observation error could otherwise copy unapproved raw output into errors.
    // The caller brackets comparison with checks of the expected physical paths.
    if !inspection::absolute(&p) {
        return Err(invalid("invalid Homebrew layout path"));
    }
    Ok(p)
}
/// Execute only after lock/journal/attempt/source guards. Every separately owned
/// bounded query is bracketed by independently reconstructed source checks.
pub(super) fn probe(s: &super::Scope, mut check: impl FnMut() -> Result<()>) -> Result<Probe> {
    let b = s.brew.as_ref().expect("brew scope");
    let args = probe_arguments(&s.package);
    check()?;
    let help = query(s, &args[0])?;
    check()?;
    let help = std::str::from_utf8(&help)
        .map_err(|_| invalid("Homebrew install capabilities unavailable"))?;
    for flag in [
        "--formula",
        "--no-ask",
        "--skip-post-install",
        "HOMEBREW_NO_INSTALL_UPGRADE",
        "HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
        "HOMEBREW_NO_INSTALL_CLEANUP",
    ] {
        if !help
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .any(|word| word == flag)
        {
            return Err(invalid(
                "Homebrew does not advertise all required install controls; no weaker fallback",
            ));
        }
    }
    for (args, expected) in args[1..4].iter().zip([&b.prefix, &b.repository, &b.cellar]) {
        check()?;
        let observed = path(&query(s, args)?)?;
        check()?;
        if &observed != expected {
            return Err(invalid(
                "Homebrew native layout differs from selected physical scope",
            ));
        }
    }
    check()?;
    let (stable_version, installed_versions) = formula_info(s)?;
    check()?;
    Ok(Probe {
        required_controls_advertised: true,
        prefix: b.prefix.clone(),
        repository: b.repository.clone(),
        cellar: b.cellar.clone(),
        stable_version,
        installed_versions,
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn root_owned_prefix_never_grants_permission_to_native_write_roots() {
        use super::writable_root;
        assert!(writable_root(0, 0o755, 501, true));
        assert!(!writable_root(0, 0o755, 501, false));
        assert!(writable_root(501, 0o755, 501, false));
        for mode in [0o775, 0o777, 0o500, 0o555] {
            assert!(!writable_root(501, mode, 501, false));
        }
        assert!(!writable_root(502, 0o755, 501, false));
    }
}
#[derive(Deserialize)]
struct Info {
    formulae: Vec<Formula>,
}
#[derive(Deserialize)]
struct Formula {
    name: String,
    full_name: String,
    tap: String,
    disabled: bool,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    oldnames: Vec<String>,
    installed: Vec<Installed>,
    versions: Versions,
}
#[derive(Deserialize)]
struct Versions {
    stable: Option<String>,
    head: Option<String>,
}
#[derive(Deserialize)]
struct Installed {
    version: String,
}
pub(super) fn formula_info(s: &super::Scope) -> Result<(String, Vec<String>)> {
    let bytes = query(s, &probe_arguments(&s.package)[4])?;
    let mut info: Info = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("invalid Homebrew formula metadata; raw output withheld"))?;
    if info.formulae.len() != 1 {
        return Err(invalid("ambiguous Homebrew formula metadata"));
    }
    let f = info.formulae.remove(0);
    if f.name != s.package
        || (f.full_name != s.package && f.full_name != format!("homebrew/core/{}", s.package))
        || f.tap != "homebrew/core"
        || f.disabled
        || !f.aliases.is_empty()
        || !f.oldnames.is_empty()
        || f.versions
            .stable
            .as_ref()
            .is_none_or(|v| !super::super::valid_observed_version(v))
        || f.installed.len() > 256
    {
        return Err(invalid(
            "Homebrew target is not an available unambiguous canonical core formula",
        ));
    }
    // HEAD availability is not permission to install HEAD; only the fixed stable
    // install protocol is supported. Installed HEAD claims cannot prove presence.
    let mut versions = Vec::new();
    for i in f.installed {
        if !super::super::valid_observed_version(&i.version)
            || i.version.starts_with("HEAD")
            || f.versions.head.as_ref() == Some(&i.version)
        {
            return Err(invalid("unsupported Homebrew installed version metadata"));
        }
        versions.push(i.version);
    }
    versions.sort();
    versions.dedup();
    Ok((f.versions.stable.expect("validated stable"), versions))
}
