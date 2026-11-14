mod support;
use dort_core::{
    context::HostContext,
    journal,
    local::RepositoryRegistry,
    state::WriterLock,
    tools::{
        inspection,
        install::{self, BinaryState, Event, PackageState, ProcessOutcome, Request},
    },
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::{Command, Stdio},
};

const MANIFEST: &str = "# PRIVATE_MANIFEST_BODY\nschema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='mise',package='demo',version='1.2.3'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n";
const PROGRAM: &str = r##"#!/bin/sh
if [ "$1" = ls ]; then
  printf q >> "$HOME/calls"
  if [ -f "$HOME/unavailable" ]; then printf PRIVATE_PROVIDER_OUTPUT; exit 1; fi
  if [ -f "$HOME/installed" ]; then printf '{"demo":[{"version":"1.2.3","installed":true}]}'
  elif [ -f "$HOME/conflict" ]; then printf '{"demo":[{"version":"9.0.0","installed":true}]}'
  else printf '{}'; fi
  exit 0
fi
[ "$#" = 6 ] && [ "$1" = --no-config ] && [ "$2" = --no-env ] && [ "$3" = --no-hooks ] && [ "$4" = install ] && [ "$5" = --jobs=1 ] && [ "$6" = demo@1.2.3 ] || exit 90
[ "$MISE_NO_CONFIG" = 1 ] && [ "$MISE_NO_ENV" = 1 ] && [ "$MISE_NO_HOOKS" = 1 ] && [ "$MISE_YES" = 1 ] || exit 91
[ "$PWD" = / ] && [ "$XDG_DATA_HOME" = "$HOME/.local/share" ] && [ -z "$DORT_TEST_PRIVATE_PARENT_SENTINEL" ] && [ -z "$NODE_OPTIONS" ] || exit 92
printf i >> "$HOME/calls"
printf PRIVATE_PROVIDER_OUTPUT
printf PRIVATE_PROVIDER_STDERR >&2
if [ -f "$HOME/fail" ]; then exit 3; fi
if [ -f "$HOME/unverified" ]; then exit 0; fi
printf installed > "$HOME/installed"
"##;
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    executable: PathBuf,
    lock: WriterLock,
}
impl Fixture {
    fn new() -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        fs::create_dir(home.host.home.join("bin")).unwrap();
        let executable = home.host.home.join("mise");
        fs::write(&executable, PROGRAM).unwrap();
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
    fn request(&self) -> Request {
        Request {
            repository: self.repo.clone(),
            profiles: vec![],
            tool: "demo".into(),
            executable: self.executable.clone(),
            search_dirs: vec![self.home.host.home.join("no-helper-programs")],
            tool_search_dirs: vec![self.home.host.home.join("bin")],
            bound_tools: BTreeSet::from(["demo".into()]),
        }
    }
    fn inspect(&self) -> inspection::Report {
        let r = self.request();
        inspection::Preview::new(
            inspection::Query::MiseInstalled,
            r.executable,
            self.home.host.home.clone(),
            r.search_dirs,
        )
        .unwrap()
        .approve()
        .inspect()
    }
    fn preview(&self) -> install::Preview {
        install::prepare(&self.home.host, self.request(), &self.inspect()).unwrap()
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
    fn mark(&self, name: &str) {
        fs::write(self.home.host.home.join(name), "fixture").unwrap();
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.host.home.join("calls")).unwrap_or_default()
    }
    fn attempt_dir(&self, id: &str) -> PathBuf {
        self.home.host.state_dir.join("tools-attempts").join(id)
    }
    fn incomplete(&self) -> String {
        let p = self.preview();
        let result = install::execute(&self.home.host, &self.lock, p.approve(), |event| {
            if event == Event::IntentDurable {
                fs::write(&self.executable, "changed executable").unwrap();
            }
        });
        assert!(result.is_err());
        fs::write(&self.executable, PROGRAM).unwrap();
        let attempts = install::diagnose(&self.home.host).unwrap();
        assert!(attempts[0].unresolved);
        attempts[0].id.clone()
    }
    fn close(&self, id: &str) -> install::Attempt {
        let p = install::prepare_close(
            &self.home.host,
            install::CloseRequest {
                brew_prefix: None,
                apt: false,
                id: id.into(),
                npm_binding: None,
                executable: None,
                search_dirs: vec![],
            },
        )
        .unwrap();
        install::close(&self.home.host, &self.lock, p.approve()).unwrap()
    }
}
#[test]
fn a_binary_appearing_after_durable_intent_refuses_spawn_and_retains_unresolved_metadata() {
    let f = Fixture::new();
    let preview = f.preview();
    let result = install::execute(&f.home.host, &f.lock, preview.approve(), |event| {
        if event == Event::Installing {
            fs::write(f.home.host.home.join("bin/demo"), "unattributed candidate").unwrap();
        }
    });
    assert!(result.is_err());
    assert_eq!(f.calls(), "qq");
    let attempts = install::diagnose(&f.home.host).unwrap();
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].unresolved);
    assert!(!f.home.host.home.join("installed").exists());
}

#[test]
fn process_free_prepare_and_fixed_install_have_private_durable_metadata_only() {
    let f = Fixture::new();
    assert!(install::diagnose(&f.home.host).unwrap().is_empty());
    let before = f.inspect();
    let p = install::prepare(&f.home.host, f.request(), &before).unwrap();
    assert_eq!(f.calls(), "q");
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
    assert_eq!(p.review().timeout_seconds, 600);
    assert_eq!(p.review().arguments.last().unwrap(), "demo@1.2.3");
    let mut phases = vec![];
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |event| {
        phases.push(event)
    })
    .unwrap();
    assert_eq!(f.calls(), "qqiq");
    assert_eq!(
        phases,
        [
            Event::FreshValidation,
            Event::FreshInspection,
            Event::IntentDurable,
            Event::Installing,
            Event::PostInspection,
            Event::OutcomeDurable
        ]
    );
    let outcome = result.outcome.unwrap();
    assert_eq!(outcome.process, ProcessOutcome::Success);
    assert_eq!(outcome.package, PackageState::ExactPresent);
    assert_eq!(outcome.binary, BinaryState::Missing);
    assert!(!result.readiness_verified && !result.no_op);
    let attempts = install::diagnose(&f.home.host).unwrap();
    assert!(!attempts[0].unresolved && !attempts[0].explicitly_closed);
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    for entry in fs::read_dir(f.attempt_dir(result.attempt_id.as_ref().unwrap())).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(
            entry.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        let text = fs::read_to_string(entry.path()).unwrap();
        for private in [
            "PRIVATE_MANIFEST_BODY",
            "PRIVATE_PROVIDER_OUTPUT",
            "PRIVATE_PROVIDER_STDERR",
            "raw_stdout",
            "raw_stderr",
        ] {
            assert!(!text.contains(private));
        }
    }
}
#[test]
fn schema_one_raw_payload_checksums_and_head_bound_closures_are_not_migrated() {
    let f = Fixture::new();
    let id = f.incomplete();
    let path = f.attempt_dir(&id).join("intent.json");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["schema"], 1);
    assert!(payload["scope"].get("npm").is_none() && payload["sources"].get("npm").is_none());
    let raw = format!(" \n{}\n ", serde_json::to_string_pretty(&payload).unwrap());
    use sha2::{Digest, Sha256};
    envelope["sha256"] = format!("{:x}", Sha256::digest(raw.as_bytes())).into();
    envelope["payload"] = raw.clone().into();
    let bytes = serde_json::to_vec(&envelope).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(install::diagnose(&f.home.host).unwrap()[0].scope.is_some());
    assert!(f.close(&id).explicitly_closed);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    // Same typed JSON, different raw payload, stale checksum: reject and invalidate closure.
    envelope["payload"] = format!("{raw}\n").into();
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let a = install::diagnose(&f.home.host).unwrap();
    assert!(a[0].unresolved && a[0].scope.is_none());
}
#[test]
fn fresh_satisfaction_is_noop_and_fresh_conflict_or_unmanaged_binary_refuses() {
    for state in ["installed", "conflict", "candidate", "unavailable"] {
        let f = Fixture::new();
        let p = f.preview();
        if state == "candidate" {
            fs::write(f.home.host.home.join("bin/demo"), "never execute").unwrap();
        } else {
            f.mark(state);
        }
        let result = install::execute(&f.home.host, &f.lock, p.approve(), |_| {});
        if state == "installed" {
            let result = result.unwrap();
            assert!(result.no_op);
            assert_eq!(result.summary().status, install::Status::NoOp);
            assert_eq!(result.summary().binary, None);
            assert_eq!(
                serde_json::to_value(&result).unwrap()["summary"]["status"],
                "no_op"
            );
        } else {
            assert!(result.is_err());
        }
        assert_eq!(f.calls(), "qq");
        assert!(!f.home.host.state_dir.join("tools-attempts").exists());
    }
}
#[test]
fn stale_manifest_executable_registration_profile_and_wrong_lock_refuse_before_query() {
    for changed in ["manifest", "executable", "registration", "profiles", "lock"] {
        let f = Fixture::new();
        let p = f.preview();
        match changed {
            "manifest" => {
                fs::write(f.repo.join("dort.toml"), format!("{MANIFEST}\n# changed\n")).unwrap()
            }
            "executable" => fs::write(&f.executable, format!("{PROGRAM}\n# changed\n")).unwrap(),
            "registration" => fs::write(
                f.home.host.local_config_path(),
                "schema=1\n[repositories]\n",
            )
            .unwrap(),
            "profiles" => {
                let mut local = dort_core::local::LocalConfig::load(&f.home.host).unwrap();
                local.active_profiles = vec!["missing".into()];
                fs::write(
                    f.home.host.local_config_path(),
                    toml::to_string(&local).unwrap(),
                )
                .unwrap();
            }
            _ => (),
        }
        let other = support::TestHome::new();
        let wrong_lock = WriterLock::acquire(&other.host).unwrap();
        let lock = if changed == "lock" {
            &wrong_lock
        } else {
            &f.lock
        };
        assert!(install::execute(&f.home.host, lock, p.approve(), |_| {}).is_err());
        assert_eq!(f.calls(), "q");
    }
}
#[test]
fn failed_or_unverified_install_records_observed_partial_outcome_without_replay() {
    for state in ["fail", "unverified"] {
        let f = Fixture::new();
        f.mark(state);
        let r = f.execute();
        let o = r.outcome.unwrap();
        assert_eq!(
            o.process,
            if state == "fail" {
                ProcessOutcome::Failed
            } else {
                ProcessOutcome::Success
            }
        );
        assert_eq!(o.package, PackageState::Absent);
        assert!(r.possible_partial_effects && !r.readiness_verified);
        assert!(!install::diagnose(&f.home.host).unwrap()[0].unresolved);
        assert_eq!(f.calls(), "qqiq");
        let before = f.calls();
        let _ = f.preview();
        assert_eq!(f.calls(), format!("{before}q")); // Fresh inspection only, never an installer replay.
    }
}
#[test]
fn unresolved_attempt_blocks_only_tools_installs_and_closure_is_guarded_and_fresh() {
    let f = Fixture::new();
    let id = f.incomplete();
    assert_eq!(f.calls(), "qq");
    assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
    let p = f.preview();
    assert!(install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).is_err());
    assert_eq!(f.calls(), "qqq");
    // Tools attempt state does not silently block unrelated file writers.
    let mut declaration = dort_core::Manifest::parse(MANIFEST)
        .unwrap()
        .tools
        .remove(0);
    declaration.id = "other".into();
    declaration.binary = "other".into();
    let p =
        dort_core::tools::declaration::preview(&f.home.host, &f.repo, &[], declaration).unwrap();
    let a = dort_core::tools::declaration::Approval::from_preview(p).unwrap();
    dort_core::tools::declaration::execute(&f.home.host, &f.lock, a, |_| Ok(())).unwrap();
    let before = f.calls();
    let p = install::prepare_close(
        &f.home.host,
        install::CloseRequest {
            brew_prefix: None,
            apt: false,
            id: id.clone(),
            npm_binding: None,
            executable: None,
            search_dirs: vec![],
        },
    )
    .unwrap();
    assert_eq!(f.calls(), before);
    assert!(p.review().original_scope_known);
    let a = install::close(&f.home.host, &f.lock, p.approve()).unwrap();
    assert!(a.explicitly_closed && !a.unresolved);
    assert_eq!(f.calls(), format!("{before}q"));
    assert!(
        install::prepare_close(
            &f.home.host,
            install::CloseRequest {
                brew_prefix: None,
                apt: false,
                id: id.clone(),
                npm_binding: None,
                executable: None,
                search_dirs: vec![]
            }
        )
        .is_err()
    );
    // Later metadata damage invalidates closure. A new explicit fresh closure binds it,
    // without overwriting the damaged artifact or pretending the install succeeded.
    fs::write(
        f.attempt_dir(&id).join("close-0000.json"),
        "PRIVATE_BAD_CLOSURE",
    )
    .unwrap();
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
    assert!(!f.close(&id).unresolved);
    assert_eq!(
        fs::read_to_string(f.attempt_dir(&id).join("close-0000.json")).unwrap(),
        "PRIVATE_BAD_CLOSURE"
    );
}
#[test]
fn damaged_intent_needs_explicit_scope_and_changed_head_or_failed_query_cannot_close() {
    let f = Fixture::new();
    let id = f.incomplete();
    fs::write(f.attempt_dir(&id).join("intent.json"), "PRIVATE_BAD_INTENT").unwrap();
    assert!(install::diagnose(&f.home.host).unwrap()[0].scope.is_none());
    assert!(
        install::prepare_close(
            &f.home.host,
            install::CloseRequest {
                brew_prefix: None,
                apt: false,
                id: id.clone(),
                npm_binding: None,
                executable: None,
                search_dirs: vec![]
            }
        )
        .is_err()
    );
    let request = install::CloseRequest {
        brew_prefix: None,
        apt: false,
        id: id.clone(),
        npm_binding: None,
        executable: Some(f.executable.clone()),
        search_dirs: f.request().search_dirs,
    };
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    assert!(!p.review().original_scope_known);
    fs::write(f.attempt_dir(&id).join("unexpected"), "metadata").unwrap();
    let before = f.calls();
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    assert_eq!(f.calls(), before);
    let p = install::prepare_close(&f.home.host, request.clone()).unwrap();
    f.mark("unavailable");
    assert!(install::close(&f.home.host, &f.lock, p.approve()).is_err());
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
    fs::remove_file(f.home.host.home.join("unavailable")).unwrap();
    let p = install::prepare_close(&f.home.host, request).unwrap();
    assert!(
        !install::close(&f.home.host, &f.lock, p.approve())
            .unwrap()
            .unresolved
    );
}
#[test]
fn unsafe_provider_paths_and_unavailable_tool_directories_are_never_absence() {
    let f = Fixture::new();
    let report = f.inspect();
    let link = f.home.host.home.join("mise-link");
    symlink(&f.executable, &link).unwrap();
    let mut request = f.request();
    request.executable = link;
    assert!(install::prepare(&f.home.host, request, &report).is_err());
    let mut request = f.request();
    request.tool_search_dirs = vec![f.home.host.home.join("missing-directory")];
    assert!(install::prepare(&f.home.host, request, &report).is_err());
    let mut request = f.request();
    request.bound_tools.clear();
    assert!(install::prepare(&f.home.host, request, &report).is_err());
    assert_eq!(f.calls(), "q");
}
#[test]
fn missing_latest_prefix_mismatch_and_prerequisite_blocked_are_non_executable() {
    for replacement in ["version='latest'", "version='1'", "version='1.2'", ""] {
        let f = Fixture::new();
        fs::write(
            f.repo.join("dort.toml"),
            MANIFEST
                .replace("version='1.2.3'", replacement)
                .replace(",}", "}"),
        )
        .unwrap();
        assert!(install::prepare(&f.home.host, f.request(), &f.inspect()).is_err());
    }
    let f = Fixture::new();
    f.mark("conflict");
    assert!(install::prepare(&f.home.host, f.request(), &f.inspect()).is_err());
    fs::remove_file(f.home.host.home.join("conflict")).unwrap();
    let manifest = MANIFEST.replace("binary='demo'", "binary='demo'\nrequires=['dependency']");
    fs::write(f.repo.join("dort.toml"), format!("{manifest}\n[[tools]]\nid='dependency'\nbinary='dependency'\n[tools.install]\nmacos={{provider='mise',package='dependency',version='1.2.3'}}\nubuntu={{provider='mise',package='dependency',version='1.2.3'}}\n")).unwrap();
    assert!(install::prepare(&f.home.host, f.request(), &f.inspect()).is_err());
}

#[test]
fn intent_guard_and_outcome_failure_never_claim_unpersisted_success() {
    let f = Fixture::new();
    let p = f.preview();
    let outside = f.home.host.home.join("outside");
    fs::create_dir(&outside).unwrap();
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |event| {
        if event == Event::FreshInspection {
            symlink(&outside, f.home.host.state_dir.join("tools-attempts")).unwrap();
        }
    });
    assert!(result.is_err());
    assert_eq!(f.calls(), "qq");
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);

    let f = Fixture::new();
    let p = f.preview();
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |event| {
        if event == Event::PostInspection {
            let id = install::diagnose(&f.home.host).unwrap().remove(0).id;
            let path = f.attempt_dir(&id).join("outcome.json");
            fs::write(&path, "PRIVATE_PARTIAL_RECORD").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
    });
    let error = result.err().unwrap().to_string();
    assert!(error.contains("no durable outcome"));
    assert!(!error.contains("PRIVATE_PARTIAL_RECORD"));
    assert_eq!(f.calls(), "qqiq");
    let attempt = install::diagnose(&f.home.host).unwrap().remove(0);
    assert!(attempt.unresolved);
    assert!(!f.close(&attempt.id).unresolved);
}

#[test]
fn postinspection_never_runs_a_provider_whose_source_binding_changed() {
    let f = Fixture::new();
    let p = f.preview();
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |event| {
        if event == Event::PostInspection {
            fs::write(
                &f.executable,
                r#"#!/bin/sh
printf BAD_REEXEC >> "$HOME/calls"
"#,
            )
            .unwrap();
        }
    })
    .unwrap();
    assert_eq!(f.calls(), "qqi");
    assert_eq!(result.outcome.unwrap().package, PackageState::Unavailable);
    assert!(result.possible_partial_effects);
}

#[test]
fn malformed_file_journals_block_installs_before_a_provider_query() {
    let f = Fixture::new();
    let p = f.preview();
    let repository_id = f.request().repository;
    let registered = dort_core::local::LocalConfig::load(&f.home.host).unwrap();
    let id = registered
        .repositories
        .iter()
        .find(|(_, r)| r.root == repository_id)
        .unwrap()
        .0;
    let parent = dort_core::local::repository_state_path(&f.home.host, id)
        .unwrap()
        .join("transactions");
    let path = parent.join("a".repeat(64));
    fs::create_dir_all(&path).unwrap();
    for directory in [
        path,
        parent.clone(),
        parent.parent().unwrap().to_path_buf(),
        f.home.host.state_dir.join("repositories"),
    ] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert!(install::execute(&f.home.host, &f.lock, p.approve(), |_| {}).is_err());
    assert_eq!(f.calls(), "q");
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());
}

#[test]
fn install_crash_child() {
    let Some(base) = std::env::var_os("DORT_INSTALL_CRASH_ROOT") else {
        return;
    };
    let base = PathBuf::from(base);
    let platform = if cfg!(target_os = "macos") {
        dort_core::platform::Platform::Macos
    } else {
        dort_core::platform::Platform::Ubuntu
    };
    let host = HostContext::new(
        base.join("home"),
        None,
        None,
        platform,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let request = Request {
        repository: base.join("repo"),
        profiles: vec![],
        tool: "demo".into(),
        executable: host.home.join("mise"),
        search_dirs: vec![host.home.join("no-helper-programs")],
        tool_search_dirs: vec![host.home.join("bin")],
        bound_tools: BTreeSet::from(["demo".into()]),
    };
    let lock = WriterLock::acquire(&host).unwrap();
    let report = inspection::Preview::new(
        inspection::Query::MiseInstalled,
        request.executable.clone(),
        host.home.clone(),
        request.search_dirs.clone(),
    )
    .unwrap()
    .approve()
    .inspect();
    let p = install::prepare(&host, request, &report).unwrap();
    let goal = std::env::var("DORT_INSTALL_CRASH_AT").unwrap();
    install::execute(&host, &lock, p.approve(), |event| {
        if format!("{event:?}") == goal {
            std::process::exit(88);
        }
    })
    .unwrap();
    panic!("crash boundary did not run");
}
#[test]
fn process_exit_leaves_explicit_attempts_and_never_replays_or_rolls_back() {
    for phase in [
        "IntentDurable",
        "Installing",
        "PostInspection",
        "OutcomeDurable",
    ] {
        let f = Fixture::new();
        let base = f.home.sandbox.path().canonicalize().unwrap();
        // Fixture registration is durable; release parent lock before isolated child.
        let host = f.home.host.clone();
        let repo = f.repo.clone();
        let executable = f.executable.clone();
        let home = f.home;
        drop(f.lock);
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "install_crash_child", "--test-threads=1"])
            .env_clear()
            .env("DORT_INSTALL_CRASH_ROOT", base)
            .env("DORT_INSTALL_CRASH_AT", phase)
            .env("DORT_TEST_PRIVATE_PARENT_SENTINEL", "PRIVATE_PARENT_VALUE")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(88), "{phase}");
        let a = install::diagnose(&host).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].unresolved, phase != "OutcomeDurable");
        let installed = host.home.join("installed").exists();
        assert_eq!(
            installed,
            matches!(phase, "PostInspection" | "OutcomeDurable")
        );
        let before = fs::read_to_string(host.home.join("calls")).unwrap();
        assert!(journal::diagnose(&host).unwrap().is_empty());
        assert_eq!(fs::read_to_string(host.home.join("calls")).unwrap(), before);
        assert_eq!(
            fs::read_to_string(repo.join("dort.toml")).unwrap(),
            MANIFEST
        );
        assert_eq!(fs::read_to_string(executable).unwrap(), PROGRAM);
        drop(home);
    }
}
