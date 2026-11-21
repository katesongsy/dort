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
}

#[test]
fn only_valid_empty_protocols_are_healthy_empty() {
    for query in [Query::BrewFormulae, Query::NpmGlobals] {
        let empty: &[u8] = if query == Query::BrewFormulae {
            br#"{"formulae":[]}"#
        } else {
            b"{}"
        };
        assert!(catalog::parse(query, empty).unwrap().is_empty());
        assert_eq!(
            catalog::parse(query, b"\\xffPRIVATE"),
            Err(Failure::Malformed)
        );
        assert_eq!(
            catalog::parse(query, &vec![b' '; LIMIT + 1]),
            Err(Failure::OutputLimit)
        );
    }
    for input in [
        b"{}".as_slice(),
        br#"{"formulae":[{"full_name":"foo","installed":[]}]}"#,
    ] {
        assert_eq!(
            catalog::parse(Query::BrewFormulae, input),
            Err(Failure::Malformed)
        );
    }
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
    let entry =
        |version: &str| serde_json::json!({"full_name":"pkg", "installed":[{"version":version}]});
    for formulae in [vec![entry(&"x".repeat(257))], vec![entry("1"); 32_769]] {
        let bytes = serde_json::to_vec(&serde_json::json!({"formulae":formulae})).unwrap();
        assert_eq!(
            catalog::parse(Query::BrewFormulae, &bytes),
            Err(Failure::Malformed)
        );
    }
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
