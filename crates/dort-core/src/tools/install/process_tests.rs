use super::{ProcessOutcome, Scope, run};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};
fn fixture(body: &str) -> (tempfile::TempDir, Scope) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let executable = home.join("fake-mise");
    fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let scope = Scope {
        platform: crate::platform::Platform::Ubuntu,
        architecture: crate::platform::Architecture::X86_64,
        repository: home.clone(),
        repository_id: "a".repeat(64),
        profiles: BTreeSet::from(["base".into()]),
        tool: "demo".into(),
        binary: "demo".into(),
        package: "demo".into(),
        version: "1.2.3".into(),
        executable,
        home: home.clone(),
        search_dirs: vec![home.join("no-helpers")],
        tool_search_dirs: vec![home.join("bin")],
        bound_tools: BTreeSet::from(["demo".into()]),
    };
    (temp, scope)
}
#[test]
fn installer_drains_and_discards_more_than_the_query_limit_from_both_pipes() {
    let sample = "PRIVATE".repeat(40);
    let (_temp, scope) = fixture(&format!(
        "i=0; while [ \"$i\" -lt 20000 ]; do printf '{sample}'; printf '{sample}' >&2; i=$((i+1)); done"
    ));
    assert_eq!(
        run(&scope, Duration::from_secs(10)),
        ProcessOutcome::Success
    );
}
#[test]
fn deadline_includes_retained_descendant_pipes_and_joins_the_owned_group() {
    for script in [
        "(while :; do :; done) & exit 0",
        "while :; do printf PRIVATE; printf PRIVATE >&2; done",
    ] {
        let (_temp, scope) = fixture(script);
        let start = Instant::now();
        assert_eq!(
            run(&scope, Duration::from_millis(100)),
            ProcessOutcome::Timeout
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
#[test]
fn failures_and_unavailable_executables_expose_only_typed_outcomes() {
    let (_temp, mut scope) = fixture("printf PRIVATE; printf PRIVATE >&2; exit 7");
    assert_eq!(run(&scope, Duration::from_secs(2)), ProcessOutcome::Failed);
    scope.executable = scope.home.join("not-present");
    assert_eq!(
        run(&scope, Duration::from_secs(2)),
        ProcessOutcome::Unavailable
    );
}
