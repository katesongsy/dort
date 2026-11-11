//! Fake programs only: these tests never invoke sudo/APT or become root.
#[path = "support/apt_installation_boundaries.rs"]
mod boundaries;
mod support;
use dort_core::{
    local::RepositoryRegistry,
    platform::Platform,
    state::WriterLock,
    tools::{
        VersionPolicy,
        inspection::{self, Query},
        install::{self, Event, PackageState, ProcessOutcome, apt, npm},
    },
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
const MANIFEST: &str = "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nubuntu={provider='apt',package='demo',version='latest'}\n";
const DPKG: &str = r##"#!/bin/sh
printf 'dpkg %s\n' "$*" >> "$HOME/calls"
[ "$PWD" = / ] && [ "$LC_ALL" = C ] && [ -z "$MISE_YES" ] || exit 90
if [ -f "$HOME/bad-inventory" ]; then printf 'other\t1\tunpacked\tok\n';
elif [ -f "$HOME/installed" ]; then printf 'demo\t2.7-1ubuntu1\tinstalled\tok\n'; fi
"##;
const CACHE: &str = r##"#!/bin/sh
printf 'cache %s\n' "$*" >> "$HOME/calls"
[ "$*" = 'policy demo' ] && [ "$LC_ALL" = C ] && [ "$PWD" = / ] && [ -z "$MISE_YES" ] || exit 90
if [ -f "$HOME/bad-candidate" ]; then printf 'PRIVATE_CANDIDATE\n'; exit 0; fi
printf 'demo:\n  Installed: (none)\n  Candidate: 2.7-1ubuntu1\n  Version table:\n     2.7-1ubuntu1 500\n        500 https://PRIVATE_ENDPOINT/ SECRET_DESCRIPTION\n'
"##;
const SUDO: &str = r##"#!/bin/sh
printf 'sudo %s\n' "$*" >> "$HOME/calls"
[ "$PWD" = / ] && [ "$LC_ALL" = C ] && [ -z "$MISE_YES" ] && [ -z "$SSH_AUTH_SOCK" ] && [ "$XDG_STATE_HOME" = "$HOME/.local/state" ] || exit 90
[ "$1" = -n ] && [ "$2" = -T ] && [ "$3" = 600 ] && [ "$4" = -u ] && [ "$5" = root ] && [ "$6" = DEBIAN_FRONTEND=noninteractive ] || exit 91
if [ -f "$HOME/denied" ]; then printf PRIVATE_POLICY >&2; exit 1; fi
shift 6
export DEBIAN_FRONTEND=noninteractive
"$@"
"##;
const GET: &str = r##"#!/bin/sh
printf 'get %s\n' "$*" >> "$HOME/calls"
[ "$DEBIAN_FRONTEND" = noninteractive ] && [ "$PWD" = / ] || exit 90
[ "$*" = '--assume-yes --no-remove --no-upgrade --no-install-recommends -o Dpkg::Use-Pty=0 -o DPkg::Options::=--force-confdef -o DPkg::Options::=--force-confold install demo' ] || exit 91
printf native-effect > "$HOME/native-effect"
printf PRIVATE_OUTPUT; printf PRIVATE_ERROR >&2
if [ -f "$HOME/fail" ]; then exit 4; fi
if [ -f "$HOME/change-program" ]; then printf '\n# changed\n' >> "$HOME/bin/dpkg-query"; fi
printf installed > "$HOME/installed"
"##;
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    lock: WriterLock,
}
impl Fixture {
    fn new() -> Self {
        let mut home = support::TestHome::new();
        home.host.platform = Platform::Ubuntu;
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        for d in ["bin", "lookup"] {
            fs::create_dir(home.host.home.join(d)).unwrap();
        }
        for (name, body) in [
            ("dpkg-query", DPKG),
            ("apt-cache", CACHE),
            ("apt-get", GET),
            ("sudo", SUDO),
        ] {
            let p = home.host.home.join("bin").join(name);
            fs::write(&p, body).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let lock = WriterLock::acquire(&home.host).unwrap();
        RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self { home, repo, lock }
    }
    fn program(&self, name: &str) -> PathBuf {
        self.home.host.home.join("bin").join(name)
    }
    fn request(&self) -> install::Request {
        install::Request {
            repository: self.repo.clone(),
            profiles: vec![],
            tool: "demo".into(),
            executable: self.program("dpkg-query"),
            search_dirs: vec![self.home.host.home.join("bin")],
            tool_search_dirs: vec![self.home.host.home.join("lookup")],
            bound_tools: BTreeSet::from(["demo".into()]),
        }
    }
    fn scope(&self) -> apt::Scope {
        apt::Scope {
            apt_cache: self.program("apt-cache"),
            apt_get: self.program("apt-get"),
            sudo: self.program("sudo"),
            policy: VersionPolicy::Latest {},
            catalogs: vec![npm::Catalog {
                query: Query::DpkgPackages,
                executable: self.program("dpkg-query"),
                search_dirs: self.request().search_dirs,
                npm_binding: None,
                tools: BTreeSet::from(["demo".into()]),
            }],
        }
    }
    fn report(&self) -> inspection::Report {
        inspection::Preview::new(
            Query::DpkgPackages,
            self.program("dpkg-query"),
            self.home.host.home.clone(),
            self.request().search_dirs,
        )
        .unwrap()
        .approve()
        .inspect()
    }
    fn preview(&self) -> install::Preview {
        apt::prepare(
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
    fn pending(&self) -> String {
        let p = self.preview();
        assert!(
            install::execute(&self.home.host, &self.lock, p.approve(), |e| {
                if e == Event::IntentDurable {
                    fs::write(self.repo.join("dort.toml"), "invalid").unwrap();
                }
            })
            .is_err()
        );
        install::diagnose(&self.home.host).unwrap()[0].id.clone()
    }
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
fn process_free_review_and_fixed_root_protocol_store_only_schema4_metadata() {
    let f = Fixture::new();
    let r = f.report();
    let calls = f.calls();
    let p = apt::prepare(&f.home.host, f.request(), f.scope(), &[&r]).unwrap();
    assert_eq!(calls, f.calls());
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
    assert_eq!(p.review().fresh_probes, [vec!["policy", "demo"]]);
    assert_eq!(p.review().fresh_inspections.len(), 1);
    for warning in [
        "ROOT RISK",
        "CAN EXECUTE",
        "services",
        "outside HOME",
        "NOT root isolation",
        "keeps waiting joined",
        "No password",
    ] {
        assert!(p.review().warning.contains(warning), "{warning}");
    }
    let out = f.execute(p).unwrap();
    assert_eq!(out.outcome.unwrap().process, ProcessOutcome::Success);
    assert_eq!(
        out.outcome.unwrap().package,
        PackageState::ProviderCurrentPresent
    );
    assert_eq!(out.observed_versions, ["2.7-1ubuntu1"]);
    assert!(!out.readiness_verified);
    assert!(f.home.host.home.join("native-effect").exists());
    let calls = f.calls();
    assert_eq!(calls.matches("sudo ").count(), 1);
    assert_eq!(calls.matches("cache ").count(), 1);
    assert_eq!(calls.matches("dpkg ").count(), 3);
    let id = out.attempt_id.unwrap();
    for name in ["intent.json", "outcome.json"] {
        let bytes = fs::read_to_string(
            f.home
                .host
                .state_dir
                .join("tools-attempts")
                .join(&id)
                .join(name),
        )
        .unwrap();
        assert!(!bytes.contains("PRIVATE") && !bytes.contains("SECRET"));
        let envelope: serde_json::Value = serde_json::from_str(&bytes).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["schema"], 4);
    }
    let a = install::diagnose(&f.home.host).unwrap();
    assert!(!a[0].unresolved);
    assert_eq!(a[0].observed_versions, ["2.7-1ubuntu1"]);
}
#[test]
fn newly_present_is_noop_without_candidate_or_privileged_request() {
    let f = Fixture::new();
    let p = f.preview();
    f.flag("installed");
    let out = f.execute(p).unwrap();
    assert!(out.no_op && out.attempt_id.is_none());
    assert!(!f.calls().contains("cache ") && !f.calls().contains("sudo "));
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
}
#[test]
fn incomplete_global_dpkg_and_invalid_candidate_fail_closed() {
    for flag in ["bad-inventory", "bad-candidate"] {
        let f = Fixture::new();
        let p = f.preview();
        f.flag(flag);
        assert!(f.execute(p).is_err());
        assert!(!f.calls().contains("sudo "));
        assert!(!f.home.host.state_dir.join("tools-attempts").exists());
    }
    let f = Fixture::new();
    f.flag("bad-inventory");
    assert!(apt::prepare(&f.home.host, f.request(), f.scope(), &[&f.report()]).is_err());
}
#[test]
fn noninteractive_policy_failure_and_native_failure_still_observe_dpkg() {
    for (flag, effect) in [("denied", false), ("fail", true)] {
        let f = Fixture::new();
        let p = f.preview();
        f.flag(flag);
        let out = f.execute(p).unwrap();
        assert_eq!(out.outcome.unwrap().process, ProcessOutcome::Failed);
        assert_eq!(out.outcome.unwrap().package, PackageState::Absent);
        assert!(out.possible_partial_effects && !out.readiness_verified);
        assert_eq!(f.home.host.home.join("native-effect").exists(), effect);
        assert_eq!(f.calls().matches("sudo ").count(), 1);
        assert_eq!(f.calls().matches("dpkg ").count(), 3);
        assert!(!serde_json::to_string(&out).unwrap().contains("PRIVATE"));
    }
}
#[test]
fn changed_program_prevents_postquery_and_never_reports_exact() {
    let f = Fixture::new();
    let p = f.preview();
    f.flag("change-program");
    let out = f.execute(p).unwrap();
    assert_eq!(out.outcome.unwrap().process, ProcessOutcome::Success);
    assert_eq!(out.outcome.unwrap().package, PackageState::Unavailable);
    assert!(out.observed_versions.is_empty() && out.possible_partial_effects);
    assert_eq!(f.calls().matches("dpkg ").count(), 2);
}
#[test]
fn all_program_and_lookup_identities_bind_before_any_fresh_process() {
    for name in ["dpkg-query", "apt-cache", "apt-get", "sudo", "lookup"] {
        let f = Fixture::new();
        let p = f.preview();
        let calls = f.calls();
        let path = if name == "lookup" {
            f.home.host.home.join(name)
        } else {
            f.program(name)
        };
        fs::rename(&path, path.with_extension("old")).unwrap();
        if name == "lookup" {
            fs::create_dir(path).unwrap();
        } else {
            fs::write(&path, "#!/bin/sh\nexit 99\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(f.execute(p).is_err());
        assert_eq!(calls, f.calls());
    }
}
#[test]
fn unsafe_missing_programs_names_policies_and_platform_are_process_free_refusals() {
    for case in 0..7 {
        let mut f = Fixture::new();
        let report = f.report();
        let calls = f.calls();
        let mut scope = f.scope();
        match case {
            0 => scope.apt_get = f.program("missing"),
            1 => {
                fs::remove_file(f.program("sudo")).unwrap();
                symlink(f.program("apt-get"), f.program("sudo")).unwrap();
            }
            2 => {
                fs::hard_link(f.program("apt-cache"), f.program("alias")).unwrap();
            }
            3 => fs::write(
                f.repo.join("dort.toml"),
                MANIFEST.replace("package='demo'", "package='demo.*'"),
            )
            .unwrap(),
            4 => {
                scope.policy = VersionPolicy::Exact {
                    version: "1.2".into(),
                };
                fs::write(
                    f.repo.join("dort.toml"),
                    MANIFEST.replace("version='latest'", "version='1.2'"),
                )
                .unwrap();
            }
            5 => f.home.host.platform = Platform::Macos,
            _ => scope.catalogs[0].query = Query::MiseInstalled,
        }
        assert!(apt::prepare(&f.home.host, f.request(), scope, &[&report]).is_err());
        assert_eq!(calls, f.calls());
    }
}
#[test]
fn final_independent_binary_and_source_guards_leave_unresolved_intent() {
    for case in 0..3 {
        let f = Fixture::new();
        let p = f.preview();
        assert!(
            install::execute(&f.home.host, &f.lock, p.approve(), |e| {
                if e == Event::Installing {
                    match case {
                        0 => {
                            let path = f.home.host.home.join("lookup/demo");
                            fs::write(&path, "NOT_EXECUTED").unwrap();
                            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
                        }
                        1 => fs::write(f.program("sudo"), "changed").unwrap(),
                        _ => fs::write(f.repo.join("dort.toml"), "invalid").unwrap(),
                    }
                }
            })
            .is_err()
        );
        assert!(!f.calls().contains("sudo "));
        assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
    }
}
#[test]
fn pending_guard_runs_before_queries_and_known_closure_needs_only_dpkg() {
    let f = Fixture::new();
    let preview = f.preview();
    let id = f.pending();
    let calls = f.calls();
    assert!(f.execute(preview).is_err());
    assert_eq!(calls, f.calls());
    for name in ["sudo", "apt-cache", "apt-get"] {
        fs::remove_file(f.program(name)).unwrap();
    }
    let p = install::prepare_close(&f.home.host, close_request(id)).unwrap();
    assert_eq!(p.review().query.query, Query::DpkgPackages);
    assert!(p.review().fresh_probes.is_empty());
    assert_eq!(calls, f.calls());
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(a.explicitly_closed && !a.unresolved);
    assert_eq!(f.calls().matches("sudo ").count(), 0);
}
#[test]
fn damaged_closure_dpkg_fallback_binds_head_and_requires_complete_inventory() {
    let f = Fixture::new();
    let id = f.pending();
    let intent = f
        .home
        .host
        .state_dir
        .join("tools-attempts")
        .join(&id)
        .join("intent.json");
    fs::remove_file(&intent).unwrap();
    let mut request = close_request(id.clone());
    request.apt = true;
    request.executable = Some(f.program("dpkg-query"));
    request.search_dirs = f.request().search_dirs;
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    assert!(!p.review().original_scope_known);
    fs::write(&intent, "damaged").unwrap();
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    f.flag("bad-inventory");
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    fs::remove_file(f.home.host.home.join("bad-inventory")).unwrap();
    let p = install::prepare_close(&f.home.host, request).unwrap();
    assert!(
        install::close(&f.home.host, &f.lock, p.approve())
            .unwrap()
            .explicitly_closed
    );
}
