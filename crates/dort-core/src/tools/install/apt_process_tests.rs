//! Nonprivileged fake supervisors exercise ownership, not real root containment.
use super::*;
use std::{fs, os::unix::fs::PermissionsExt, time::Instant};
fn fixture(body: &str) -> (tempfile::TempDir, super::super::Scope) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let executable = home.join("fake-supervisor");
    fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let scope = super::super::Scope {
        npm: None,
        brew: None,
        apt: Some(Scope {
            apt_cache: executable.clone(),
            apt_get: executable.clone(),
            sudo: executable.clone(),
            policy: VersionPolicy::Latest {},
            catalogs: vec![],
        }),
        platform: crate::platform::Platform::Ubuntu,
        architecture: crate::platform::Architecture::X86_64,
        repository: home.clone(),
        repository_id: "a".repeat(64),
        profiles: BTreeSet::from(["base".into()]),
        tool: "demo".into(),
        binary: "demo".into(),
        package: "demo".into(),
        version: "latest".into(),
        executable,
        home: home.clone(),
        search_dirs: vec![home.join("no-helpers")],
        tool_search_dirs: vec![home.join("bin")],
        bound_tools: BTreeSet::from(["demo".into()]),
    };
    (temp, scope)
}
#[test]
fn supervisor_timeout_relays_term_but_never_user_group_kill() {
    let (_temp, scope) =
        fixture("trap 'printf relayed > \"$HOME/relayed\"; exit 0' TERM\nwhile :; do :; done");
    let mut notices = 0;
    assert_eq!(
        run(&scope, Duration::from_secs(1), || notices += 1),
        ProcessOutcome::Timeout
    );
    assert_eq!(notices, 1);
    assert!(scope.home.join("relayed").exists());
}
#[test]
fn ignored_relay_keeps_waiting_joined_past_requested_deadline() {
    let (_temp, scope) =
        fixture("trap '' TERM\n/bin/sleep 2\nprintf finished > \"$HOME/finished\"");
    let start = Instant::now();
    let mut notices = 0;
    assert_eq!(
        run(&scope, Duration::from_secs(1), || notices += 1),
        ProcessOutcome::Timeout
    );
    assert_eq!(notices, 1);
    assert!(start.elapsed() >= Duration::from_millis(1800));
    assert!(scope.home.join("finished").exists());
}
#[test]
fn observer_unwinding_still_joins_the_owned_fake_supervisor() {
    let (_temp, scope) =
        fixture("trap '' TERM\n/bin/sleep 2\nprintf finished > \"$HOME/finished\"");
    let start = Instant::now();
    let result = std::panic::catch_unwind(|| {
        run(&scope, Duration::from_secs(1), || {
            panic!("metadata observer failure")
        })
    });
    assert!(result.is_err());
    assert!(start.elapsed() >= Duration::from_millis(1800));
    assert!(scope.home.join("finished").exists());
}
#[test]
fn fair_large_discarded_output_and_static_noninteractive_failure() {
    let sample = "PRIVATE".repeat(40);
    let (_temp, scope) = fixture(&format!(
        "i=0; while [ \"$i\" -lt 20000 ]; do printf '{sample}'; printf '{sample}' >&2; i=$((i+1)); done"
    ));
    assert_eq!(
        run(&scope, Duration::from_secs(10), || panic!(
            "unexpected deadline"
        )),
        ProcessOutcome::Success
    );
    let (_temp, scope) = fixture("printf PRIVATE_POLICY >&2; exit 1");
    assert_eq!(
        run(&scope, Duration::from_secs(2), || panic!(
            "unexpected deadline"
        )),
        ProcessOutcome::Failed
    );
}
