mod support;
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    state::WriterLock,
    tools::{
        VersionPolicy,
        inspection::{self, Query},
        install::{self, Event, PackageState, ProcessOutcome, brew, npm},
    },
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
const MANIFEST: &str = "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='brew',package='demo',version='latest'}\n";
const BREW: &str = r##"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
[ "$PWD" = / ] && [ -z "$SSH_AUTH_SOCK" ] && [ -z "$MISE_YES" ] && [ "$XDG_STATE_HOME" = "$HOME/.local/state" ] || exit 90
case "$*" in
'help install')
 if [ -f "$HOME/bad-help" ]; then printf PRIVATE_HELP; else printf '%s\n' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP'; fi;;
'--prefix') if [ -f "$HOME/bad-prefix" ]; then printf '/wrong\n'; else printf '%s\n' "$HOME/prefix"; fi;;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --formula homebrew/core/demo')
 if [ -f "$HOME/bad-info" ]; then printf '{"formulae":[{"name":"alias","full_name":"alias","tap":"foreign/tap","disabled":false,"versions":{"stable":"1.2"},"installed":[]}]}'; exit 0; fi
 printf '{"formulae":[{"name":"demo","full_name":"demo","tap":"homebrew/core","disabled":false,"aliases":[],"oldnames":[],"versions":{"stable":"2.7"},"description":"PRIVATE_DESCRIPTION","installed":['
 if [ -f "$HOME/installed" ]; then printf '{"version":"2.7"}'; fi
 printf ']}]}';;
'info --json=v2 --installed --formula')
 if [ -f "$HOME/catalog-unavailable" ]; then printf PRIVATE_QUERY; exit 3; fi
 if [ -f "$HOME/installed" ]; then printf '{"formulae":[{"full_name":"demo","installed":[{"version":"2.7"}]}]}'
 elif [ -f "$HOME/ambiguous" ]; then printf '{"formulae":[{"full_name":"other/tap/demo","installed":[{"version":"2.7"}]}]}'
 else printf '{"formulae":[]}'; fi;;
'install --formula --no-ask --skip-post-install homebrew/core/demo')
 [ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ] && [ "$HOMEBREW_NO_ANALYTICS" = 1 ] && [ "$HOMEBREW_NO_ENV_HINTS" = 1 ] && [ "$HOMEBREW_NO_INSTALL_UPGRADE" = 1 ] && [ "$HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK" = 1 ] && [ "$HOMEBREW_NO_INSTALL_CLEANUP" = 1 ] || exit 91
 printf native-dependency > "$HOME/prefix/Cellar/dependency-effect"
 printf native-command > "$HOME/prefix/bin/demo"
 printf PRIVATE_INSTALL; printf PRIVATE_ERROR >&2
 if [ -f "$HOME/fail" ]; then exit 4; fi
 if [ -f "$HOME/change-self" ]; then printf '\n# changed\n' >> "$0"; fi
 printf installed > "$HOME/installed";;
*) exit 99;;
esac
"##;
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    executable: PathBuf,
    lock: WriterLock,
}
impl Fixture {
    fn new() -> Self {
        let mut home = support::TestHome::new();
        home.host.platform = dort_core::platform::Platform::Macos;
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        for d in [
            "Cellar",
            "bin",
            "etc",
            "include",
            "lib",
            "opt",
            "sbin",
            "share",
            "var/homebrew",
            "Homebrew/Library",
            "Homebrew/bin",
        ] {
            fs::create_dir_all(home.host.home.join("prefix").join(d)).unwrap();
        }
        fs::create_dir(home.host.home.join("lookup")).unwrap();
        let executable = home.host.home.join("prefix/Homebrew/bin/brew");
        fs::write(&executable, BREW).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let lock = WriterLock::acquire(&home.host).unwrap();
        RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            executable,
            lock,
        }
    }
    fn request(&self) -> install::Request {
        request(&self.home.host, self.repo.clone())
    }
    fn scope(&self) -> brew::Scope {
        scope(&self.home.host)
    }
    fn report(&self) -> inspection::Report {
        report(&self.home.host)
    }
    fn preview(&self) -> install::Preview {
        brew::prepare(
            &self.home.host,
            self.request(),
            self.scope(),
            &[&self.report()],
        )
        .unwrap()
    }
    fn flag(&self, name: &str) {
        fs::write(self.home.host.home.join(name), "flag").unwrap();
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.host.home.join("calls")).unwrap_or_default()
    }
    fn execute(&self, p: install::Preview) -> dort_core::Result<install::Report> {
        install::execute(&self.home.host, &self.lock, p.approve(), |_| {})
    }
}
fn request(host: &HostContext, repo: PathBuf) -> install::Request {
    install::Request {
        repository: repo,
        profiles: vec![],
        tool: "demo".into(),
        executable: host.home.join("prefix/Homebrew/bin/brew"),
        search_dirs: vec![host.home.join("lookup")],
        tool_search_dirs: vec![host.home.join("lookup")],
        bound_tools: BTreeSet::from(["demo".into()]),
    }
}
fn scope(host: &HostContext) -> brew::Scope {
    brew::Scope {
        prefix: host.home.join("prefix"),
        repository: host.home.join("prefix/Homebrew"),
        cellar: host.home.join("prefix/Cellar"),
        policy: VersionPolicy::Latest {},
        catalogs: vec![npm::Catalog {
            query: Query::BrewFormulae,
            executable: host.home.join("prefix/Homebrew/bin/brew"),
            search_dirs: vec![host.home.join("lookup")],
            npm_binding: None,
            tools: BTreeSet::from(["demo".into()]),
        }],
    }
}
fn report(host: &HostContext) -> inspection::Report {
    inspection::Preview::new(
        Query::BrewFormulae,
        host.home.join("prefix/Homebrew/bin/brew"),
        host.home.clone(),
        vec![host.home.join("lookup")],
    )
    .unwrap()
    .approve()
    .inspect()
}
fn close_request(id: String) -> install::CloseRequest {
    install::CloseRequest {
        id,
        executable: None,
        search_dirs: vec![],
        npm_binding: None,
        brew_prefix: None,
        apt: false,
    }
}
#[test]
fn process_free_review_and_fixed_native_install_record_current_not_exact() {
    let f = Fixture::new();
    let r = f.report();
    let calls = f.calls();
    let p = brew::prepare(&f.home.host, f.request(), f.scope(), &[&r]).unwrap();
    assert_eq!(calls, f.calls());
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
    assert!(p.review().warning.contains("CAN EXECUTE"));
    assert!(p.review().warning.contains("upgraded"));
    assert_eq!(p.review().fresh_probes.len(), 5);
    let out = f.execute(p).unwrap();
    assert_eq!(out.outcome.unwrap().process, ProcessOutcome::Success);
    assert_eq!(
        out.outcome.unwrap().package,
        PackageState::ProviderCurrentPresent
    );
    assert_eq!(out.observed_versions, ["2.7"]);
    assert!(!out.readiness_verified && !out.possible_partial_effects);
    let id = out.attempt_id.unwrap();
    let dir = f.home.host.state_dir.join("tools-attempts").join(&id);
    for name in ["intent.json", "outcome.json"] {
        let bytes = fs::read_to_string(dir.join(name)).unwrap();
        assert!(!bytes.contains("PRIVATE"));
        let env: serde_json::Value = serde_json::from_str(&bytes).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(env["payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["schema"], 3);
    }
    assert!(
        f.home
            .host
            .home
            .join("prefix/Cellar/dependency-effect")
            .exists()
    );
    assert!(f.home.host.home.join("prefix/bin/demo").exists());
    let a = install::diagnose(&f.home.host).unwrap();
    assert!(!a[0].unresolved);
    assert_eq!(a[0].observed_versions, ["2.7"]);
}
#[test]
fn newly_satisfied_is_noop_without_attempt_or_upgrade() {
    let f = Fixture::new();
    let p = f.preview();
    f.flag("installed");
    let out = f.execute(p).unwrap();
    assert!(out.no_op && out.attempt_id.is_none());
    assert!(!f.calls().contains("install --formula"));
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
}
#[test]
fn unknown_capabilities_layout_identity_and_catalogs_refuse_without_intent() {
    for flag in [
        "bad-help",
        "bad-prefix",
        "bad-info",
        "catalog-unavailable",
        "ambiguous",
    ] {
        let f = Fixture::new();
        let p = f.preview();
        f.flag(flag);
        assert!(f.execute(p).is_err(), "{flag}");
        assert!(!f.home.host.state_dir.join("tools-attempts").exists());
        assert!(!f.calls().contains("install --formula"));
    }
}
#[test]
fn unsafe_or_missing_native_roots_and_programs_refuse_process_free() {
    for case in 0..5 {
        let f = Fixture::new();
        let r = f.report();
        let calls = f.calls();
        let dir = f.home.host.home.join("prefix/lib");
        match case {
            0 => {
                fs::remove_dir(&dir).unwrap();
            }
            1 => {
                fs::remove_dir(&dir).unwrap();
                symlink(f.home.host.home.join("lookup"), &dir).unwrap();
            }
            2 => {
                fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
            }
            3 => {
                fs::hard_link(&f.executable, f.home.host.home.join("linked-brew")).unwrap();
            }
            _ => {
                fs::remove_file(&f.executable).unwrap();
                symlink(f.home.host.home.join("nonexistent"), &f.executable).unwrap();
            }
        }
        assert!(brew::prepare(&f.home.host, f.request(), f.scope(), &[&r]).is_err());
        assert_eq!(calls, f.calls());
    }
}
#[test]
fn unapproved_probe_paths_are_never_observed_or_copied_into_errors() {
    let f = Fixture::new();
    symlink(
        f.home.host.home.join("lookup"),
        f.home.host.home.join("private-link"),
    )
    .unwrap();
    let script = BREW.replace(
        "printf '/wrong\\n'",
        "printf '%s\\n' \"$HOME/private-link/PRIVATE_TOKEN\"",
    );
    assert_ne!(script, BREW);
    fs::write(&f.executable, script).unwrap();
    let p = f.preview();
    f.flag("bad-prefix");
    let error = f.execute(p).err().unwrap().to_string();
    assert!(error.contains("layout differs"));
    assert!(!error.contains("PRIVATE_TOKEN") && !error.contains("private-link"));
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
}
#[test]
fn unsupported_policy_qualified_taps_and_unmanaged_candidates_are_not_installable() {
    for change in [
        "package='third/tap/demo'",
        "package='homebrew/core/demo'",
        "package='demo',version='1.2.3'",
    ] {
        let f = Fixture::new();
        let r = f.report();
        fs::write(
            f.repo.join("dort.toml"),
            MANIFEST.replace("package='demo',version='latest'", change),
        )
        .unwrap();
        assert!(brew::prepare(&f.home.host, f.request(), f.scope(), &[&r]).is_err());
    }
    let f = Fixture::new();
    let r = f.report();
    fs::write(f.home.host.home.join("lookup/demo"), "not executed").unwrap();
    fs::set_permissions(
        f.home.host.home.join("lookup/demo"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(brew::prepare(&f.home.host, f.request(), f.scope(), &[&r]).is_err());
}
#[test]
fn source_profile_prefix_and_lookup_drift_refuse_before_query() {
    for case in 0..4 {
        let f = Fixture::new();
        let p = f.preview();
        let calls = f.calls();
        match case {
            0 => fs::write(f.repo.join("dort.toml"), format!("{MANIFEST}\n# drift")).unwrap(),
            1 => {
                let dir = f.home.host.home.join("prefix/lib");
                fs::rename(&dir, dir.with_file_name("old-lib")).unwrap();
                fs::create_dir(&dir).unwrap();
            }
            2 => fs::write(&f.executable, format!("{BREW}\n# drift")).unwrap(),
            _ => {
                let dir = f.home.host.home.join("lookup");
                fs::rename(&dir, dir.with_file_name("old-lookup")).unwrap();
                fs::create_dir(&dir).unwrap();
            }
        }
        assert!(f.execute(p).is_err());
        assert_eq!(calls, f.calls());
    }
}
#[test]
fn independent_final_binary_guard_leaves_pending_intent_without_spawn() {
    let f = Fixture::new();
    let p = f.preview();
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |e| {
        if e == Event::Installing {
            let path = f.home.host.home.join("lookup/demo");
            fs::write(&path, "NOT_EXECUTED").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    });
    assert!(result.is_err());
    assert!(!f.calls().contains("install --formula"));
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
}
#[test]
fn failures_are_partial_and_changed_provider_is_not_run_for_postinspection() {
    for flag in ["fail", "change-self"] {
        let f = Fixture::new();
        let p = f.preview();
        f.flag(flag);
        let out = f.execute(p).unwrap();
        assert!(out.possible_partial_effects);
        let outcome = out.outcome.unwrap();
        if flag == "fail" {
            assert_eq!(outcome.process, ProcessOutcome::Failed);
            assert_eq!(outcome.package, PackageState::Absent);
        } else {
            assert_eq!(outcome.package, PackageState::Unavailable);
            assert!(out.observed_versions.is_empty());
            assert!(
                f.calls().ends_with(
                    "install --formula --no-ask --skip-post-install homebrew/core/demo\n"
                )
            );
        }
        assert!(f.home.host.home.join("prefix/bin/demo").exists());
    }
}
#[test]
fn known_and_damaged_historical_closure_need_no_current_manifest_and_bind_head() {
    let f = Fixture::new();
    let p = f.preview();
    assert!(
        install::execute(&f.home.host, &f.lock, p.approve(), |e| {
            if e == Event::IntentDurable {
                f.flag("stop-marker");
                fs::write(f.repo.join("dort.toml"), "changed").unwrap();
            }
        })
        .is_err()
    );
    let a = install::diagnose(&f.home.host).unwrap().remove(0);
    let id = a.id;
    fs::remove_file(f.repo.join("dort.toml")).unwrap();
    let calls = f.calls();
    let p = install::prepare_close(&f.home.host, close_request(id.clone())).unwrap();
    assert_eq!(calls, f.calls());
    assert_eq!(p.review().fresh_probes, [vec!["--prefix".to_owned()]]);
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(a.explicitly_closed && !a.unresolved);
    let dir = f.home.host.state_dir.join("tools-attempts").join(&id);
    fs::write(dir.join("intent.json"), "damaged").unwrap();
    let mut req = close_request(id.clone());
    req.executable = Some(f.executable.clone());
    req.search_dirs = f.request().search_dirs;
    req.brew_prefix = Some(f.home.host.home.join("prefix"));
    let p = install::prepare_close(&f.home.host, req.clone()).unwrap();
    assert!(!p.review().original_scope_known);
    fs::write(dir.join("unexpected"), "metadata").unwrap();
    let calls = f.calls();
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    assert_eq!(calls, f.calls());
    let p = install::prepare_close(&f.home.host, req).unwrap();
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(a.explicitly_closed && !a.unresolved);
    assert_eq!(
        fs::read_to_string(dir.join("intent.json")).unwrap(),
        "damaged"
    );
}
#[test]
fn unresolved_tools_block_before_any_fresh_probe() {
    let f = Fixture::new();
    let p = f.preview();
    assert!(
        install::execute(&f.home.host, &f.lock, p.approve(), |e| {
            if e == Event::IntentDurable {
                fs::write(f.repo.join("dort.toml"), "changed").unwrap();
            }
        })
        .is_err()
    );
    fs::write(f.repo.join("dort.toml"), MANIFEST).unwrap();
    let p = f.preview();
    let calls = f.calls();
    assert!(f.execute(p).is_err());
    assert_eq!(calls, f.calls());
}
#[test]
fn malformed_file_journal_blocks_before_probe_but_not_historical_closure() {
    let f = Fixture::new();
    let p = f.preview();
    let local = dort_core::local::LocalConfig::load(&f.home.host).unwrap();
    let id = local.repositories.keys().next().unwrap();
    let parent = dort_core::local::repository_state_path(&f.home.host, id)
        .unwrap()
        .join("transactions");
    let path = parent.join("a".repeat(64));
    fs::create_dir_all(&path).unwrap();
    for d in [
        path,
        parent.clone(),
        parent.parent().unwrap().to_owned(),
        f.home.host.state_dir.join("repositories"),
    ] {
        fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let calls = f.calls();
    assert!(f.execute(p).is_err());
    assert_eq!(calls, f.calls());
    let root = f.home.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let id = "b".repeat(64);
    fs::create_dir(root.join(&id)).unwrap();
    fs::set_permissions(root.join(&id), fs::Permissions::from_mode(0o700)).unwrap();
    let mut req = close_request(id);
    req.executable = Some(f.executable.clone());
    req.search_dirs = f.request().search_dirs;
    req.brew_prefix = Some(f.home.host.home.join("prefix"));
    let p = install::prepare_close(&f.home.host, req).unwrap();
    assert!(
        install::close(&f.home.host, &f.lock, p.approve())
            .unwrap()
            .explicitly_closed
    );
}
#[test]
fn explicit_cross_catalog_prerequisites_are_fresh_and_never_installed() {
    for missing in [false, true] {
        let f = Fixture::new();
        fs::write(f.repo.join("dort.toml"),MANIFEST.replace("binary='demo'", "binary='demo'\nrequires=['prereq']") + "\n[[tools]]\nid='prereq'\nbinary='prereq'\n[tools.install]\nmacos={provider='mise',package='prereq',version='1.2.3'}\n").unwrap();
        let exe = f.home.host.home.join("mise");
        fs::write(&exe,"#!/bin/sh\nprintf m >> \"$HOME/calls\"\nif [ -f \"$HOME/prereq-missing\" ]; then printf '{}'; else printf '{\"prereq\":[{\"version\":\"1.2.3\",\"installed\":true}]}'; fi\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let runtime = inspection::Preview::new(
            Query::MiseInstalled,
            exe.clone(),
            f.home.host.home.clone(),
            f.request().search_dirs,
        )
        .unwrap()
        .approve()
        .inspect();
        let target = f.report();
        let mut scope = f.scope();
        scope.catalogs.push(npm::Catalog {
            query: Query::MiseInstalled,
            executable: exe,
            search_dirs: f.request().search_dirs,
            npm_binding: None,
            tools: BTreeSet::from(["prereq".into()]),
        });
        let mut req = f.request();
        req.bound_tools.insert("prereq".into());
        let p = brew::prepare(&f.home.host, req, scope, &[&target, &runtime]).unwrap();
        if missing {
            f.flag("prereq-missing");
            assert!(f.execute(p).is_err());
            assert!(!f.calls().contains("install --formula"));
        } else {
            let out = f.execute(p).unwrap();
            assert_eq!(
                out.outcome.unwrap().package,
                PackageState::ProviderCurrentPresent
            );
        }
    }
}
#[test]
fn capability_tokens_canonical_metadata_and_layout_paths_fail_closed() {
    for (old, new) in [
        (
            "--no-ask --skip-post-install",
            "--no-ask-suffix --skip-post-install",
        ),
        (
            "HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
            "UNSUPPORTED HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
        ),
        ("\"disabled\":false", "\"disabled\":true"),
        ("\"aliases\":[]", "\"aliases\":[\"other\"]"),
        ("\"oldnames\":[]", "\"oldnames\":[\"renamed\"]"),
        ("\"tap\":\"homebrew/core\"", "\"tap\":\"third/tap\""),
        ("\"stable\":\"2.7\"", "\"stable\":null"),
        ("$HOME/prefix/Homebrew\";;", "$HOME/prefix/Cellar\";;"),
    ] {
        let f = Fixture::new();
        assert!(BREW.contains(old));
        fs::write(&f.executable, BREW.replace(old, new)).unwrap();
        let p = f.preview();
        assert!(f.execute(p).is_err(), "{old}");
        assert!(!f.home.host.state_dir.join("tools-attempts").exists());
        assert!(!f.calls().contains("install --formula"));
    }
}
#[test]
fn mixed_schema_one_two_three_history_is_read_without_reencoding_legacy_artifacts() {
    let f = Fixture::new();
    let out = f.execute(f.preview()).unwrap();
    let brew_id = out.attempt_id.unwrap();
    let mise = f.home.host.home.join("mise");
    fs::write(&mise,"#!/bin/sh\nif [ \"$1\" = ls ]; then\nif [ -f \"$HOME/legacy-installed\" ]; then printf '{\"legacy\":[{\"version\":\"1.2.3\",\"installed\":true}],\"node\":[{\"version\":\"20.1.0\",\"installed\":true}]}'; else printf '{\"node\":[{\"version\":\"20.1.0\",\"installed\":true}]}'; fi\nelse printf installed > \"$HOME/legacy-installed\"; fi\n").unwrap();
    fs::set_permissions(&mise, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.repo.join("dort.toml"),"schema=1\n[[tools]]\nid='legacy'\nbinary='legacy'\n[tools.install]\nmacos={provider='mise',package='legacy',version='1.2.3'}\n").unwrap();
    let query = || {
        inspection::Preview::new(
            Query::MiseInstalled,
            mise.clone(),
            f.home.host.home.clone(),
            f.request().search_dirs,
        )
        .unwrap()
        .approve()
        .inspect()
    };
    let mut req = f.request();
    req.executable = mise.clone();
    req.tool = "legacy".into();
    req.bound_tools = BTreeSet::from(["legacy".into()]);
    let p = install::prepare(&f.home.host, req, &query()).unwrap();
    let legacy = f.execute(p).unwrap().attempt_id.unwrap();
    let bytes = |id: &str, name: &str| {
        fs::read(
            f.home
                .host
                .state_dir
                .join("tools-attempts")
                .join(id)
                .join(name),
        )
        .unwrap()
    };
    let old_intent = bytes(&legacy, "intent.json");
    let old_outcome = bytes(&legacy, "outcome.json");
    fs::write(f.repo.join("dort.toml"),"schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='20.1.0'}\n[[tools]]\nid='pkg'\nbinary='pkg'\n[tools.install]\nmacos={provider='npm',package='pkg',version='1.2.3',runtime_tool='node'}\n").unwrap();
    let node = f.home.host.home.join("lookup/node");
    let cli = f.home.host.home.join("npm-cli.js");
    fs::write(&cli, "METADATA_ONLY").unwrap();
    fs::write(&node,"#!/bin/sh\nshift\nif [ \"$1\" = ls ]; then\nif [ -f \"$HOME/npm-installed\" ]; then printf '{\"dependencies\":{\"pkg\":{\"version\":\"1.2.3\"}}}'; else printf '{\"dependencies\":{}}'; fi\nelse printf installed > \"$HOME/npm-installed\"; fi\n").unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
    let binding = inspection::NpmBinding {
        runtime_tool: "node".into(),
        node: node.clone(),
        npm_cli: cli,
        prefix: f.home.host.home.join("npm-global"),
    };
    let manifest =
        dort_core::Manifest::parse(&fs::read_to_string(f.repo.join("dort.toml")).unwrap()).unwrap();
    let target = inspection::Preview::npm(
        &manifest,
        binding.clone(),
        f.home.host.home.clone(),
        f.request().search_dirs,
    )
    .unwrap()
    .approve()
    .inspect();
    let runtime = query();
    let mut req = f.request();
    req.tool = "pkg".into();
    req.executable = node.clone();
    req.bound_tools = BTreeSet::from(["node".into(), "pkg".into()]);
    let scope = npm::Scope {
        binding: binding.clone(),
        catalogs: vec![
            npm::Catalog {
                query: Query::NpmGlobals,
                executable: node,
                search_dirs: req.search_dirs.clone(),
                npm_binding: Some(binding),
                tools: BTreeSet::from(["pkg".into()]),
            },
            npm::Catalog {
                query: Query::MiseInstalled,
                executable: mise,
                search_dirs: req.search_dirs.clone(),
                npm_binding: None,
                tools: BTreeSet::from(["node".into()]),
            },
        ],
    };
    let p = npm::prepare(&f.home.host, req, scope, &[&target, &runtime]).unwrap();
    let npm_id = f.execute(p).unwrap().attempt_id.unwrap();
    let npm_intent = bytes(&npm_id, "intent.json");
    let npm_outcome = bytes(&npm_id, "outcome.json");
    let brew_intent = bytes(&brew_id, "intent.json");
    let brew_outcome = bytes(&brew_id, "outcome.json");
    // Schema 4 shares the attempt directory without migrating schemas 1–3.
    let mut apt_host = f.home.host.clone();
    apt_host.platform = dort_core::platform::Platform::Ubuntu;
    fs::write(f.repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nubuntu={provider='apt',package='demo',version='latest'}\n").unwrap();
    for (name, body) in [
        (
            "dpkg-query",
            "if [ -f \"$HOME/apt-installed\" ]; then printf 'demo\\t2.7\\tinstalled\\tok\\n'; fi",
        ),
        (
            "apt-cache",
            "printf 'demo:\\n  Installed: (none)\\n  Candidate: 2.7\\n'",
        ),
        ("apt-get", "printf installed > \"$HOME/apt-installed\""),
        ("sudo", "shift 6; \"$@\""),
    ] {
        let path = apt_host.home.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut req = f.request();
    req.executable = apt_host.home.join("dpkg-query");
    let target = inspection::Preview::new(
        Query::DpkgPackages,
        req.executable.clone(),
        apt_host.home.clone(),
        req.search_dirs.clone(),
    )
    .unwrap()
    .approve()
    .inspect();
    let scope = install::apt::Scope {
        apt_cache: apt_host.home.join("apt-cache"),
        apt_get: apt_host.home.join("apt-get"),
        sudo: apt_host.home.join("sudo"),
        policy: VersionPolicy::Latest {},
        catalogs: vec![npm::Catalog {
            query: Query::DpkgPackages,
            executable: req.executable.clone(),
            search_dirs: req.search_dirs.clone(),
            npm_binding: None,
            tools: req.bound_tools.clone(),
        }],
    };
    let preview = install::apt::prepare(&apt_host, req, scope, &[&target]).unwrap();
    install::execute(&apt_host, &f.lock, preview.approve(), |_| {}).unwrap();
    let a = install::diagnose(&f.home.host).unwrap();
    assert_eq!(a.len(), 4);
    for attempt in &a {
        let s = attempt.summary();
        let current = attempt
            .scope
            .as_ref()
            .is_some_and(|s| s.brew.is_some() || s.apt.is_some());
        assert_eq!(
            s.status,
            if current {
                install::Status::CurrentReported
            } else {
                install::Status::ExactReported
            }
        );
        assert!(!s.readiness_verified);
        let json = serde_json::to_value(attempt).unwrap();
        assert_eq!(
            json["summary"]["status"],
            if current {
                "current_reported"
            } else {
                "exact_reported"
            }
        );
        assert!(
            !String::from_utf8(bytes(&attempt.id, "intent.json"))
                .unwrap()
                .contains("summary")
        );
        assert!(
            !String::from_utf8(bytes(&attempt.id, "outcome.json"))
                .unwrap()
                .contains("summary")
        );
    }
    assert_eq!(brew_intent, bytes(&brew_id, "intent.json"));
    assert_eq!(brew_outcome, bytes(&brew_id, "outcome.json"));
    assert!(a.iter().all(|a| !a.unresolved));
    assert_eq!(old_intent, bytes(&legacy, "intent.json"));
    assert_eq!(old_outcome, bytes(&legacy, "outcome.json"));
    assert_eq!(npm_intent, bytes(&npm_id, "intent.json"));
    assert_eq!(npm_outcome, bytes(&npm_id, "outcome.json"));
    assert!(
        a.iter()
            .find(|a| a.id == brew_id)
            .unwrap()
            .scope
            .as_ref()
            .unwrap()
            .brew
            .is_some()
    );
}
#[test]
fn brew_crash_child() {
    let Some(base) = std::env::var_os("DORT_BREW_CRASH_ROOT") else {
        return;
    };
    let base = PathBuf::from(base);
    let host = HostContext::new(
        base.join("home"),
        None,
        None,
        dort_core::platform::Platform::Macos,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let r = report(&host);
    let p = brew::prepare(
        &host,
        request(&host, base.join("repo")),
        scope(&host),
        &[&r],
    )
    .unwrap();
    let goal = std::env::var("DORT_BREW_CRASH_AT").unwrap();
    install::execute(&host, &lock, p.approve(), |e| {
        if format!("{e:?}") == goal {
            std::process::exit(88);
        }
    })
    .unwrap();
    panic!("crash boundary not reached");
}
#[test]
fn process_exit_keeps_native_effects_intent_and_outcome_without_replay() {
    for phase in [
        "IntentDurable",
        "Installing",
        "PostInspection",
        "OutcomeDurable",
    ] {
        let f = Fixture::new();
        let base = f.home.sandbox.path().canonicalize().unwrap();
        let host = f.home.host.clone();
        drop(f.lock);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "brew_crash_child", "--test-threads=1"])
            .env_clear()
            .env("DORT_BREW_CRASH_ROOT", base)
            .env("DORT_BREW_CRASH_AT", phase)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(88), "{phase}");
        let calls = fs::read_to_string(host.home.join("calls")).unwrap();
        let a = install::diagnose(&host).unwrap().remove(0);
        assert_eq!(a.unresolved, phase != "OutcomeDurable");
        assert_eq!(
            host.home.join("prefix/bin/demo").exists(),
            matches!(phase, "PostInspection" | "OutcomeDurable")
        );
        assert_eq!(calls, fs::read_to_string(host.home.join("calls")).unwrap());
    }
}
