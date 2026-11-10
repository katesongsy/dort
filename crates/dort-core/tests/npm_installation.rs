mod support;
use dort_core::{
    local::RepositoryRegistry,
    state::WriterLock,
    tools::{
        inspection::{self, NpmBinding, Query},
        install::{self, Event, PackageState, ProcessOutcome, npm},
    },
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};

const MANIFEST: &str = "schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='20.1.0'}\nubuntu={provider='mise',package='node',version='20.1.0'}\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='npm',package='@demo/pkg',version='1.2.3',runtime_tool='node'}\nubuntu={provider='npm',package='@demo/pkg',version='1.2.3',runtime_tool='node'}\n";
const NODE: &str = r##"#!/bin/sh
[ "$1" = "$HOME/runtime/npm-cli.js" ] || exit 90
shift
if [ "$1" = ls ]; then
  printf n >> "$HOME/calls"
  if [ -f "$HOME/unavailable" ]; then printf PRIVATE_NPM_OUTPUT; exit 1; fi
  if [ -f "$HOME/installed" ]; then printf '{"dependencies":{"@demo/pkg":{"version":"1.2.3"}}}'
  elif [ -f "$HOME/conflict" ]; then printf '{"dependencies":{"@demo/pkg":{"version":"9.0.0"}}}'
  else printf '{"dependencies":{}}'; fi
  exit 0
fi
[ "$#" = 14 ] && [ "$1" = install ] && [ "$2" = --global ] && [ "$3" = --prefix ] && [ "$4" = "$HOME/prefix/deep" ] || exit 91
shift 4
[ "$1" = --ignore-scripts ] && [ "$2" = --audit=false ] && [ "$3" = --fund=false ] && [ "$4" = --update-notifier=false ] && [ "$5" = --logs-max=0 ] && [ "$6" = --loglevel=silent ] && [ "$7" = --registry=https://registry.npmjs.org/ ] && [ "$8" = --userconfig=/dev/null ] && [ "$9" = --globalconfig=/dev/null ] || exit 92
shift 9
[ "$1" = @demo/pkg@1.2.3 ] && [ "$PWD" = / ] && [ "$XDG_STATE_HOME" = "$HOME/.local/state" ] && [ -z "$NODE_OPTIONS" ] && [ -z "$NPM_CONFIG_USERCONFIG" ] && [ -z "$SSH_AUTH_SOCK" ] && [ -z "$MISE_YES" ] || exit 93
printf i >> "$HOME/calls"
printf PRIVATE_NPM_OUTPUT
printf PRIVATE_NPM_ERROR >&2
printf native-effect > "$HOME/prefix/deep/lib/node_modules/effect"
printf native-command > "$HOME/prefix/deep/bin/demo"
printf native-man > "$HOME/prefix/deep/share/man/demo.1"
if [ -f "$HOME/fail" ]; then exit 4; fi
if [ -f "$HOME/unverified" ]; then exit 0; fi
printf installed > "$HOME/installed"
"##;
const MISE: &str = r##"#!/bin/sh
[ "$1" = ls ] && [ "$2" = --installed ] && [ "$3" = --json ] || exit 90
printf m >> "$HOME/calls"
if [ -f "$HOME/change-cli" ]; then printf MUTATED_CLI > "$HOME/runtime/npm-cli.js"; fi
if [ -f "$HOME/runtime-unavailable" ]; then exit 1; fi
if [ -f "$HOME/runtime-missing" ]; then printf '{}'; else printf '{"node":[{"version":"20.1.0","installed":true}]}'; fi
"##;
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    node: PathBuf,
    cli: PathBuf,
    mise: PathBuf,
    prefix: PathBuf,
    lock: WriterLock,
}
impl Fixture {
    fn new() -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        let runtime = home.host.home.join("runtime");
        fs::create_dir(&runtime).unwrap();
        let node = runtime.join("node");
        let cli = runtime.join("npm-cli.js");
        let mise = home.host.home.join("mise");
        for (path, body) in [(&node, NODE), (&mise, MISE), (&cli, "CLI_METADATA_ONLY")] {
            fs::write(path, body).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let prefix = home.host.home.join("prefix/deep");
        let lock = WriterLock::acquire(&home.host).unwrap();
        RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            node,
            cli,
            mise,
            prefix,
            lock,
        }
    }
    fn binding(&self) -> NpmBinding {
        NpmBinding {
            runtime_tool: "node".into(),
            node: self.node.clone(),
            npm_cli: self.cli.clone(),
            prefix: self.prefix.clone(),
        }
    }
    fn request(&self) -> install::Request {
        install::Request {
            repository: self.repo.clone(),
            profiles: vec![],
            tool: "demo".into(),
            executable: self.node.clone(),
            search_dirs: vec![self.home.host.home.join("no-helpers")],
            tool_search_dirs: vec![self.node.parent().unwrap().to_owned()],
            bound_tools: BTreeSet::from(["demo".into(), "node".into()]),
        }
    }
    fn scope(&self) -> npm::Scope {
        let dirs = self.request().search_dirs;
        npm::Scope {
            binding: self.binding(),
            catalogs: vec![
                npm::Catalog {
                    query: Query::MiseInstalled,
                    executable: self.mise.clone(),
                    search_dirs: dirs.clone(),
                    npm_binding: None,
                    tools: BTreeSet::from(["node".into()]),
                },
                npm::Catalog {
                    query: Query::NpmGlobals,
                    executable: self.node.clone(),
                    search_dirs: dirs,
                    npm_binding: Some(self.binding()),
                    tools: BTreeSet::from(["demo".into()]),
                },
            ],
        }
    }
    fn reports(&self) -> Vec<inspection::Report> {
        vec![
            inspection::Preview::new(
                Query::MiseInstalled,
                self.mise.clone(),
                self.home.host.home.clone(),
                self.request().search_dirs.clone(),
            )
            .unwrap()
            .approve()
            .inspect(),
            inspection::Preview::npm(
                &dort_core::Manifest::parse(MANIFEST).unwrap(),
                self.binding(),
                self.home.host.home.clone(),
                self.request().search_dirs,
            )
            .unwrap()
            .approve()
            .inspect(),
        ]
    }
    fn preview(&self) -> install::Preview {
        let r = self.reports();
        npm::prepare(
            &self.home.host,
            self.request(),
            self.scope(),
            &r.iter().collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn execute(&self) -> install::Report {
        install::execute(
            &self.home.host,
            &self.lock,
            self.preview().approve(),
            |_| {},
        )
        .unwrap()
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.host.home.join("calls")).unwrap_or_default()
    }
    fn mark(&self, name: &str) {
        fs::write(self.home.host.home.join(name), "fixture").unwrap();
    }
    fn attempt(&self, id: &str) -> PathBuf {
        self.home.host.state_dir.join("tools-attempts").join(id)
    }
    fn pending(&self) -> String {
        let p = self.preview();
        assert!(
            install::execute(&self.home.host, &self.lock, p.approve(), |e| {
                if e == Event::IntentDurable {
                    self.mark("changed");
                    fs::write(&self.cli, "changed CLI").unwrap();
                }
            })
            .is_err()
        );
        fs::write(&self.cli, "CLI_METADATA_ONLY").unwrap();
        let a = install::diagnose(&self.home.host).unwrap();
        assert!(a[0].unresolved);
        a[0].id.clone()
    }
}
#[test]
fn exact_npm_is_process_free_until_approval_and_has_native_effects_and_schema_two() {
    let f = Fixture::new();
    let r = f.reports();
    let calls = f.calls();
    let p = npm::prepare(
        &f.home.host,
        f.request(),
        f.scope(),
        &r.iter().collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(f.calls(), calls);
    assert!(!f.prefix.exists());
    let review = serde_json::to_string(&p.review()).unwrap();
    assert!(
        review.contains("--ignore-scripts")
            && review.contains("--globalconfig=/dev/null")
            && review.contains("bin/man")
    );
    let report = install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).unwrap();
    assert_eq!(report.outcome.unwrap().process, ProcessOutcome::Success);
    assert_eq!(report.outcome.unwrap().package, PackageState::ExactPresent);
    assert!(!report.readiness_verified && !report.possible_partial_effects);
    assert_eq!(f.calls(), "mnmnimn");
    for path in ["lib/node_modules/effect", "bin/demo", "share/man/demo.1"] {
        assert!(f.prefix.join(path).is_file());
    }
    let id = report.attempt_id.unwrap();
    for name in ["intent.json", "outcome.json"] {
        let text = fs::read_to_string(f.attempt(&id).join(name)).unwrap();
        let e: serde_json::Value = serde_json::from_str(&text).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(e["payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["schema"], 2);
        assert!(!text.contains("PRIVATE_NPM"));
        assert_eq!(
            fs::metadata(f.attempt(&id).join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(!install::diagnose(&f.home.host).unwrap()[0].unresolved);
}
#[test]
fn cross_catalog_runtime_must_be_explicit_satisfied_and_the_sole_selected_node() {
    let f = Fixture::new();
    let reports = f.reports();
    let refs = reports.iter().collect::<Vec<_>>();
    let mut scope = f.scope();
    scope.catalogs.remove(0);
    assert!(npm::prepare(&f.home.host, f.request(), scope, &refs[1..]).is_err());
    let mut scope = f.scope();
    scope.catalogs[1].tools.insert("node".into());
    assert!(npm::prepare(&f.home.host, f.request(), scope, &refs).is_err());
    for marker in ["runtime-missing", "runtime-unavailable"] {
        f.mark(marker);
        let r = f.reports();
        assert!(
            npm::prepare(
                &f.home.host,
                f.request(),
                f.scope(),
                &r.iter().collect::<Vec<_>>()
            )
            .is_err()
        );
        fs::remove_file(f.home.host.home.join(marker)).unwrap();
    }
    let other = f.home.host.home.join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("node"), "NEVER_RUN").unwrap();
    let mut request = f.request();
    request.tool_search_dirs.push(other);
    assert!(npm::prepare(&f.home.host, request, f.scope(), &refs).is_err());
    assert!(!f.prefix.exists());
}
#[test]
fn fresh_runtime_target_and_guards_are_checked_before_prefix_or_install() {
    for marker in [
        "runtime-missing",
        "runtime-unavailable",
        "conflict",
        "unavailable",
    ] {
        let f = Fixture::new();
        let p = f.preview();
        f.mark(marker);
        assert!(install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).is_err());
        assert!(!f.calls().contains('i'));
        assert!(!f.prefix.exists());
        assert!(install::diagnose(&f.home.host).unwrap().is_empty());
    }
    let f = Fixture::new();
    let p = f.preview();
    f.mark("installed");
    let r = install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).unwrap();
    assert!(r.no_op && r.attempt_id.is_none());
    assert!(!f.prefix.exists());
    let f = Fixture::new();
    let p = f.preview();
    let calls = f.calls();
    fs::write(&f.cli, "changed").unwrap();
    assert!(install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).is_err());
    assert_eq!(calls, f.calls());
}
#[test]
fn unsafe_prefixes_and_runtime_inside_native_write_roots_refuse_without_query() {
    let f = Fixture::new();
    let reports = f.reports();
    let calls = f.calls();
    for prefix in [
        f.home.host.home.clone(),
        PathBuf::from("/usr/local"),
        f.home.host.home.join("linked/deep"),
    ] {
        if prefix.ends_with("linked/deep") {
            symlink(f.node.parent().unwrap(), f.home.host.home.join("linked")).unwrap();
        }
        let mut scope = f.scope();
        scope.binding.prefix = prefix.clone();
        scope.catalogs[1].npm_binding.as_mut().unwrap().prefix = prefix;
        assert!(
            npm::prepare(
                &f.home.host,
                f.request(),
                scope,
                &reports.iter().collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    fs::create_dir(f.home.host.home.join("prefix")).unwrap();
    fs::set_permissions(
        f.home.host.home.join("prefix"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    assert!(
        npm::prepare(
            &f.home.host,
            f.request(),
            f.scope(),
            &reports.iter().collect::<Vec<_>>()
        )
        .is_err()
    );
    assert_eq!(calls, f.calls());
    // No queries were used to reject prefix safety problems.
    assert_eq!(calls, f.calls());
}
#[test]
fn native_write_roots_cannot_contain_node_or_cli_and_mutation_parents_cannot_be_links() {
    for relative in [
        "bin/node",
        "lib/node_modules/npm/npm-cli.js",
        "share/man/npm-cli.js",
    ] {
        let f = Fixture::new();
        let mut reports = f.reports();
        let calls = f.calls();
        let path = f.prefix.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "TRUSTED_PROGRAM").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut request = f.request();
        let mut scope = f.scope();
        if relative == "bin/node" {
            scope.binding.node = path.clone();
            request.executable = path;
        } else {
            scope.binding.npm_cli = path;
        }
        scope.catalogs[1].executable = scope.binding.node.clone();
        scope.catalogs[1].npm_binding = Some(scope.binding.clone());
        reports[1].description.executable = scope.binding.node.clone();
        reports[1].description.npm_binding = Some(scope.binding.clone());
        assert!(
            npm::prepare(
                &f.home.host,
                request,
                scope,
                &reports.iter().collect::<Vec<_>>()
            )
            .is_err()
        );
        assert_eq!(calls, f.calls());
    }
    for relative in [
        "lib",
        "lib/node_modules",
        "lib/node_modules/@demo",
        "bin",
        "share",
        "share/man",
    ] {
        let f = Fixture::new();
        let reports = f.reports();
        let path = f.prefix.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(f.node.parent().unwrap(), &path).unwrap();
        assert!(
            npm::prepare(
                &f.home.host,
                f.request(),
                f.scope(),
                &reports.iter().collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn trusted_programs_are_bound_by_metadata_without_reading_or_running_their_bodies() {
    for kind in ["link", "hardlink", "directory", "large-node"] {
        let f = Fixture::new();
        let reports = f.reports();
        let calls = f.calls();
        match kind {
            "link" => {
                fs::remove_file(&f.cli).unwrap();
                symlink(&f.node, &f.cli).unwrap();
            }
            "hardlink" => {
                fs::hard_link(&f.cli, f.home.host.home.join("other-cli")).unwrap();
            }
            "directory" => {
                fs::remove_file(&f.cli).unwrap();
                fs::create_dir(&f.cli).unwrap();
            }
            _ => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&f.node)
                    .unwrap()
                    .set_len(32 * 1024 * 1024)
                    .unwrap();
            }
        }
        let result = npm::prepare(
            &f.home.host,
            f.request(),
            f.scope(),
            &reports.iter().collect::<Vec<_>>(),
        );
        assert_eq!(result.is_ok(), kind == "large-node");
        assert_eq!(calls, f.calls());
    }
}
#[test]
fn cli_drift_between_catalog_queries_refuses_before_executing_node_and_post_drift_is_unavailable() {
    let f = Fixture::new();
    let p = f.preview();
    f.mark("change-cli");
    assert!(install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).is_err());
    assert_eq!(f.calls(), "mnm");
    assert!(!f.prefix.exists());
    let f = Fixture::new();
    let p = f.preview();
    let r = install::execute(&f.home.host, &f.lock, p.approve(), |e| {
        if e == Event::PostInspection {
            fs::write(&f.cli, "changed CLI").unwrap();
        }
    })
    .unwrap();
    assert_eq!(r.outcome.unwrap().package, PackageState::Unavailable);
    assert!(r.possible_partial_effects);
    assert_eq!(f.calls(), "mnmni");
    assert!(!install::diagnose(&f.home.host).unwrap()[0].unresolved);
}
#[test]
fn directory_substitution_and_binary_appearing_after_intent_refuse_spawn() {
    for binary in [false, true] {
        let f = Fixture::new();
        let p = f.preview();
        let r = install::execute(&f.home.host, &f.lock, p.approve(), |e| {
            if e == Event::IntentDurable {
                if binary {
                    fs::write(f.node.parent().unwrap().join("demo"), "NEVER_EXECUTE").unwrap();
                } else {
                    fs::create_dir(f.home.host.home.join("prefix")).unwrap();
                }
            }
        });
        assert!(r.is_err());
        assert!(!f.calls().contains('i'));
        assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
    }
}
#[test]
fn failures_have_durable_typed_outcomes_and_never_claim_rollback_or_readiness() {
    for marker in ["fail", "unverified"] {
        let f = Fixture::new();
        f.mark(marker);
        let r = f.execute();
        assert!(r.possible_partial_effects && !r.readiness_verified);
        assert_eq!(r.outcome.unwrap().package, PackageState::Absent);
        assert!(f.prefix.join("bin/demo").exists());
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("PRIVATE_NPM"));
    }
}
#[test]
fn historical_npm_closure_does_not_require_current_manifest_or_invent_runtime_trust() {
    let f = Fixture::new();
    let id = f.pending();
    fs::write(f.repo.join("dort.toml"), "changed manifest").unwrap();
    let request = install::CloseRequest {
        id: id.clone(),
        executable: None,
        search_dirs: vec![],
        npm_binding: None,
    };
    let calls = f.calls();
    let p = install::prepare_close(&f.home.host, request).unwrap();
    assert_eq!(f.calls(), calls);
    assert!(p.review().original_scope_known && p.review().query.query == Query::NpmGlobals);
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(a.explicitly_closed && !a.unresolved);
    assert!(!f.prefix.exists());
    let envelope: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(f.attempt(&id).join("close-0000.json")).unwrap())
            .unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["schema"], 2);
    fs::write(f.attempt(&id).join("intent.json"), "DAMAGED_PRIVATE").unwrap();
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
    let request = install::CloseRequest {
        id,
        executable: Some(f.node.clone()),
        search_dirs: f.request().search_dirs,
        npm_binding: Some(f.binding()),
    };
    let p = install::prepare_close(&f.home.host, request).unwrap();
    assert!(!p.review().original_scope_known);
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(!a.unresolved);
}
#[test]
fn npm_crash_child() {
    let Some(base) = std::env::var_os("DORT_NPM_CRASH_ROOT") else {
        return;
    };
    let base = PathBuf::from(base);
    let host = dort_core::context::HostContext::new(
        base.join("home"),
        None,
        None,
        dort_core::platform::Platform::detect().unwrap(),
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let binding = NpmBinding {
        runtime_tool: "node".into(),
        node: host.home.join("runtime/node"),
        npm_cli: host.home.join("runtime/npm-cli.js"),
        prefix: host.home.join("prefix/deep"),
    };
    let dirs = vec![host.home.join("no-helpers")];
    let reports = [
        inspection::Preview::new(
            Query::MiseInstalled,
            host.home.join("mise"),
            host.home.clone(),
            dirs.clone(),
        )
        .unwrap()
        .approve()
        .inspect(),
        inspection::Preview::npm(
            &dort_core::Manifest::parse(MANIFEST).unwrap(),
            binding.clone(),
            host.home.clone(),
            dirs.clone(),
        )
        .unwrap()
        .approve()
        .inspect(),
    ];
    let scope = npm::Scope {
        binding: binding.clone(),
        catalogs: vec![
            npm::Catalog {
                query: Query::MiseInstalled,
                executable: host.home.join("mise"),
                search_dirs: dirs.clone(),
                npm_binding: None,
                tools: BTreeSet::from(["node".into()]),
            },
            npm::Catalog {
                query: Query::NpmGlobals,
                executable: binding.node.clone(),
                search_dirs: dirs.clone(),
                npm_binding: Some(binding.clone()),
                tools: BTreeSet::from(["demo".into()]),
            },
        ],
    };
    let request = install::Request {
        repository: base.join("repo"),
        profiles: vec![],
        tool: "demo".into(),
        executable: binding.node,
        search_dirs: dirs,
        tool_search_dirs: vec![host.home.join("runtime")],
        bound_tools: BTreeSet::from(["node".into(), "demo".into()]),
    };
    let p = npm::prepare(&host, request, scope, &reports.iter().collect::<Vec<_>>()).unwrap();
    let goal = std::env::var("DORT_NPM_CRASH_AT").unwrap();
    install::execute(&host, &lock, p.approve(), |e| {
        if format!("{e:?}") == goal {
            std::process::exit(88);
        }
    })
    .unwrap();
    panic!("crash boundary did not run");
}
#[test]
fn npm_process_exit_preserves_intent_prefix_effects_and_outcome_without_replay() {
    use std::process::{Command, Stdio};
    for phase in [
        "IntentDurable",
        "Installing",
        "PostInspection",
        "OutcomeDurable",
    ] {
        let f = Fixture::new();
        let base = f.home.sandbox.path().canonicalize().unwrap();
        let host = f.home.host.clone();
        let prefix = f.prefix.clone();
        drop(f.lock);
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "npm_crash_child", "--test-threads=1"])
            .env_clear()
            .env("DORT_NPM_CRASH_ROOT", base)
            .env("DORT_NPM_CRASH_AT", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(88), "{phase}");
        let calls = fs::read_to_string(host.home.join("calls")).unwrap();
        let a = install::diagnose(&host).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].unresolved, phase != "OutcomeDurable");
        assert_eq!(prefix.is_dir(), phase != "IntentDurable");
        assert_eq!(
            prefix.join("bin/demo").exists(),
            matches!(phase, "PostInspection" | "OutcomeDurable")
        );
        assert_eq!(calls, fs::read_to_string(host.home.join("calls")).unwrap());
        assert!(dort_core::journal::diagnose(&host).unwrap().is_empty());
        let _keep_disposable_home = f.home;
    }
}
#[test]
fn mixed_legacy_and_npm_attempts_share_only_the_explicit_tools_guard() {
    let f = Fixture::new();
    let id = f.pending();
    let repo = f.home.repository("legacy");
    fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='legacy'\nbinary='legacy'\n[tools.install]\nmacos={provider='mise',package='legacy',version='1.2.3'}\nubuntu={provider='mise',package='legacy',version='1.2.3'}\n").unwrap();
    RepositoryRegistry::register(&f.home.host, &f.lock, &repo).unwrap();
    let request = install::Request {
        repository: repo,
        profiles: vec![],
        tool: "legacy".into(),
        executable: f.mise.clone(),
        search_dirs: f.request().search_dirs,
        tool_search_dirs: f.request().tool_search_dirs,
        bound_tools: BTreeSet::from(["legacy".into()]),
    };
    let report = inspection::Preview::new(
        Query::MiseInstalled,
        f.mise.clone(),
        f.home.host.home.clone(),
        request.search_dirs.clone(),
    )
    .unwrap()
    .approve()
    .inspect();
    let preview = install::prepare(&f.home.host, request.clone(), &report).unwrap();
    let calls = f.calls();
    assert!(install::execute(&f.home.host, &f.lock, preview.approve(), |_| {}).is_err());
    assert_eq!(calls, f.calls());
    let p = install::prepare_close(
        &f.home.host,
        install::CloseRequest {
            id,
            executable: None,
            search_dirs: vec![],
            npm_binding: None,
        },
    )
    .unwrap();
    install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    let p = install::prepare(&f.home.host, request, &report).unwrap();
    let r = install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).unwrap();
    let bytes = fs::read_to_string(
        f.attempt(r.attempt_id.as_ref().unwrap())
            .join("intent.json"),
    )
    .unwrap();
    let envelope: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["schema"], 1);
    assert!(payload["scope"].get("npm").is_none());
    let a = install::diagnose(&f.home.host).unwrap();
    assert_eq!(a.len(), 2);
    assert!(a.iter().all(|a| !a.unresolved));
}
#[test]
fn damaged_attempt_and_changed_cli_or_head_cannot_be_closed_with_stale_approval() {
    let f = Fixture::new();
    let id = f.pending();
    let request = install::CloseRequest {
        id: id.clone(),
        executable: None,
        search_dirs: vec![],
        npm_binding: None,
    };
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    let calls = f.calls();
    fs::write(&f.cli, "new CLI").unwrap();
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    assert_eq!(calls, f.calls());
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    fs::write(f.attempt(&id).join("unexpected"), "metadata").unwrap();
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    let p = install::prepare_close(&f.home.host, request).unwrap();
    f.mark("unavailable");
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
}
