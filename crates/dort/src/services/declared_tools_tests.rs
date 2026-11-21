use super::declared_tools::{self, Input, Status};
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

pub(crate) struct Fixture {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) host: HostContext,
    pub(crate) repo: PathBuf,
    pub(crate) bin: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("bin");
        for path in [&home, &repo, &bin] {
            fs::create_dir(path).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='a'\nbinary='a'\n[tools.install]\nprovider='brew'\npackage='a'\n[[tools]]\nid='b'\nbinary='b'\n[tools.install]\nprovider='brew'\npackage='b'\n").unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        drop(lock);
        let f = Self {
            _temp: temp,
            host,
            repo,
            bin,
        };
        f.program("brew", r#"
case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\n' "$HOME/prefix";;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --installed --formula')
printf Q >> "$HOME/calls"
printf '{"formulae":['
if [ -f "$HOME/a" ]; then printf '{"full_name":"a","installed":[{"version":"1.2.3"}]},'; fi
if [ -f "$HOME/b" ]; then printf '{"full_name":"b","installed":[{"version":"1.2.3"}]},'; fi
printf '{"full_name":"unrelated","installed":[{"version":"1.2.3"}]}]}';;
'info --json=v2 --formula homebrew/core/'*)
for arg in "$@"; do :; done
name=${arg##*/}
printf '{"formulae":[{"name":"%s","full_name":"%s","tap":"homebrew/core","disabled":false,"versions":{"stable":"1.2.3"},"installed":[' "$name" "$name"
if [ -f "$HOME/$name" ]; then printf '{"version":"1.2.3"}'; fi
printf ']}]}';;
'install --formula --no-ask --skip-post-install homebrew/core/a') printf A >> "$HOME/calls"; if [ -f "$HOME/fail-a" ]; then exit 9; fi; printf installed > "$HOME/a";;
'install --formula --no-ask --skip-post-install homebrew/core/b') printf B >> "$HOME/calls"; printf installed > "$HOME/b";;
*) exit 90;;
esac
"#);
        f
    }
    pub(crate) fn program(&self, name: &str, body: &str) {
        let path = if name == "brew" {
            let prefix = self.host.home.join("prefix");
            for dir in [
                "Homebrew/bin",
                "Homebrew/Library",
                "Cellar",
                "bin",
                "etc",
                "include",
                "lib",
                "opt",
                "sbin",
                "share",
                "var/homebrew",
            ] {
                fs::create_dir_all(prefix.join(dir)).unwrap();
            }
            let path = prefix.join("Homebrew/bin/brew");
            if !self.bin.join(name).exists() {
                std::os::unix::fs::symlink(&path, self.bin.join(name)).unwrap();
            }
            path
        } else {
            self.bin.join(name)
        };
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    pub(crate) fn input(&self) -> Input {
        Input {
            manifest: self.repo.join("dort.toml"),
            profiles: vec![],
            detection: super::inspection_defaults::Inputs {
                search_dirs: vec![self.bin.clone()],
                overrides: vec![],
            },
        }
    }
    pub(crate) fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
}
#[test]
fn multiple_npm_runtimes_keep_independent_settings_and_automatic_associations() {
    use super::tools::Request;
    use dort_core::tools::inspection::Query;
    let f = Fixture::new();
    let path = f.repo.join("dort.toml");
    let mut manifest = String::from("# PRIVATE_MANIFEST\nschema=1\n");
    let mut overrides = Vec::new();
    for n in 1..=2 {
        manifest.push_str(&format!("[[tools]]\nid='runtime{n}'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n[[tools]]\nid='cli{n}'\nbinary='cli{n}'\n[tools.install]\nprovider='npm'\npackage='cli{n}'\nversion='1.2.3'\nruntime_tool='runtime{n}'\n"));
        let prefix = f.host.home.join(format!("runtime{n}"));
        fs::create_dir_all(prefix.join("bin")).unwrap();
        let node = prefix.join("bin/node");
        fs::write(&node, format!("#!/bin/sh\nprintf {n} >> \"$HOME/calls\"\nprintf '{{\"dependencies\":{{\"cli{n}\":{{\"version\":\"1.2.3\"}}}}}}'\n")).unwrap();
        fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
        let cli = prefix.join("npm-cli.js");
        fs::write(&cli, "PRIVATE_CLI_CONTENT").unwrap();
        overrides.push(Request {
            query: Query::NpmGlobals,
            executable: node,
            search_dirs: vec![f.bin.clone()],
            manifest: Some(path.clone()),
            runtime_tool: Some(format!("runtime{n}")),
            npm_cli: Some(cli),
            prefix: Some(prefix),
        });
    }
    fs::write(&path, manifest).unwrap();
    f.program("brew", r#"printf M >> "$HOME/calls"; printf '{"formulae":[{"full_name":"node","installed":[{"version":"10.1.0"},{"version":"20.1.0"}]}]}'"#);
    let missing = f.input().prepare(&f.host).unwrap();
    assert_eq!(missing.review().needs_setup.len(), 2);
    assert_eq!(missing.review().inspections.len(), 1);
    let mut input = f.input();
    input.detection.overrides = vec![overrides[0].clone()];
    let partial = input.prepare(&f.host).unwrap();
    assert_eq!(partial.review().needs_setup.len(), 1);
    assert_eq!(partial.review().needs_setup[0].tool, "cli2");
    let mut input = f.input();
    input.detection.overrides = overrides;
    let discovery_input = input.detection.clone();
    let preview = input.prepare(&f.host).unwrap();
    let review = preview.review();
    assert!(review.needs_setup.is_empty());
    assert_eq!(review.inspections.len(), 3);
    for n in 1..=2 {
        let scope = review
            .inspections
            .iter()
            .find(|s| s.tools == [format!("cli{n}")])
            .unwrap();
        assert_eq!(
            scope
                .query
                .provider
                .npm_binding
                .as_ref()
                .unwrap()
                .runtime_tool,
            format!("runtime{n}")
        );
    }
    assert!(f.calls().is_empty());
    let checked = preview.approve().check(&f.host).unwrap();
    assert_eq!(checked.snapshots.len(), 3);
    assert_eq!(f.calls().matches('1').count(), 1);
    assert_eq!(f.calls().matches('2').count(), 1);
    let report = serde_json::to_string(&checked).unwrap();
    assert!(!report.contains("PRIVATE_MANIFEST") && !report.contains("PRIVATE_CLI_CONTENT"));
    let preview = super::installed_tools::prepare(&f.host, Some(path), discovery_input).unwrap();
    assert_eq!(preview.review().inspections.len(), 3);
    let current = preview.approve().inspect().unwrap();
    let mut runtimes = Vec::new();
    for row in current.rows() {
        if current.package(row).unwrap().0.query == Query::NpmGlobals {
            let seed = current.seed(row).unwrap();
            runtimes.push(seed.runtime.unwrap());
        }
    }
    assert_eq!(runtimes, ["runtime1", "runtime2"]);
    assert_eq!(f.calls().matches('1').count(), 2);
    assert_eq!(f.calls().matches('2').count(), 2);
    let report = serde_json::to_string(&current).unwrap();
    assert!(!report.contains("PRIVATE_MANIFEST") && !report.contains("PRIVATE_CLI_CONTENT"));
}
#[test]
fn unmanaged_commands_have_specific_reason_next_step_and_no_execution_warning() {
    let f = Fixture::new();
    for name in ["a", "b"] {
        f.program(name, "exit 99");
    }
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    assert_eq!(checked.missing(), 2);
    assert_eq!(checked.installable(), 0);
    let preview = checked.prepare_missing(&f.host).unwrap();
    assert!(preview.is_empty());
    assert_eq!(preview.deferred.len(), 2);
    for issue in &preview.deferred {
        assert!(issue.reason.contains("command already exists"));
        assert!(issue.next_step.contains("Review the command candidates"));
    }
    assert!(
        !preview.review().warning.contains("EXECUTE") && !preview.review().warning.contains("root")
    );
    assert!(preview.approve().is_err());
    assert_eq!(f.calls(), "Q");
}
#[test]
fn missing_prerequisites_are_deferred_outside_the_approved_install_set() {
    let f = Fixture::new();
    let path = f.repo.join("dort.toml");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("binary='b'", "binary='b'\nrequires=['a']"),
    )
    .unwrap();
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    let preview = checked.prepare_missing(&f.host).unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(preview.review().installations[0].0, "a");
    assert!(preview.deferred.iter().any(|d| d.tool == "b"
        && d.reason.contains("Required tool a")
        && d.next_step.contains("first")));
    assert!(!preview.review().warning.contains("root hooks"));
    let lock = WriterLock::acquire(&f.host).unwrap();
    assert!(
        preview
            .approve()
            .unwrap()
            .execute(&f.host, &lock, |_| {})
            .succeeded()
    );
    assert!(f.calls().contains('A') && !f.calls().contains('B'));
}

#[test]
fn changed_scope_refuses_queries_and_bulk_execution_before_provider_commands() {
    let f = Fixture::new();
    let preview = f.input().prepare(&f.host).unwrap();
    let path = f.repo.join("dort.toml");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{original}\n# changed\n")).unwrap();
    assert!(preview.approve().check(&f.host).is_err());
    assert!(f.calls().is_empty());
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    let approval = checked.prepare_missing(&f.host).unwrap().approve().unwrap();
    let config = f.host.local_config_path();
    fs::write(
        &config,
        format!("{}\n# changed\n", fs::read_to_string(&config).unwrap()),
    )
    .unwrap();
    let lock = WriterLock::acquire(&f.host).unwrap();
    let result = approval.execute(&f.host, &lock, |_| {});
    assert!(!result.succeeded());
    assert_eq!(result.not_started, ["b"]);
    assert_eq!(f.calls(), "Q");
}

#[test]
fn invalid_manifest_errors_withhold_contents_and_inactive_profiles_are_not_checked() {
    let f = Fixture::new();
    let path = f.repo.join("dort.toml");
    fs::write(&path, "schema=1\nPRIVATE_MANIFEST_SECRET not valid").unwrap();
    let error = f.input().prepare(&f.host).err().unwrap().to_string();
    assert!(!error.contains("PRIVATE_MANIFEST_SECRET"));
    assert!(f.calls().is_empty());
    fs::write(&path, "schema=1\n[profiles]\navailable=['dev']\n[[tools]]\nid='a'\nbinary='a'\nprofiles=['dev']\n[tools.install]\nprovider='brew'\npackage='a'\n").unwrap();
    let preview = f.input().prepare(&f.host).unwrap();
    assert!(preview.review().inspections.is_empty());
    let mut input = f.input();
    input.profiles.push("dev".into());
    assert_eq!(
        input.prepare(&f.host).unwrap().review().inspections[0].tools,
        ["a"]
    );
    assert!(f.calls().is_empty());
}

#[test]
fn declared_check_prepares_only_required_providers_and_binds_one_review_without_queries() {
    let f = Fixture::new();
    f.program("npm", "printf forbidden >> \"$HOME/npm-ran\"; exit 93");
    let preview = f.input().prepare(&f.host).unwrap();
    assert!(f.calls().is_empty());
    let review = preview.review();
    assert_eq!(review.inspections.len(), 1);
    assert_eq!(review.inspections[0].tools, &["a", "b"]);
    assert_eq!(review.bindings.len(), 2);
    let checked = preview.approve().check(&f.host).unwrap();
    assert_eq!(f.calls(), "Q");
    assert!(!f.host.home.join("npm-ran").exists());
    assert_eq!(
        checked.missing(),
        2,
        "{}",
        serde_json::to_string(&checked).unwrap()
    );
    assert_eq!(checked.installable(), 2);
    assert!(checked.available());
    assert!(
        checked
            .reconciliation
            .plan
            .rows
            .iter()
            .all(|r| declared_tools::status(r) == Status::Missing)
    );
}
#[test]
fn absent_provider_and_failed_query_are_unknown_not_missing() {
    let f = Fixture::new();
    fs::remove_file(f.bin.join("brew")).unwrap();
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    assert!(!checked.available());
    assert_eq!(checked.missing(), 0);
    assert_eq!(checked.needs_setup.len(), 2);
    assert!(
        checked
            .reconciliation
            .plan
            .rows
            .iter()
            .all(|r| declared_tools::status(r) == Status::CouldNotCheck)
    );
    f.program(
        "brew",
        "printf Q >> \"$HOME/calls\"; printf PRIVATE >&2; exit 8",
    );
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    assert!(!checked.available());
    assert_eq!(checked.missing(), 0);
    assert!(!serde_json::to_string(&checked).unwrap().contains("PRIVATE"));
}
#[test]
fn combined_review_is_process_free_consumes_exact_set_and_runs_serially() {
    let f = Fixture::new();
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    let preview = checked.prepare_missing(&f.host).unwrap();
    assert_eq!(preview.len(), 2);
    assert_eq!(f.calls(), "Q");
    assert_eq!(
        preview
            .review()
            .installations
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let approval = preview.approve().unwrap();
    let lock = WriterLock::acquire(&f.host).unwrap();
    let installed = approval.execute(&f.host, &lock, |_| {});
    assert!(
        installed.succeeded(),
        "{}",
        serde_json::to_string(&installed).unwrap()
    );
    assert_eq!(installed.results.len(), 2);
    assert!(installed.not_started.is_empty());
    assert_eq!(f.calls().matches('A').count(), 1);
    assert_eq!(f.calls().matches('B').count(), 1);
    assert!(f.calls().find('A').unwrap() < f.calls().find('B').unwrap());
    assert!(
        !serde_json::to_string(&installed)
            .unwrap()
            .contains("PRIVATE")
    );
}
#[test]
fn combined_install_stops_after_first_failed_process_without_replaying_or_undoing() {
    let f = Fixture::new();
    fs::write(f.host.home.join("fail-a"), "fixture").unwrap();
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    let approval = checked.prepare_missing(&f.host).unwrap().approve().unwrap();
    let lock = WriterLock::acquire(&f.host).unwrap();
    let installed = approval.execute(&f.host, &lock, |_| {});
    assert!(!installed.succeeded());
    assert_eq!(installed.results.len(), 1);
    assert_eq!(installed.not_started, ["b"]);
    assert!(!f.calls().contains('B'));
    assert!(!f.host.home.join("b").exists());
}
#[test]
fn stale_declarations_require_fresh_check_and_an_installed_claim_is_not_missing() {
    let f = Fixture::new();
    fs::write(f.host.home.join("a"), "installed").unwrap();
    let checked = f
        .input()
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap();
    assert_eq!(
        declared_tools::status(&checked.reconciliation.plan.rows[0]),
        Status::Installed
    );
    assert_eq!(checked.missing(), 1);
    fs::write(f.repo.join("dort.toml"), "schema=1\n").unwrap();
    assert!(checked.prepare_missing(&f.host).is_err());
    assert_eq!(f.calls(), "Q");
}
