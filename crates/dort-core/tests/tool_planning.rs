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
    Tool {
        id: id.into(),
        binary: id.into(),
        profiles: vec!["base".into()],
        requires: vec![],
        install: InstallSpec {
            provider,
            package: package.into(),
            version: version.map(str::to_owned),
            runtime_tool: (provider == Provider::Npm).then(|| "node".into()),
        },
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
            npm_global_prefix: None,
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
    let m = manifest(vec![tool("rg", Provider::Brew, "ripgrep", None)]);
    let p = planning::plan(&m, Platform::Linux, &active(), &Observations::default()).unwrap();
    assert!(matches!(p.rows[0].package, PackageFinding::NotObserved));
    assert_eq!(p.rows[0].action, PackageAction::Blocked);
    let mut r = report(Query::BrewFormulae, &[]);
    r.observation = Observation::Unavailable {
        reason: inspection::Failure::Timeout,
    };
    let p = planning::plan(
        &m,
        Platform::Linux,
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
    let m = manifest(vec![tool("rg", Provider::Brew, "ripgrep", None)]);
    let r = report(Query::BrewFormulae, &[]);
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
            Platform::Linux,
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
        let m = manifest(vec![tool("rg", Provider::Brew, "ripgrep", version)]);
        let r = report(Query::BrewFormulae, &[("ripgrep", "1:0.1-1~old")]);
        for binary in [
            BinaryObservation::NotObserved,
            BinaryObservation::Missing,
            candidates(&["/unattributed/rg", "/other/rg"]),
        ] {
            let p = planning::plan(
                &m,
                Platform::Linux,
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
fn multiple_versions_preserve_provider_current_claims_without_activation() {
    let m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    let r = report(
        Query::BrewFormulae,
        &[("node", "20.0.0"), ("node", "22.1.0"), ("node", "22.1.0")],
    );
    let p = planning::plan(
        &m,
        Platform::Linux,
        &active(),
        &input("node", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(p.rows[0].action, PackageAction::NoChange);
    assert!(
        matches!(&p.rows[0].package, PackageFinding::Present { versions } if versions.len() == 2 && versions.iter().all(|v| v.policy_match == VersionMatch::Satisfied))
    );
}

#[test]
fn exact_prerelease_and_build_metadata_use_the_shared_npm_policy() {
    let m = manifest(vec![
        tool("node", Provider::Brew, "node", None),
        tool("demo", Provider::Npm, "demo", Some("22.1.0-beta.1+chosen")),
    ]);
    let runtime = report(Query::BrewFormulae, &[("node", "22.0.0")]);
    for (version, action) in [
        ("22.1.0-beta.1+chosen", PackageAction::NoChange),
        ("22.1.0-beta.1+other", PackageAction::Blocked),
        ("22.1.0", PackageAction::Blocked),
    ] {
        let npm = report(Query::NpmGlobals, &[("demo", version)]);
        let observed = Observations {
            catalogs: [("node".into(), &runtime), ("demo".into(), &npm)].into(),
            binaries: [
                ("node".into(), candidates(&["/trusted/node"])),
                ("demo".into(), BinaryObservation::NotObserved),
            ]
            .into(),
        };
        let p = planning::plan(&m, Platform::Linux, &active(), &observed).unwrap();
        assert_eq!(
            p.rows.iter().find(|r| r.id == "demo").unwrap().action,
            action
        );
    }
}

#[test]
fn unenforceable_provider_policies_are_rejected() {
    for requested in ["22", "22.01", "^1.2.3", "1.2.3"] {
        assert!(
            manifest(vec![tool("node", Provider::Brew, "node", Some(requested))])
                .validate()
                .is_err()
        );
    }
}

#[test]
fn exact_provider_and_explicit_query_scope_are_not_inferred_or_merged() {
    let m = manifest(vec![
        tool("one", Provider::Brew, "node", None),
        tool("two", Provider::Brew, "node", None),
    ]);
    let r1 = report(Query::BrewFormulae, &[]);
    let mut r2 = report(Query::BrewFormulae, &[("node", "22.0.0")]);
    r2.description.home = "/other/home".into();
    r2.description.executable = "/other/brew".into();
    let obs = Observations {
        catalogs: [("one".into(), &r1), ("two".into(), &r2)].into(),
        binaries: BTreeMap::new(),
    };
    let p = planning::plan(&m, Platform::Linux, &active(), &obs).unwrap();
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
    let wrong = report(Query::NpmGlobals, &[("node", "22.0.0")]);
    let m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    let p = planning::plan(
        &m,
        Platform::Linux,
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
fn qualified_tap_names_are_never_silently_normalized() {
    for platform in [Platform::Macos, Platform::Linux] {
        for (requested, reported) in [("rg", "tap/tools/rg"), ("tap/tools/rg", "rg")] {
            let m = manifest(vec![tool("test", Provider::Brew, requested, None)]);
            let r = report(Query::BrewFormulae, &[(reported, "1.0.0")]);
            let p = planning::plan(
                &m,
                platform,
                &active(),
                &input("test", &r, BinaryObservation::Missing),
            )
            .unwrap();
            assert_eq!(p.rows[0].package, PackageFinding::AmbiguousIdentity);
            assert_eq!(p.rows[0].blockers, [Blocker::AmbiguousPackageIdentity]);
        }
    }
}

#[test]
fn platform_independent_tools_still_report_inactive_dependencies() {
    let mut inactive = tool("inactive", Provider::Brew, "inactive", None);
    inactive.profiles = vec!["extra".into()];
    let mut dependent = tool("dependent", Provider::Brew, "dependent", None);
    dependent.requires = vec!["inactive".into()];
    let mut m = manifest(vec![
        dependent,
        inactive,
        tool("brew", Provider::Brew, "git", None),
    ]);
    m.profiles.available = vec!["extra".into()];
    let r = report(Query::BrewFormulae, &[]);
    let p = planning::plan(
        &m,
        Platform::Linux,
        &active(),
        &input("dependent", &r, BinaryObservation::Missing),
    )
    .unwrap();
    assert_eq!(p.rows.len(), 2);
    assert_eq!(
        p.rows.iter().find(|r| r.id == "brew").unwrap().package,
        PackageFinding::NotObserved
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
    let mut child = tool("child", Provider::Brew, "child", None);
    child.requires = vec!["root".into()];
    let m = manifest(vec![
        child,
        tool("root", Provider::Brew, "root", None),
        tool("alpha", Provider::Brew, "alpha", None),
    ]);
    let empty = report(Query::BrewFormulae, &[]);
    let mut obs = Observations::default();
    for id in ["alpha", "root", "child"] {
        obs.catalogs.insert(id.into(), &empty);
        obs.binaries.insert(id.into(), BinaryObservation::Missing);
    }
    let p = planning::plan(&m, Platform::Linux, &active(), &obs).unwrap();
    assert_eq!(
        p.rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        m.dependency_order(Platform::Linux, &active()).unwrap()
    );
    let child = p.rows.iter().find(|r| r.id == "child").unwrap();
    assert_eq!(
        child.blockers,
        [Blocker::DependencyNeedsReview {
            tool: "root".into()
        }]
    );
    assert_eq!(child.action, PackageAction::Blocked);
    let root = report(Query::BrewFormulae, &[("root", "1.0.0")]);
    obs.catalogs.insert("root".into(), &root);
    let p = planning::plan(&m, Platform::Linux, &active(), &obs).unwrap();
    assert_eq!(
        p.rows.iter().find(|r| r.id == "child").unwrap().action,
        PackageAction::ReviewMissingPackage
    );
    let mut unavailable = report(Query::BrewFormulae, &[]);
    unavailable.observation = Observation::Unavailable {
        reason: inspection::Failure::Unavailable,
    };
    obs.catalogs.insert("root".into(), &unavailable);
    assert!(
        planning::plan(&m, Platform::Linux, &active(), &obs)
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
        tool("node", Provider::Brew, "node", None),
    ]);
    let node = report(Query::BrewFormulae, &[("node", "22.0.0")]);
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
        let p = planning::plan(&m, Platform::Linux, &active(), &obs).unwrap();
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
    let mut inventory = report(Query::NpmGlobals, &[]);
    inventory.description.npm_binding = None;
    inventory.description.npm_global_prefix = Some("/discovered/globals".into());
    let p = planning::plan(
        &m,
        Platform::Linux,
        &active(),
        &Observations {
            catalogs: [("cli".into(), &inventory), ("node".into(), &node)].into(),
            binaries: [
                ("cli".into(), BinaryObservation::Missing),
                ("node".into(), candidates(&["/trusted/node"])),
            ]
            .into(),
        },
    )
    .unwrap();
    let row = p.rows.iter().find(|r| r.id == "cli").unwrap();
    assert_eq!(row.action, PackageAction::Blocked);
    assert!(matches!(
        row.package,
        PackageFinding::ScopeMismatch {
            reason: ScopeIssue::InvalidRuntimeBinding
        }
    ));
    let mut wrong = report(Query::NpmGlobals, &[("@scope/cli", "1.0.0")]);
    wrong.description.npm_binding.as_mut().unwrap().runtime_tool = "some-other-runtime".into();
    let p = planning::plan(
        &m,
        Platform::Linux,
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
    let m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    let r = report(Query::BrewFormulae, &[("node", "PRIVATE\x1b[31m")]);
    let p = planning::plan(
        &m,
        Platform::Linux,
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
        assert!(planning::plan(&m, Platform::Linux, &active(), &obs).is_err());
    }
    let mut r = report(Query::BrewFormulae, &[]);
    r.description.home = "relative".into();
    assert!(
        planning::plan(
            &m,
            Platform::Linux,
            &active(),
            &input("node", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
}

#[test]
fn package_noop_can_still_have_dependency_blockers_and_never_means_tool_readiness() {
    let mut child = tool("child", Provider::Brew, "child", None);
    child.requires = vec!["root".into()];
    let mut grandchild = tool("grandchild", Provider::Brew, "grandchild", None);
    grandchild.requires = vec!["child".into()];
    let m = manifest(vec![
        grandchild,
        child,
        tool("root", Provider::Brew, "root", None),
    ]);
    let child = report(Query::BrewFormulae, &[("child", "1.0.0")]);
    let empty = report(Query::BrewFormulae, &[]);
    let mut obs = input("child", &child, BinaryObservation::Missing);
    obs.catalogs.insert("grandchild".into(), &empty);
    obs.binaries
        .insert("grandchild".into(), BinaryObservation::Missing);
    let p = planning::plan(&m, Platform::Linux, &active(), &obs).unwrap();
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
    let mut m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    m.files.push(dort_core::model::ManagedFile {
        id: "unrelated".into(),
        source: Some("absent/source".into()),
        template: None,
        target: "~/.unrelated".into(),
        mode: dort_core::model::FileMode::Template,
        profiles: vec!["base".into()],
        requires: vec![],
        overrides: BTreeMap::new(),
        inputs: Default::default(),
        syntax: None,
        host_inputs: None,
        permissions: None,
    });
    let r = report(Query::BrewFormulae, &[("node", "22.0.0")]);
    let p = planning::plan(
        &m,
        Platform::Linux,
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
                Platform::Linux,
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
            .map(|i| tool(&format!("t{i}"), Provider::Brew, "node", None))
            .collect(),
    );
    assert!(
        planning::plan(
            &too_many,
            Platform::Linux,
            &active(),
            &Observations::default()
        )
        .is_err()
    );
    let m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    let mut r = report(Query::BrewFormulae, &[]);
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
            Platform::Linux,
            &active(),
            &input("node", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
    if let Observation::Complete { installations } = &mut r.observation {
        installations.pop();
    }
    let m = manifest(vec![
        tool("one", Provider::Brew, "node", None),
        tool("two", Provider::Brew, "node", None),
        tool("three", Provider::Brew, "node", None),
    ]);
    let obs = Observations {
        catalogs: ["one", "two", "three"].map(|id| (id.into(), &r)).into(),
        binaries: BTreeMap::new(),
    };
    assert!(planning::plan(&m, Platform::Linux, &active(), &obs).is_err());
}

#[test]
fn resolved_profiles_and_selected_observation_ids_are_required_and_plans_grant_no_authority() {
    let m = manifest(vec![tool("node", Provider::Brew, "node", None)]);
    for profiles in [BTreeSet::new(), ["base".into(), "unknown".into()].into()] {
        assert!(planning::plan(&m, Platform::Linux, &profiles, &Observations::default()).is_err());
    }
    let r = report(Query::BrewFormulae, &[]);
    assert!(
        planning::plan(
            &m,
            Platform::Linux,
            &active(),
            &input("unknown", &r, BinaryObservation::Missing)
        )
        .is_err()
    );
    let p = planning::plan(
        &m,
        Platform::Linux,
        &active(),
        &input("node", &r, BinaryObservation::Missing),
    )
    .unwrap();
    let json = serde_json::to_value(p).unwrap();
    assert_eq!(json["rows"][0]["action"], "review_missing_package");
    assert!(json.get("approval").is_none() && json.get("execute").is_none());
}
