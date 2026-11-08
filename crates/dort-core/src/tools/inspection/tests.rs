use super::*;

#[test]
fn catalog_whitelists_metadata_and_preserves_multiple_versions() {
    let entries = catalog::parse(Query::BrewFormulae, br#"{"formulae":[{"full_name":"tap/tools/rg","installed":[{"version":"14.1.0","runtime_dependencies":["PRIVATE"]},{"version":"13.0.0"}],"description":"PRIVATE"}],"casks":[]}"#).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e.version.as_str())
            .collect::<Vec<_>>(),
        ["13.0.0", "14.1.0"]
    );
    assert!(!format!("{entries:?}").contains("PRIVATE"));
    assert!(!serde_json::to_string(&entries).unwrap().contains("PRIVATE"));
    let entries = catalog::parse(Query::MiseInstalled, br#"{"node":[{"version":"22.1.0","installed":true,"active":false,"source":{"path":"PRIVATE"}},{"version":"20.0.0","installed":true,"active":true}]}"#).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].package, "node");
    assert!(!serde_json::to_string(&entries).unwrap().contains("PRIVATE"));
}

#[test]
fn dpkg_architecture_epochs_and_removed_packages_remain_distinct() {
    let entries = catalog::parse(Query::DpkgPackages, b"zlib1g:amd64\t1:1.2.13-1~ubuntu1\tinstalled\tok\nold\t2.0\tconfig-files\tok\nmissing\t\tnot-installed\tok\n").unwrap();
    assert_eq!(
        entries,
        [Installation {
            package: "zlib1g:amd64".into(),
            version: "1:1.2.13-1~ubuntu1".into()
        }]
    );
    for state in [
        "unpacked",
        "half-installed",
        "half-configured",
        "triggers-awaited",
        "triggers-pending",
    ] {
        assert_eq!(
            catalog::parse(
                Query::DpkgPackages,
                format!("pkg\t1.0\t{state}\tok\n").as_bytes()
            ),
            Err(Failure::IncompletePackages)
        );
    }
    assert_eq!(
        catalog::parse(Query::DpkgPackages, b"pkg\t1.0\tinstalled\treinstreq\n"),
        Err(Failure::IncompletePackages)
    );
}

#[test]
fn only_valid_empty_protocols_are_healthy_empty() {
    for (query, bytes) in [
        (Query::BrewFormulae, b"{\"formulae\":[]}".as_slice()),
        (Query::MiseInstalled, b"{}"),
        (Query::DpkgPackages, b""),
    ] {
        assert!(catalog::parse(query, bytes).unwrap().is_empty());
    }
    for (query, bytes) in [
        (Query::BrewFormulae, b"{}".as_slice()),
        (
            Query::BrewFormulae,
            b"{\"formulae\":[{\"full_name\":\"foo\",\"installed\":[]}]}",
        ),
        (
            Query::MiseInstalled,
            b"{\"node\":[{\"version\":\"22\",\"installed\":false}]}",
        ),
        (Query::MiseInstalled, b"{\"node\":[{\"version\":\"22\"}]}"),
        (Query::MiseInstalled, b"{\"node\":[]}"),
        (Query::DpkgPackages, b"pkg\t1\tinstalled\tok"),
        (Query::DpkgPackages, b"pkg\t1\tunknown\tok\n"),
        (Query::DpkgPackages, b"pkg\tPRIVATE\x1b\tinstalled\tok\n"),
        (Query::DpkgPackages, b"pkg\t1\tinstalled\tok\textra\n"),
        (Query::BrewFormulae, b"\xffPRIVATE"),
    ] {
        assert_eq!(catalog::parse(query, bytes), Err(Failure::Malformed));
    }
    assert_eq!(
        catalog::parse(Query::MiseInstalled, &vec![b' '; LIMIT + 1]),
        Err(Failure::OutputLimit)
    );
}

#[test]
fn npm_inventory_is_top_level_scoped_and_drops_private_provider_fields() {
    let entries = catalog::parse(Query::NpmGlobals, br#"{"dependencies":{"@scope/pkg":{"version":"1.2.3","resolved":"PRIVATE","dependencies":{"transitive":{"version":"9.0.0"}}},"npm":{"version":"11.0.0"}}}"#).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].package, "@scope/pkg");
    assert!(!serde_json::to_string(&entries).unwrap().contains("PRIVATE"));
    for input in [
        b"{}".as_slice(),
        b"{\"dependencies\":{}}",
        b"{\"name\":\"lib\"}",
    ] {
        assert!(catalog::parse(Query::NpmGlobals, input).unwrap().is_empty());
    }
    for input in [
        br#"{"problems":["PRIVATE"]}"#.as_slice(),
        br#"{"error":{"message":"PRIVATE"}}"#,
        br#"{"dependencies":{"pkg":{"version":"1.0.0","invalid":"PRIVATE"}}}"#,
        br#"{"dependencies":{"pkg":{"version":"1.0.0","missing":true}}}"#,
    ] {
        assert_eq!(
            catalog::parse(Query::NpmGlobals, input),
            Err(Failure::IncompletePackages)
        );
    }
    assert_eq!(
        catalog::parse(Query::NpmGlobals, br#"{"PRIVATE":"not npm output"}"#),
        Err(Failure::Malformed)
    );
}

#[test]
fn catalog_rejects_oversized_fields_and_record_counts() {
    let oversized = "x".repeat(257);
    assert_eq!(
        catalog::parse(
            Query::DpkgPackages,
            format!("pkg\t{oversized}\tinstalled\tok\n").as_bytes()
        ),
        Err(Failure::Malformed)
    );
    let many = "pkg\t1\tinstalled\tok\n".repeat(32_769);
    assert_eq!(
        catalog::parse(Query::DpkgPackages, many.as_bytes()),
        Err(Failure::Malformed)
    );
}

fn shell(script: &str) -> Command {
    // Trusted fixture interpreter, no external tools, startup files or real HOME.
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .env("HOME", "/no-external-programs")
        .env("PATH", "/no-external-programs")
        .current_dir("/")
        .args(["-c", script]);
    command
}
#[test]
fn bounded_runner_drains_both_pipes_and_withholds_failures() {
    assert_eq!(
        run_bounded(
            shell("printf '{}'; printf PRIVATE >&2"),
            64,
            Duration::from_secs(2)
        )
        .unwrap(),
        b"{}"
    );
    assert_eq!(
        run_bounded(
            shell("printf PRIVATE; printf PRIVATE >&2; exit 7"),
            64,
            Duration::from_secs(2)
        ),
        Err(Failure::QueryFailed)
    );
    assert_eq!(
        run_bounded(
            shell("while :; do printf 12345678; printf 12345678 >&2; done"),
            1024,
            Duration::from_secs(2)
        ),
        Err(Failure::OutputLimit)
    );
    assert_eq!(
        run_bounded(
            shell("while :; do printf 12345678 >&2; done"),
            1024,
            Duration::from_secs(2)
        ),
        Err(Failure::OutputLimit)
    );
}
#[test]
fn bounded_runner_deadline_includes_descendant_retained_pipes() {
    let start = Instant::now();
    assert_eq!(
        run_bounded(
            shell("(while :; do :; done) & exit 0"),
            1024,
            Duration::from_millis(50)
        ),
        Err(Failure::Timeout)
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(
        run_bounded(
            shell("while :; do :; done"),
            1024,
            Duration::from_millis(50)
        ),
        Err(Failure::Timeout)
    );
}
