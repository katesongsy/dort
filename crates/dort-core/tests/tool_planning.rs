//! Pure supplied-metadata fixtures: no filesystem, environment or provider processes.
use dort_core::{
    Manifest,
    model::{InstallSpec, Provider, Tool},
    platform::Platform,
    tools::{
        VersionMatch,
        inspection::{self, Description, Installation, Observation, Query, Report},
        planning::{
            self, BinaryObservation, Blocker, Observations, PackageAction, PackageFinding,
            ScopeIssue,
        },
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

fn tool(id: &str, provider: Provider, package: &str, version: Option<&str>) -> Tool {
    let platform = if provider == Provider::Brew {
        Platform::Macos
    } else {
        Platform::Ubuntu
    };
    Tool {
        id: id.into(),
        binary: id.into(),
        profiles: vec!["base".into()],
        requires: vec![],
        install: [(
            platform,
            InstallSpec {
                provider,
                package: package.into(),
                version: version.map(str::to_owned),
                runtime_tool: (provider == Provider::Npm).then(|| "node".into()),
            },
        )]
        .into(),
    }
}
fn manifest(tools: Vec<Tool>) -> Manifest {
    Manifest {
        schema: 1,
        profiles: Default::default(),
        tools,
        files: vec![],
    }
}
fn active() -> BTreeSet<String> {
    ["base".into()].into()
}
fn report(query: Query, claims: &[(&str, &str)]) -> Report {
    let binding = (query == Query::NpmGlobals).then(|| inspection::NpmBinding {
        runtime_tool: "node".into(),
        node: "/trusted/node".into(),
        npm_cli: "/trusted/npm-cli.js".into(),
        prefix: "/chosen/prefix".into(),
    });
    Report {
        description: Description {
            query,
            executable: if binding.is_some() {
                "/trusted/node".into()
            } else {
                "/trusted/provider".into()
            },
            arguments: vec![],
            working_directory: "/",
            home: "/fixture/home".into(),
            trusted_search_path: vec!["/trusted".into()],
            npm_binding: binding,
            warning: inspection::WARNING,
            output_limit_bytes: 4 * 1024 * 1024,
            timeout_seconds: 10,
        },
        observation: Observation::Complete {
            installations: claims
                .iter()
                .map(|(p, v)| Installation {
                    package: (*p).into(),
                    version: (*v).into(),
                })
                .collect(),
        },
    }
}
fn input<'a>(id: &str, report: &'a Report, binary: BinaryObservation) -> Observations<'a> {
    Observations {
        catalogs: [(id.into(), report)].into(),
        binaries: [(id.into(), binary)].into(),
    }
}
fn candidates(paths: &[&str]) -> BinaryObservation {
    BinaryObservation::Candidates {
        paths: paths.iter().map(PathBuf::from).collect(),
    }
}

#[test]
fn unavailable_and_unobserved_are_never_healthy_absence_or_install_proposals() {
    let m = manifest(vec![tool("rg", Provider::Apt, "ripgrep", None)]);
    let p = planning::plan(&m, Platform::Ubuntu, &active(), &Observations::default()).unwrap();
    assert!(matches!(p.rows[0].package, PackageFinding::NotObserved));
    assert_eq!(p.rows[0].action, PackageAction::Blocked);
    let mut r = report(Query::DpkgPackages, &[]);
    r.observation = Observation::Unavailable {
        reason: inspection::Failure::Timeout,
    };
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("rg", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert!(matches!(
        p.rows[0].package,
        PackageFinding::Unavailable {
            reason: inspection::Failure::Timeout
        }
    ));
    assert_eq!(p.rows[0].blockers, [Blocker::ProviderUnavailable]);
}

#[test]
fn only_known_package_absence_and_explicit_binary_absence_propose_review() {
    let m = manifest(vec![tool("rg", Provider::Apt, "ripgrep", None)]);
    let r = report(Query::DpkgPackages, &[]);
    for (binary, blocker) in [
        (
            BinaryObservation::NotObserved,
            Some(Blocker::BinaryNotObserved),
        ),
        (
            BinaryObservation::Unavailable,
            Some(Blocker::BinaryObservationUnavailable),
        ),
        (
            candidates(&["/somewhere/rg-99.0.0"]),
            Some(Blocker::UnattributedBinaryCandidates),
        ),
        (BinaryObservation::Missing, None),
    ] {
        let p = planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("rg", &r, binary.clone()),
        )
        .unwrap();
        assert!(matches!(p.rows[0].package, PackageFinding::Absent));
        assert_eq!(p.rows[0].binary_observation, binary);
        if let Some(blocker) = blocker {
            assert_eq!(p.rows[0].blockers, [blocker]);
            assert_eq!(p.rows[0].action, PackageAction::Blocked);
        } else {
            assert_eq!(p.rows[0].action, PackageAction::ReviewMissingPackage);
            assert!(!p.has_blockers());
        }
    }
}

#[test]
fn latest_and_default_never_upgrade_an_existing_provider_claim_or_infer_binary_ownership() {
    for version in [None, Some("latest")] {
        let m = manifest(vec![tool("rg", Provider::Apt, "ripgrep", version)]);
        let r = report(Query::DpkgPackages, &[("ripgrep", "1:0.1-1~old")]);
        for binary in [
            BinaryObservation::NotObserved,
            BinaryObservation::Missing,
            candidates(&["/unattributed/rg", "/other/rg"]),
        ] {
            let p = planning::plan(
                &m,
                Platform::Ubuntu,
                &active(),
                &input("rg", &r, binary.clone()),
            )
            .unwrap();
            assert_eq!(p.rows[0].action, PackageAction::NoChange);
            assert_eq!(p.rows[0].binary_observation, binary);
            assert!(
                matches!(&p.rows[0].package, PackageFinding::Present { versions } if versions[0].policy_match == VersionMatch::Satisfied)
            );
        }
    }
}

#[test]
fn multiple_versions_match_policy_without_claiming_activation_or_overwriting_mismatches() {
    let m = manifest(vec![tool("node", Provider::Mise, "node", Some("22"))]);
    let r = report(
        Query::MiseInstalled,
        &[("node", "20.0.0"), ("node", "22.1.0"), ("node", "22.1.0")],
    );
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("node", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(p.rows[0].action, PackageAction::NoChange);
    assert!(
        matches!(&p.rows[0].package, PackageFinding::Present { versions } if versions.len() == 2 && versions[0].policy_match == VersionMatch::Mismatch && versions[1].policy_match == VersionMatch::Satisfied)
    );
    for (version, expected) in [
        ("20.0.0", Blocker::VersionMismatch),
        ("22.1.0-beta.1", Blocker::VersionMismatch),
        ("opaque", Blocker::VersionUnverifiable),
    ] {
        let r = report(Query::MiseInstalled, &[("node", version)]);
        let p = planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("node", &r, candidates(&["/looks/like/node22"])),
        )
        .unwrap();
        assert_eq!(p.rows[0].action, PackageAction::Blocked);
        assert_eq!(p.rows[0].blockers, [expected]);
    }
}

#[test]
fn exact_prerelease_and_build_metadata_use_the_existing_shared_policy() {
    let m = manifest(vec![tool(
        "node",
        Provider::Mise,
        "node",
        Some("22.1.0-beta.1+chosen"),
    )]);
    for (version, action) in [
        ("22.1.0-beta.1+chosen", PackageAction::NoChange),
        ("22.1.0-beta.1+other", PackageAction::Blocked),
        ("22.1.0", PackageAction::Blocked),
    ] {
        let r = report(Query::MiseInstalled, &[("node", version)]);
        assert_eq!(
            planning::plan(
                &m,
                Platform::Ubuntu,
                &active(),
                &input("node", &r, BinaryObservation::NotObserved)
            )
            .unwrap()
            .rows[0]
                .action,
            action
        );
    }
}

#[test]
fn legacy_noncanonical_or_unobservable_requests_do_not_propose_an_unverifiable_install() {
    let oversized = format!("1.0.0+{}", "a".repeat(260));
    for requested in ["022", "22.01", "18446744073709551616", oversized.as_str()] {
        let m = manifest(vec![tool("node", Provider::Mise, "node", Some(requested))]);
        m.validate().unwrap(); // Keep the legacy manifest grammar accepted.
        let empty = report(Query::MiseInstalled, &[]);
        let p = planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("node", &empty, BinaryObservation::Missing),
        )
        .unwrap();
        assert!(matches!(p.rows[0].package, PackageFinding::Absent));
        assert_eq!(p.rows[0].action, PackageAction::Blocked);
        assert_eq!(p.rows[0].blockers, [Blocker::VersionUnverifiable]);
    }
}

#[test]
fn exact_provider_and_explicit_query_scope_are_not_inferred_or_merged() {
    let m = manifest(vec![
        tool("one", Provider::Mise, "node", None),
        tool("two", Provider::Mise, "node", None),
    ]);
    let r1 = report(Query::MiseInstalled, &[]);
    let mut r2 = report(Query::MiseInstalled, &[("node", "22.0.0")]);
    r2.description.home = "/other/home".into();
    r2.description.executable = "/other/mise".into();
    let obs = Observations {
        catalogs: [("one".into(), &r1), ("two".into(), &r2)].into(),
        binaries: BTreeMap::new(),
    };
    let p = planning::plan(&m, Platform::Ubuntu, &active(), &obs).unwrap();
    assert!(matches!(p.rows[0].package, PackageFinding::Absent));
    assert_eq!(p.rows[1].action, PackageAction::NoChange);
    assert_eq!(
        p.rows[0].catalog_scope.as_ref().unwrap().home,
        PathBuf::from("/fixture/home")
    );
    assert_eq!(
        p.rows[1].catalog_scope.as_ref().unwrap().home,
        PathBuf::from("/other/home")
    );
    let wrong = report(Query::DpkgPackages, &[("node", "22.0.0")]);
    let m = manifest(vec![tool("node", Provider::Mise, "node", None)]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("node", &wrong, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(
        p.rows[0].package,
        PackageFinding::ScopeMismatch {
            reason: ScopeIssue::WrongProvider
        }
    );
}

#[test]
fn qualified_architecture_tap_and_backend_names_are_never_silently_normalized() {
    for (provider, query, platform, requested, reported) in [
        (
            Provider::Apt,
            Query::DpkgPackages,
            Platform::Ubuntu,
            "libc6",
            "libc6:amd64",
        ),
        (
            Provider::Apt,
            Query::DpkgPackages,
            Platform::Ubuntu,
            "libc6:amd64",
            "libc6",
        ),
        (
            Provider::Brew,
            Query::BrewFormulae,
            Platform::Macos,
            "rg",
            "tap/tools/rg",
        ),
        (
            Provider::Mise,
            Query::MiseInstalled,
            Platform::Ubuntu,
            "node",
            "core:node",
        ),
    ] {
        let m = manifest(vec![tool("test", provider, requested, None)]);
        let r = report(query, &[(reported, "1.0.0")]);
        let p = planning::plan(
            &m,
            platform,
            &active(),
            &input("test", &r, BinaryObservation::Missing),
        )
        .unwrap();
        assert_eq!(p.rows[0].package, PackageFinding::AmbiguousIdentity);
        assert_eq!(p.rows[0].blockers, [Blocker::AmbiguousPackageIdentity]);
        let r = report(query, &[(requested, "1.0.0")]);
        assert_eq!(
            planning::plan(
                &m,
                platform,
                &active(),
                &input("test", &r, BinaryObservation::Missing)
            )
            .unwrap()
            .rows[0]
                .action,
            PackageAction::NoChange
        );
    }
}

#[test]
fn unsupported_platform_and_inactive_dependency_are_reportable_not_assumed_installs() {
    let mut inactive = tool("inactive", Provider::Mise, "inactive", None);
    inactive.profiles = vec!["extra".into()];
    let mut dependent = tool("dependent", Provider::Mise, "dependent", None);
    dependent.requires = vec!["inactive".into()];
    let mut m = manifest(vec![
        dependent,
        inactive,
        tool("brew", Provider::Brew, "git", None),
    ]);
    m.profiles.available = vec!["extra".into()];
    let r = report(Query::MiseInstalled, &[]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("dependent", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(p.rows.len(), 2);
    assert!(
        p.rows
            .iter()
            .any(|r| r.package == PackageFinding::UnsupportedPlatform
                && r.action == PackageAction::Blocked)
    );
    assert!(
        p.rows
            .iter()
            .find(|r| r.id == "dependent")
            .unwrap()
            .blockers
            .contains(&Blocker::DependencyInactive {
                tool: "inactive".into()
            })
    );
}

#[test]
fn dependency_order_reuses_manifest_graph_and_requires_fresh_observation_after_review() {
    let mut child = tool("child", Provider::Mise, "child", None);
    child.requires = vec!["root".into()];
    let m = manifest(vec![
        child,
        tool("root", Provider::Mise, "root", None),
        tool("alpha", Provider::Mise, "alpha", None),
    ]);
    let empty = report(Query::MiseInstalled, &[]);
    let mut obs = Observations::default();
    for id in ["alpha", "root", "child"] {
        obs.catalogs.insert(id.into(), &empty);
        obs.binaries.insert(id.into(), BinaryObservation::Missing);
    }
    let p = planning::plan(&m, Platform::Ubuntu, &active(), &obs).unwrap();
    assert_eq!(
        p.rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        m.dependency_order(Platform::Ubuntu, &active()).unwrap()
    );
    let child = p.rows.iter().find(|r| r.id == "child").unwrap();
    assert_eq!(
        child.blockers,
        [Blocker::DependencyNeedsReview {
            tool: "root".into()
        }]
    );
    assert_eq!(child.action, PackageAction::Blocked);
    let root = report(Query::MiseInstalled, &[("root", "1.0.0")]);
    obs.catalogs.insert("root".into(), &root);
    let p = planning::plan(&m, Platform::Ubuntu, &active(), &obs).unwrap();
    assert_eq!(
        p.rows.iter().find(|r| r.id == "child").unwrap().action,
        PackageAction::ReviewMissingPackage
    );
    let mut unavailable = report(Query::MiseInstalled, &[]);
    unavailable.observation = Observation::Unavailable {
        reason: inspection::Failure::Unavailable,
    };
    obs.catalogs.insert("root".into(), &unavailable);
    assert!(
        planning::plan(&m, Platform::Ubuntu, &active(), &obs)
            .unwrap()
            .rows
            .iter()
            .find(|r| r.id == "child")
            .unwrap()
            .blockers
            .contains(&Blocker::DependencyBlocked {
                tool: "root".into()
            })
    );
}

#[test]
fn npm_scope_must_bind_declared_runtime_and_selected_node_candidate_without_inference() {
    let m = manifest(vec![
        tool("cli", Provider::Npm, "@scope/cli", Some("1.0.0")),
        tool("node", Provider::Mise, "node", Some("22")),
    ]);
    let node = report(Query::MiseInstalled, &[("node", "22.0.0")]);
    let npm = report(Query::NpmGlobals, &[]);
    for (binary, allowed) in [
        (BinaryObservation::NotObserved, false),
        (BinaryObservation::Missing, false),
        (candidates(&["/other/node"]), false),
        (candidates(&["/trusted/node", "/other/node"]), false),
        (candidates(&["/trusted/node"]), true),
    ] {
        let obs = Observations {
            catalogs: [("cli".into(), &npm), ("node".into(), &node)].into(),
            binaries: [
                ("cli".into(), BinaryObservation::Missing),
                ("node".into(), binary),
            ]
            .into(),
        };
        let p = planning::plan(&m, Platform::Ubuntu, &active(), &obs).unwrap();
        let row = p.rows.iter().find(|r| r.id == "cli").unwrap();
        assert_eq!(
            row.blockers.contains(&Blocker::RuntimeBindingUnverified),
            !allowed
        );
        assert_eq!(row.action == PackageAction::ReviewMissingPackage, allowed);
        assert_eq!(
            row.catalog_scope
                .as_ref()
                .unwrap()
                .npm_binding
                .as_ref()
                .unwrap()
                .prefix,
            PathBuf::from("/chosen/prefix")
        );
    }
    let mut wrong = report(Query::NpmGlobals, &[("@scope/cli", "1.0.0")]);
    wrong.description.npm_binding.as_mut().unwrap().runtime_tool = "some-other-runtime".into();
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("cli", &wrong, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(
        p.rows.iter().find(|r| r.id == "cli").unwrap().package,
        PackageFinding::ScopeMismatch {
            reason: ScopeIssue::InvalidRuntimeBinding
        }
    );
}

#[test]
fn invalid_metadata_is_withheld_and_unsafe_path_identity_is_explicitly_rejected() {
    let m = manifest(vec![tool("node", Provider::Mise, "node", None)]);
    let r = report(Query::MiseInstalled, &[("node", "PRIVATE\x1b[31m")]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("node", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(p.rows[0].package, PackageFinding::InvalidMetadata);
    assert!(!serde_json::to_string(&p).unwrap().contains("PRIVATE"));
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for path in [
        PathBuf::from("relative"),
        PathBuf::from("/a/../b"),
        PathBuf::from(OsString::from_vec(b"/bad\xff".to_vec())),
    ] {
        let mut obs = Observations::default();
        obs.binaries.insert(
            "node".into(),
            BinaryObservation::Candidates { paths: vec![path] },
        );
        assert!(planning::plan(&m, Platform::Ubuntu, &active(), &obs).is_err());
    }
    let mut r = report(Query::MiseInstalled, &[]);
    r.description.home = "relative".into();
    assert!(
        planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("node", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
}

#[test]
fn package_noop_can_still_have_dependency_blockers_and_never_means_tool_readiness() {
    let mut child = tool("child", Provider::Mise, "child", None);
    child.requires = vec!["root".into()];
    let mut grandchild = tool("grandchild", Provider::Mise, "grandchild", None);
    grandchild.requires = vec!["child".into()];
    let m = manifest(vec![
        grandchild,
        child,
        tool("root", Provider::Mise, "root", None),
    ]);
    let child = report(Query::MiseInstalled, &[("child", "1.0.0")]);
    let empty = report(Query::MiseInstalled, &[]);
    let mut obs = input("child", &child, BinaryObservation::Missing);
    obs.catalogs.insert("grandchild".into(), &empty);
    obs.binaries
        .insert("grandchild".into(), BinaryObservation::Missing);
    let p = planning::plan(&m, Platform::Ubuntu, &active(), &obs).unwrap();
    let child = p.rows.iter().find(|r| r.id == "child").unwrap();
    assert_eq!(child.action, PackageAction::NoChange);
    assert_eq!(
        child.blockers,
        [Blocker::DependencyBlocked {
            tool: "root".into()
        }]
    );
    let grandchild = p.rows.iter().find(|r| r.id == "grandchild").unwrap();
    assert_eq!(grandchild.action, PackageAction::Blocked);
    assert_eq!(
        grandchild.blockers,
        [Blocker::DependencyBlocked {
            tool: "child".into()
        }]
    );
}

#[test]
fn pure_package_planning_does_not_expand_or_observe_unrelated_file_targets() {
    let mut m = manifest(vec![tool("node", Provider::Mise, "node", None)]);
    m.files.push(dort_core::model::ManagedFile {
        id: "unrelated".into(),
        source: "absent/source".into(),
        target: "~/.unrelated".into(),
        mode: dort_core::model::FileMode::Template,
        profiles: vec!["base".into()],
        requires: vec![],
        overrides: BTreeMap::new(),
        permissions: None,
    });
    let r = report(Query::MiseInstalled, &[("node", "22.0.0")]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("node", &r, BinaryObservation::NotObserved),
    )
    .unwrap();
    assert_eq!(p.rows[0].action, PackageAction::NoChange);
    let mut reversed = m.clone();
    reversed.tools.reverse();
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        serde_json::to_value(
            planning::plan(
                &reversed,
                Platform::Ubuntu,
                &active(),
                &input("node", &r, BinaryObservation::NotObserved)
            )
            .unwrap()
        )
        .unwrap()
    );
}

#[test]
fn analysis_bounds_selected_tools_catalog_records_and_total_copied_claims() {
    let too_many = manifest(
        (0..=planning::MAX_SELECTED_TOOLS)
            .map(|i| tool(&format!("t{i}"), Provider::Mise, "node", None))
            .collect(),
    );
    assert!(
        planning::plan(
            &too_many,
            Platform::Ubuntu,
            &active(),
            &Observations::default()
        )
        .is_err()
    );
    let m = manifest(vec![tool("node", Provider::Mise, "node", None)]);
    let mut r = report(Query::MiseInstalled, &[]);
    r.observation = Observation::Complete {
        installations: (0..32_769)
            .map(|i| Installation {
                package: "node".into(),
                version: format!("v{i}"),
            })
            .collect(),
    };
    assert!(
        planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("node", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
    if let Observation::Complete { installations } = &mut r.observation {
        installations.pop();
    }
    let m = manifest(vec![
        tool("one", Provider::Mise, "node", None),
        tool("two", Provider::Mise, "node", None),
        tool("three", Provider::Mise, "node", None),
    ]);
    let obs = Observations {
        catalogs: ["one", "two", "three"].map(|id| (id.into(), &r)).into(),
        binaries: BTreeMap::new(),
    };
    assert!(planning::plan(&m, Platform::Ubuntu, &active(), &obs).is_err());
}

#[test]
fn resolved_profiles_and_selected_observation_ids_are_required_and_plans_grant_no_authority() {
    let m = manifest(vec![tool("node", Provider::Mise, "node", None)]);
    for profiles in [BTreeSet::new(), ["base".into(), "unknown".into()].into()] {
        assert!(planning::plan(&m, Platform::Ubuntu, &profiles, &Observations::default()).is_err());
    }
    let r = report(Query::MiseInstalled, &[]);
    assert!(
        planning::plan(
            &m,
            Platform::Ubuntu,
            &active(),
            &input("unknown", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &active(),
        &input("node", &r, BinaryObservation::Missing),
    )
    .unwrap();
    let json = serde_json::to_value(p).unwrap();
    assert_eq!(json["rows"][0]["action"], "review_missing_package");
    assert!(json.get("approval").is_none() && json.get("execute").is_none());
}
