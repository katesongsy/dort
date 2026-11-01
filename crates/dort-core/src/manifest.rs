use crate::{
    Error, Result,
    context::HostContext,
    model::{FileMode, InstallSpec, ManagedFile, Provider, Tool},
    platform::Platform,
    profiles::{Profiles, selected},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    #[serde(default)]
    pub profiles: Profiles,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub files: Vec<ManagedFile>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Structural validation does not execute providers or require source files to exist.
    pub fn validate(&self) -> Result<()> {
        let mut issues = vec![];
        if self.schema != 1 {
            issues.push(format!("unsupported manifest schema: {}", self.schema));
        }
        let known = self.profiles.known();
        check_names("available profiles", &self.profiles.available, &mut issues);
        for profile in &self.profiles.available {
            if !identifier(profile) {
                issues.push(format!("invalid profile identifier: {profile}"));
            }
        }
        check_profiles(
            "default profiles",
            &self.profiles.default,
            &known,
            false,
            &mut issues,
        );
        for (platform, profiles) in &self.profiles.platform {
            check_profiles(
                &format!("{platform:?} profiles"),
                profiles,
                &known,
                false,
                &mut issues,
            );
        }
        let tool_ids: BTreeSet<_> = self.tools.iter().map(|t| t.id.as_str()).collect();
        check_names(
            "tool IDs",
            &self.tools.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
            &mut issues,
        );
        check_names(
            "file IDs",
            &self.files.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
            &mut issues,
        );
        for tool in &self.tools {
            if !identifier(&tool.id) {
                issues.push(format!("invalid tool ID: {}", tool.id));
            }
            if !identifier(&tool.binary) {
                issues.push(format!(
                    "{}: binary must be a bare executable name",
                    tool.id
                ));
            }
            check_profiles(&tool.id, &tool.profiles, &known, true, &mut issues);
            check_references(&tool.id, &tool.requires, &tool_ids, &mut issues);
            if tool.install.is_empty() {
                issues.push(format!(
                    "{}: at least one installation definition is required",
                    tool.id
                ));
            }
            for (platform, install) in &tool.install {
                if let Err(message) = validate_install(*platform, install) {
                    issues.push(format!("{} ({platform:?}): {message}", tool.id));
                }
                if let Some(runtime) = &install.runtime_tool {
                    check_references(
                        &tool.id,
                        std::slice::from_ref(runtime),
                        &tool_ids,
                        &mut issues,
                    );
                }
            }
        }
        let mut targets: Vec<(&str, &Path)> = vec![];
        for file in &self.files {
            if !identifier(&file.id) {
                issues.push(format!("invalid file ID: {}", file.id));
            }
            check_profiles(&file.id, &file.profiles, &known, true, &mut issues);
            check_references(&file.id, &file.requires, &tool_ids, &mut issues);
            for source in std::iter::once(&file.source).chain(file.overrides.values()) {
                if !valid_source(source) {
                    issues.push(format!("{}: source must be repository-relative without traversal or reserved paths", file.id));
                }
            }
            if !valid_target(&file.target) {
                issues.push(format!(
                    "{}: target must be an absolute or ~/ path without traversal",
                    file.id
                ));
            }
            for (other, target) in &targets {
                if paths_overlap(target, &file.target) {
                    issues.push(format!("conflicting destinations: {other} and {}", file.id));
                }
            }
            targets.push((&file.id, &file.target));
            if let Some(permissions) = &file.permissions {
                let valid = permissions.len() == 4
                    && permissions.starts_with('0')
                    && permissions.bytes().all(|b| (b'0'..=b'7').contains(&b));
                if !valid {
                    issues.push(format!(
                        "{}: permissions must be four octal digits (0000–0777)",
                        file.id
                    ));
                }
                if file.mode == FileMode::Link {
                    issues.push(format!(
                        "{}: permissions are not supported for link mode",
                        file.id
                    ));
                }
                if file.mode == FileMode::Secret && permissions != "0600" {
                    issues.push(format!("{}: secret permissions must be 0600", file.id));
                }
            }
        }
        // Runtime dependencies may differ by platform; do not combine mutually exclusive edges.
        for platform in [Platform::Macos, Platform::Ubuntu] {
            let all: BTreeSet<_> = tool_ids.iter().map(|s| (*s).to_owned()).collect();
            if let Err(e) = self.order(platform, &all) {
                issues.push(e.to_string());
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(issues))
        }
    }

    pub fn active_profiles(
        &self,
        host: &HostContext,
        local: &[String],
    ) -> Result<BTreeSet<String>> {
        self.profiles.resolve(host.platform, local)
    }

    /// Checks effective-platform definitions, inactive dependencies and expanded target collisions.
    /// It does not claim that providers, binaries or sources have been inspected.
    pub fn validate_for_host(
        &self,
        host: &HostContext,
        local: &[String],
        repository: Option<&Path>,
    ) -> Result<BTreeSet<String>> {
        self.validate()?;
        let active = self.active_profiles(host, local)?;
        let selected_tools: BTreeSet<_> = self
            .tools
            .iter()
            .filter(|t| selected(&t.profiles, &active))
            .map(|t| t.id.clone())
            .collect();
        self.dependency_order(host.platform, &active)?;
        let mut targets: Vec<(&str, std::path::PathBuf)> = vec![];
        for file in self.files.iter().filter(|f| selected(&f.profiles, &active)) {
            for dependency in &file.requires {
                if !selected_tools.contains(dependency) {
                    return Err(Error::Invalid(format!(
                        "{}: required tool {dependency} is inactive",
                        file.id
                    )));
                }
            }
            let path = host.target_path(&file.target, repository)?;
            if let Some((other, _)) = targets.iter().find(|(_, p)| paths_overlap(p, &path)) {
                return Err(Error::Invalid(format!(
                    "conflicting expanded destinations: {other} and {}",
                    file.id
                )));
            }
            targets.push((&file.id, path));
        }
        Ok(active)
    }

    pub fn dependency_order(
        &self,
        platform: Platform,
        active: &BTreeSet<String>,
    ) -> Result<Vec<String>> {
        self.validate()?;
        if !active.contains("base") || !active.is_subset(&self.profiles.known()) {
            return Err(Error::Invalid(
                "dependency ordering requires resolved, known profiles including base".into(),
            ));
        }
        let ids: BTreeSet<_> = self
            .tools
            .iter()
            .filter(|t| selected(&t.profiles, active))
            .map(|t| t.id.clone())
            .collect();
        for tool in self.tools.iter().filter(|t| ids.contains(&t.id)) {
            if !tool.install.contains_key(&platform) {
                return Err(Error::Invalid(format!(
                    "{}: no installation definition for {platform:?}",
                    tool.id
                )));
            }
            for dependency in self.dependencies(tool, platform) {
                if !ids.contains(dependency) {
                    return Err(Error::Invalid(format!(
                        "{}: required tool {dependency} is inactive",
                        tool.id
                    )));
                }
            }
        }
        self.order(platform, &ids)
    }

    fn dependencies<'a>(&'a self, tool: &'a Tool, platform: Platform) -> BTreeSet<&'a str> {
        tool.requires
            .iter()
            .map(String::as_str)
            .chain(
                tool.install
                    .get(&platform)
                    .and_then(|i| i.runtime_tool.as_deref()),
            )
            .collect()
    }

    fn order(&self, platform: Platform, selected_ids: &BTreeSet<String>) -> Result<Vec<String>> {
        let mut remaining: BTreeMap<_, BTreeSet<_>> = self
            .tools
            .iter()
            .filter(|t| selected_ids.contains(&t.id))
            .map(|t| {
                (
                    t.id.clone(),
                    self.dependencies(t, platform)
                        .into_iter()
                        .filter(|d| selected_ids.contains(*d))
                        .map(str::to_owned)
                        .collect(),
                )
            })
            .collect();
        let mut ordered = vec![];
        while !remaining.is_empty() {
            let next = remaining
                .iter()
                .find(|(_, deps)| deps.is_empty())
                .map(|(id, _)| id.clone())
                .ok_or_else(|| Error::Invalid(format!("tool dependency cycle on {platform:?}")))?;
            remaining.remove(&next);
            for deps in remaining.values_mut() {
                deps.remove(&next);
            }
            ordered.push(next);
        }
        Ok(ordered)
    }
}

fn check_names(label: &str, values: &[String], issues: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            issues.push(format!("duplicate {label}: {value}"));
        }
    }
}

fn check_profiles(
    label: &str,
    values: &[String],
    known: &BTreeSet<String>,
    nonempty: bool,
    issues: &mut Vec<String>,
) {
    check_names(&format!("profiles in {label}"), values, issues);
    if nonempty && values.is_empty() {
        issues.push(format!("{label}: profiles cannot be empty; omit for base"));
    }
    for value in values {
        if !known.contains(value) {
            issues.push(format!("{label}: unknown profile {value}"));
        }
    }
}

fn check_references(
    label: &str,
    values: &[String],
    known: &BTreeSet<&str>,
    issues: &mut Vec<String>,
) {
    check_names(&format!("dependencies in {label}"), values, issues);
    for value in values {
        if !known.contains(value.as_str()) {
            issues.push(format!("{label}: unknown required tool {value}"));
        }
    }
}

fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.as_bytes()[0].is_ascii_alphanumeric()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn safe_components(path: &Path) -> bool {
    path.to_str().is_some_and(|s| {
        !s.is_empty()
            && !s.chars().any(char::is_control)
            && !s.split('/').any(|c| c == ".." || c == ".")
    })
}

fn valid_source(path: &Path) -> bool {
    safe_components(path)
        && !path.is_absolute()
        && !path.starts_with("~")
        && !path.components().any(|c| c.as_os_str() == ".git")
        && path != Path::new("dort.toml")
        && path != Path::new("recipients.toml")
}

fn valid_target(path: &Path) -> bool {
    safe_components(path)
        && (path.is_absolute() || path.starts_with("~"))
        && path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
            > usize::from(path.starts_with("~"))
}

fn paths_overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn validate_install(platform: Platform, spec: &InstallSpec) -> std::result::Result<(), String> {
    if matches!(
        (platform, spec.provider),
        (Platform::Macos, Provider::Apt) | (Platform::Ubuntu, Provider::Brew)
    ) {
        return Err("provider is unsupported on this platform".into());
    }
    let plain = |s: &str| identifier(s) && !s.contains("..");
    let valid_package = match spec.provider {
        Provider::Brew => spec.package.split('/').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-+@".contains(&b))
                && !p.starts_with('-')
                && !p.contains("..")
        }),
        Provider::Apt => {
            spec.package.split(':').count() <= 2
                && spec.package.split(':').all(|p| {
                    !p.is_empty()
                        && p.as_bytes()[0].is_ascii_lowercase()
                        && p.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b)
                        })
                })
        }
        Provider::Mise => plain(&spec.package),
        Provider::Npm => {
            if let Some(scoped) = spec.package.strip_prefix('@') {
                scoped
                    .split_once('/')
                    .is_some_and(|(scope, name)| plain(scope) && plain(name))
            } else {
                plain(&spec.package)
            }
        }
    };
    if !valid_package {
        return Err("invalid provider package identifier".into());
    }
    if spec.provider == Provider::Npm && spec.runtime_tool.is_none() {
        return Err("npm requires an explicit runtime_tool".into());
    }
    if spec.provider != Provider::Npm && spec.runtime_tool.is_some() {
        return Err("runtime_tool is only valid for npm".into());
    }
    if let Some(version) = spec.version.as_deref() {
        let supported = match spec.provider {
            Provider::Brew | Provider::Apt => version == "latest",
            Provider::Npm => version == "latest" || semver::Version::parse(version).is_ok(),
            Provider::Mise => {
                version == "latest"
                    || semver::Version::parse(version).is_ok()
                    || (version.split('.').count() <= 2
                        && version
                            .split('.')
                            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())))
            }
        };
        if !supported {
            return Err("provider cannot enforce this version policy".into());
        }
    }
    Ok(())
}
