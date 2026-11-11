//! Explicit cross-catalog npm scope. No snapshot inference or candidate execution.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const WARNING: &str = "Installs one exact npm package with lifecycle scripts disabled. Native dependencies and bin/man links can change the selected prefix and expose commands immediately on PATH. Trusted npm may read configuration and contact dependency endpoints. Not a sandbox, atomic operation, readiness guarantee or rollback. A 10-minute deadline may leave partial effects.";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub query: inspection::Query,
    pub executable: PathBuf,
    pub search_dirs: Vec<PathBuf>,
    pub npm_binding: Option<inspection::NpmBinding>,
    pub tools: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub binding: inspection::NpmBinding,
    pub catalogs: Vec<Catalog>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sources {
    pub programs: Vec<Entry>,
    pub directories: Vec<Entry>,
    pub tool_directories: Vec<Entry>,
}
pub(super) fn valid(scope: &super::Scope) -> bool {
    let Some(npm) = &scope.npm else {
        return false;
    };
    if npm.catalogs.is_empty()
        || npm.catalogs.len() > 16
        || npm.binding.node != scope.executable
        || npm.binding.prefix == scope.home
        || npm.binding.prefix.ancestors().count() > 32
        || !npm.binding.prefix.starts_with(&scope.home)
        || !scope.bound_tools.contains(&npm.binding.runtime_tool)
    {
        return false;
    }
    let mut ids = BTreeSet::new();
    for c in &npm.catalogs {
        if c.tools.is_empty()
            || c.tools.len() > 256
            || c.search_dirs.len() > 256
            || c.preview(&scope.home).is_err()
            || c.tools.iter().any(|id| !ids.insert(id.clone()))
        {
            return false;
        }
        if c.tools.contains(&scope.tool)
            && (c.npm_binding.as_ref() != Some(&npm.binding) || c.search_dirs != scope.search_dirs)
        {
            return false;
        }
    }
    ids == scope.bound_tools
}
impl Catalog {
    pub(super) fn preview(&self, home: &Path) -> Result<inspection::Preview> {
        let result = if self.query == inspection::Query::NpmGlobals {
            let binding = self
                .npm_binding
                .clone()
                .ok_or_else(|| invalid("npm catalog binding missing"))?;
            if binding.node != self.executable {
                return Err(invalid("npm catalog executable differs from selected Node"));
            }
            inspection::Preview::npm_probe(binding, home.to_owned(), self.search_dirs.clone())
        } else {
            if self.npm_binding.is_some() {
                return Err(invalid("non-npm catalog cannot have an npm binding"));
            }
            inspection::Preview::new(
                self.query,
                self.executable.clone(),
                home.to_owned(),
                self.search_dirs.clone(),
            )
        };
        result.map_err(|_| invalid("invalid explicit installation catalog scope"))
    }
    pub(super) fn matches(&self, home: &Path, r: &inspection::Report) -> bool {
        let d = &r.description;
        d.query == self.query
            && d.executable == self.executable
            && d.home == home
            && d.trusted_search_path == self.search_dirs
            && d.npm_binding == self.npm_binding
    }
}
/// Process-free reconstruction from explicit catalog bindings. Serialized command
/// descriptions are not accepted. The application must validate snapshot provenance.
pub fn prepare(
    host: &HostContext,
    request: Request,
    npm: Scope,
    reports: &[&inspection::Report],
) -> Result<Preview> {
    let (manifest, scope, sources) = super::sources(host, &request, Some(&npm), None)?;
    // The durable intent budget applies before creation. Newly pinned parents
    // expand in-memory fingerprints, not the immutable persisted intent.
    super::bounded_metadata(&scope, &sources)?;
    if !valid(&scope) {
        return Err(invalid("invalid explicit npm installation scope"));
    }
    validate_catalogs(&manifest, &scope, &npm, reports)?;
    let row = super::evaluate_catalogs(host, &manifest, &scope, reports)?;
    if row.action != planning::PackageAction::ReviewMissingPackage || !row.blockers.is_empty() {
        return Err(invalid(
            "only an observed missing exact npm package with satisfied explicit runtime and prerequisites can be reviewed",
        ));
    }
    Ok(Preview {
        request,
        scope,
        sources,
    })
}
fn validate_catalogs(
    manifest: &Manifest,
    scope: &super::Scope,
    npm: &Scope,
    reports: &[&inspection::Report],
) -> Result<()> {
    if npm.catalogs.is_empty() || npm.catalogs.len() > 16 || reports.len() != npm.catalogs.len() {
        return Err(invalid("installation requires bounded explicit catalogs"));
    }
    let mut bound = BTreeMap::new();
    for (catalog, report) in npm.catalogs.iter().zip(reports) {
        catalog.preview(&scope.home)?;
        if catalog.tools.is_empty() || !catalog.matches(&scope.home, report) {
            return Err(invalid("selected installation catalog scope differs"));
        }
        for id in &catalog.tools {
            if !scope.bound_tools.contains(id) || bound.insert(id.clone(), catalog).is_some() {
                return Err(invalid(
                    "duplicate or inactive installation catalog binding",
                ));
            }
        }
    }
    if bound.len() != scope.bound_tools.len()
        || !bound.get(&scope.tool).is_some_and(|c| {
            c.query == inspection::Query::NpmGlobals && c.npm_binding.as_ref() == Some(&npm.binding)
        })
        || !bound.contains_key(&npm.binding.runtime_tool)
        || !manifest
            .tools
            .iter()
            .any(|t| t.id == npm.binding.runtime_tool)
    {
        return Err(invalid(
            "explicit target and runtime catalog bindings required",
        ));
    }
    Ok(())
}
pub(super) fn catalogs<'a>(
    scope: &super::Scope,
    npm: &Scope,
    reports: &[&'a inspection::Report],
) -> Result<BTreeMap<String, &'a inspection::Report>> {
    if npm.catalogs.len() != reports.len() {
        return Err(invalid("installation catalog count differs"));
    }
    let mut catalogs = BTreeMap::new();
    for (c, r) in npm.catalogs.iter().zip(reports) {
        if !c.matches(&scope.home, r) {
            return Err(invalid("installation catalog scope differs"));
        }
        for id in &c.tools {
            if catalogs.insert(id.clone(), *r).is_some() {
                return Err(invalid("duplicate installation catalog binding"));
            }
        }
    }
    Ok(catalogs)
}
pub(super) fn arguments(scope: &super::Scope) -> Vec<String> {
    let binding = &scope.npm.as_ref().expect("npm scope").binding;
    [
        binding.npm_cli.to_str().expect("validated path").to_owned(),
        "install".into(),
        "--global".into(),
        "--prefix".into(),
        binding.prefix.to_str().expect("validated path").into(),
        "--ignore-scripts".into(),
        "--audit=false".into(),
        "--fund=false".into(),
        "--update-notifier=false".into(),
        "--logs-max=0".into(),
        "--loglevel=silent".into(),
        "--registry=https://registry.npmjs.org/".into(),
        "--userconfig=/dev/null".into(),
        "--globalconfig=/dev/null".into(),
        format!("{}@{}", scope.package, scope.version),
    ]
    .into()
}
pub(super) fn program(path: &Path, executable: bool) -> Result<Entry> {
    let e = observation::inspect(path, false)?;
    if e.kind != Kind::Regular
        || e.fingerprint
            .as_ref()
            .is_none_or(|f| f.links != 1 || (executable && f.mode & 0o111 == 0))
    {
        return Err(invalid(
            "installation requires a physical regular trusted program",
        ));
    }
    Ok(e)
}
pub(super) fn sources(scope: &super::Scope) -> Result<Sources> {
    let npm = scope.npm.as_ref().expect("npm scope");
    let b = &npm.binding;
    if !inspection::absolute(&b.prefix)
        || b.prefix == scope.home
        || !b.prefix.starts_with(&scope.home)
    {
        return Err(invalid(
            "npm installation prefix must be strictly inside HOME",
        ));
    }
    for root in ["lib/node_modules", "bin", "share/man"] {
        let root = b.prefix.join(root);
        if b.node.starts_with(&root) || b.npm_cli.starts_with(&root) {
            return Err(invalid(
                "npm write roots must not contain selected Node or npm CLI",
            ));
        }
    }
    let mut programs = vec![program(&b.node, true)?, program(&b.npm_cli, false)?];
    for c in &npm.catalogs {
        c.preview(&scope.home)?;
        programs.push(program(&c.executable, true)?);
        if let Some(b) = &c.npm_binding {
            programs.push(program(&b.npm_cli, false)?);
        }
    }
    let mut paths = BTreeSet::new();
    let mut roots = vec![
        b.prefix.clone(),
        b.prefix.join("lib/node_modules"),
        b.prefix.join("bin"),
        b.prefix.join("share/man"),
    ];
    if let Some((scope, _)) = scope
        .package
        .strip_prefix('@')
        .and_then(|p| p.split_once('/'))
    {
        roots.push(b.prefix.join("lib/node_modules").join(format!("@{scope}")));
    }
    for path in roots {
        for p in path.ancestors().take_while(|p| p.starts_with(&scope.home)) {
            paths.insert(p.to_owned());
        }
    }
    let mut directories = Vec::new();
    for path in paths {
        let e = observation::inspect(&path, false)?;
        if !matches!(e.kind, Kind::Missing | Kind::Directory)
            || e.fingerprint.as_ref().is_some_and(|f| {
                f.uid != rustix::process::geteuid().as_raw() || f.mode & 0o022 != 0
            })
            || (path == scope.home && e.kind != Kind::Directory)
        {
            return Err(invalid(
                "npm prefix parents must be physical owned directories without group/world write access",
            ));
        }
        directories.push(e);
    }
    let tool_directories = scope
        .tool_search_dirs
        .iter()
        .map(|p| {
            let e = observation::inspect(p, false)?;
            if e.kind != Kind::Directory {
                return Err(invalid("tool search directory unavailable or unsafe"));
            }
            Ok(e)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Sources {
        programs,
        directories,
        tool_directories,
    })
}
pub(super) fn same_directory(a: &Entry, b: &Entry) -> bool {
    a.path == b.path
        && a.kind == b.kind
        && match (&a.fingerprint, &b.fingerprint) {
            (Some(a), Some(b)) => adoption::parent_identity(a, b),
            (None, None) => true,
            _ => false,
        }
        && a.parents.len() == b.parents.len()
        && a.parents.iter().zip(&b.parents).all(|(a, b)| {
            a.path == b.path && adoption::parent_identity(&a.fingerprint, &b.fingerprint)
        })
}
pub(super) fn same(a: &Sources, b: &Sources) -> bool {
    a.tool_directories.len() == b.tool_directories.len()
        && a.tool_directories
            .iter()
            .zip(&b.tool_directories)
            .all(|(a, b)| same_directory(a, b))
        && a.programs.len() == b.programs.len()
        && a.programs
            .iter()
            .zip(&b.programs)
            .all(|(a, b)| same_source(a, b))
        && a.directories.len() == b.directories.len()
        && a.directories
            .iter()
            .zip(&b.directories)
            .all(|(a, b)| same_directory(a, b))
}
/// Called only after immutable intent and final guards. Create only approved
/// mutation parents, descriptor-relative, then pin the resulting identities.
pub(super) fn create_directories(scope: &super::Scope, old: &Sources) -> Result<Sources> {
    if !same(old, &sources(scope)?) {
        return Err(invalid("npm prefix changed before creation"));
    }
    let mut created = Vec::new();
    for e in &old.directories {
        if e.kind != Kind::Missing {
            continue;
        }
        let handle = observation::parent_handle(&e.path)?;
        if handle.missing {
            return Err(invalid("npm prefix creation parent unavailable"));
        }
        rustix::fs::mkdirat(
            &handle.directory,
            &handle.name,
            rustix::fs::Mode::from_raw_mode(0o755),
        )
        .map_err(|_| invalid("npm prefix creation failed; inspect pending attempt"))?;
        handle
            .directory
            .sync_all()
            .map_err(|_| invalid("npm prefix parent sync failed"))?;
        let made = observation::parent_handle(&e.path.join(".dort-parent-check"))?;
        made.directory
            .sync_all()
            .map_err(|_| invalid("npm prefix directory sync failed"))?;
        // Pin each new directory immediately, not only at the end of creation.
        let entry = observation::inspect(&e.path, false)?;
        if entry.kind != Kind::Directory {
            return Err(invalid("new npm directory changed"));
        }
        created.push(entry);
    }
    let fresh = sources(scope)?;
    if old.tool_directories.len() != fresh.tool_directories.len()
        || old
            .tool_directories
            .iter()
            .zip(&fresh.tool_directories)
            .any(|(a, b)| !same_directory(a, b))
        || created.iter().any(|a| {
            fresh
                .directories
                .iter()
                .find(|b| b.path == a.path)
                .is_none_or(|b| !same_directory(a, b))
        })
        || old
            .programs
            .iter()
            .zip(&fresh.programs)
            .any(|(a, b)| !same_source(a, b))
        || old
            .directories
            .iter()
            .zip(&fresh.directories)
            .any(|(a, b)| a.kind != Kind::Missing && !same_directory(a, b))
    {
        return Err(invalid(
            "npm prefix changed while creating approved parents",
        ));
    }
    Ok(fresh)
}
