//! Entirely in-memory previews: no providers, files, environment discovery or writes.
use dort_core::{
    Manifest,
    model::{InstallSpec, Provider, Tool},
    platform::Platform,
    tools::{
        inspection::{self, Description, Installation, Observation, Query, Report},
        planning::{self, BinaryObservation, Observations, PackageAction},
    },
};
use std::{collections::BTreeSet, path::PathBuf};
fn tool(id: &str, provider: Provider, package: &str, version: Option<&str>) -> Tool {
    Tool {
        id: id.into(),
        binary: id.into(),
        profiles: vec!["base".into()],
        requires: vec![],
        install: [(
            if provider == Provider::Brew {
                Platform::Macos
            } else {
                Platform::Ubuntu
            },
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
fn report(query: Query, values: &[(&str, &str)]) -> Report {
    let npm = (query == Query::NpmGlobals).then(|| inspection::NpmBinding {
        runtime_tool: "node".into(),
        node: "/chosen/node".into(),
        npm_cli: "/chosen/npm-cli.js".into(),
        prefix: "/chosen/prefix".into(),
    });
    Report {
        description: Description {
            query,
            executable: if npm.is_some() {
                "/chosen/node".into()
            } else {
                "/chosen/provider".into()
            },
            home: "/fixture/home".into(),
            arguments: vec![],
            working_directory: "/",
            trusted_search_path: vec!["/provider/search".into()],
            npm_binding: npm,
            warning: inspection::WARNING,
            output_limit_bytes: 4 * 1024 * 1024,
            timeout_seconds: 10,
        },
        observation: Observation::Complete {
            installations: values
                .iter()
                .map(|(p, v)| Installation {
                    package: (*p).into(),
                    version: (*v).into(),
                })
                .collect(),
        },
    }
}
fn plan_one(
    provider: Provider,
    query: Query,
    version: Option<&str>,
    values: &[(&str, &str)],
    binary: BinaryObservation,
) -> planning::Plan {
    let m = manifest(vec![tool("tool", provider, "tool", version)]);
    let r = report(query, values);
    planning::plan(
        &m,
        if provider == Provider::Brew {
            Platform::Macos
        } else {
            Platform::Ubuntu
        },
        &BTreeSet::from(["base".into()]),
        &Observations {
            catalogs: [("tool".into(), &r)].into(),
            binaries: [("tool".into(), binary)].into(),
        },
    )
    .unwrap()
}
fn preview(plan: &planning::Plan) -> serde_json::Value {
    serde_json::to_value(&plan.rows[0].install_preview).unwrap()
}
#[test]
fn brew_missing_formula_has_only_preview_metadata_and_explicit_native_side_effect_risks() {
    for policy in [None, Some("latest")] {
        let p = plan_one(
            Provider::Brew,
            Query::BrewFormulae,
            policy,
            &[],
            BinaryObservation::Missing,
        );
        let v = preview(&p);
        assert_eq!(p.rows[0].action, PackageAction::ReviewMissingPackage);
        assert_eq!(v["state"], "preview_only");
        let c = &v["command"];
        assert_eq!(c["executable"], "/chosen/provider");
        assert_eq!(
            c["arguments"],
            serde_json::json!(["install", "--formula", "tool"])
        );
        assert_eq!(c["environment_cleared"], true);
        assert_eq!(c["execution_authorized"], false);
        assert_eq!(c["working_directory"], "/");
        for key in [
            "HOMEBREW_NO_AUTO_UPDATE",
            "HOMEBREW_NO_INSTALL_UPGRADE",
            "HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK",
            "HOMEBREW_NO_INSTALL_CLEANUP",
            "HOMEBREW_NO_ANALYTICS",
        ] {
            assert_eq!(c["environment"][key], "1");
        }
        assert_eq!(c["environment"]["PATH"], "/provider/search");
        assert!(
            c["effects"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(
                    "dependency_changes_may_include_upgrades"
                ))
        );
        assert!(c["warning"].as_str().unwrap().contains("PREVIEW ONLY"));
        assert!(
            !serde_json::to_string(c)
                .unwrap()
                .contains("ignore-dependencies")
        );
    }
}
#[test]
fn mise_default_latest_prefix_and_exact_requests_are_distinct_and_do_not_activate() {
    for (version, arg, selection) in [
        (None, "tool", "provider_default_unresolved"),
        (Some("latest"), "tool@latest", "latest_unresolved"),
        (Some("22"), "tool@22", "mise_prefix_unresolved"),
        (Some("22.3"), "tool@22.3", "mise_prefix_unresolved"),
        (
            Some("22.3.1-beta.1+chosen"),
            "tool@22.3.1-beta.1+chosen",
            "exact_requested",
        ),
    ] {
        let p = plan_one(
            Provider::Mise,
            Query::MiseInstalled,
            version,
            &[],
            BinaryObservation::Missing,
        );
        let v = preview(&p);
        let c = &v["command"];
        assert_eq!(
            c["arguments"],
            serde_json::json!(["install", "--jobs=1", arg])
        );
        assert_eq!(c["version_selection"], selection);
        assert!(
            c["effects"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("mise_activation_not_requested"))
        );
        assert_eq!(c["environment"]["MISE_AUTO_INSTALL"], "0");
        assert!(c["environment"].get("MISE_OFFLINE").is_none());
        if version.is_none() {
            assert!(
                c["effects"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(
                        "global_configuration_may_select_multiple_versions"
                    ))
            );
        }
        assert!(
            !c["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "use" || a == "exec" || a == "--force")
        );
    }
}
#[test]
fn npm_uses_exact_bound_node_cli_prefix_and_disables_lifecycle_scripts_in_the_description() {
    for (policy, package) in [
        (None, "@scope/tool"),
        (Some("latest"), "@scope/tool@latest"),
        (
            Some("1.2.3-beta.1+chosen"),
            "@scope/tool@1.2.3-beta.1+chosen",
        ),
    ] {
        let m = manifest(vec![
            tool("cli", Provider::Npm, "@scope/tool", policy),
            tool("node", Provider::Mise, "node", Some("22")),
        ]);
        let node = report(Query::MiseInstalled, &[("node", "22.0.0")]);
        let npm = report(Query::NpmGlobals, &[]);
        let p = planning::plan(
            &m,
            Platform::Ubuntu,
            &BTreeSet::from(["base".into()]),
            &Observations {
                catalogs: [("node".into(), &node), ("cli".into(), &npm)].into(),
                binaries: [
                    (
                        "node".into(),
                        BinaryObservation::Candidates {
                            paths: vec![PathBuf::from("/chosen/node")],
                        },
                    ),
                    ("cli".into(), BinaryObservation::Missing),
                ]
                .into(),
            },
        )
        .unwrap();
        let row = p.rows.iter().find(|r| r.id == "cli").unwrap();
        let v = serde_json::to_value(&row.install_preview).unwrap();
        let c = &v["command"];
        assert_eq!(c["executable"], "/chosen/node");
        assert_eq!(
            c["arguments"],
            serde_json::json!([
                "/chosen/npm-cli.js",
                "install",
                "--global",
                "--prefix",
                "/chosen/prefix",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--logs-max=0",
                "--update-notifier=false",
                package
            ])
        );
        assert!(
            c["effects"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(
                    "skipped_scripts_may_leave_package_unusable"
                ))
        );
        assert_eq!(c["execution_authorized"], false);
        assert!(c["environment"].get("NODE_OPTIONS").is_none());
        assert!(c["environment"].get("NPM_CONFIG_USERCONFIG").is_none());
    }
}
#[test]
fn apt_defers_privileged_handoff_and_never_invents_an_install_executable_from_dpkg_query() {
    let p = plan_one(
        Provider::Apt,
        Query::DpkgPackages,
        None,
        &[],
        BinaryObservation::Missing,
    );
    let v = preview(&p);
    assert_eq!(
        v,
        serde_json::json!({"state":"deferred","reason":"privileged_handoff_not_designed"})
    );
    assert!(!serde_json::to_string(&v).unwrap().contains("apt-get"));
    assert!(v.get("command").is_none());
}
#[test]
fn existing_defaults_and_latest_have_no_command_even_with_missing_or_unattributed_binaries() {
    for policy in [None, Some("latest")] {
        for binary in [
            BinaryObservation::NotObserved,
            BinaryObservation::Missing,
            BinaryObservation::Candidates {
                paths: vec!["/unattributed/tool".into()],
            },
        ] {
            let p = plan_one(
                Provider::Mise,
                Query::MiseInstalled,
                policy,
                &[("tool", "1.0.0")],
                binary,
            );
            assert_eq!(
                preview(&p),
                serde_json::json!({"state":"no_package_change"})
            );
        }
    }
}
#[test]
fn blocked_unknown_ambiguous_mismatched_or_unverifiable_inputs_have_no_command() {
    for (policy, values, binary) in [
        (None, vec![], BinaryObservation::NotObserved),
        (None, vec![], BinaryObservation::Unavailable),
        (
            None,
            vec![],
            BinaryObservation::Candidates {
                paths: vec!["/other/tool".into()],
            },
        ),
        (
            None,
            vec![("core:tool", "1.0.0")],
            BinaryObservation::Missing,
        ),
        (
            Some("22"),
            vec![("tool", "20.0.0")],
            BinaryObservation::Missing,
        ),
        (Some("022"), vec![], BinaryObservation::Missing),
    ] {
        let p = plan_one(
            Provider::Mise,
            Query::MiseInstalled,
            policy,
            &values,
            binary,
        );
        assert_eq!(preview(&p), serde_json::json!({"state":"blocked"}));
    }
    let m = manifest(vec![tool("tool", Provider::Mise, "tool", None)]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &BTreeSet::from(["base".into()]),
        &Observations::default(),
    )
    .unwrap();
    assert_eq!(preview(&p)["state"], "blocked");
}
#[test]
fn command_metadata_is_bounded_per_description_and_across_the_plan() {
    use dort_core::tools::install_preview::{MAX_COMMAND_BYTES, MAX_TOTAL_COMMAND_BYTES};
    let mut r = report(Query::MiseInstalled, &[]);
    r.description.home = PathBuf::from(format!("/{}", "a".repeat(14_000)));
    let m = manifest(vec![tool("tool", Provider::Mise, "tool", None)]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &BTreeSet::from(["base".into()]),
        &Observations {
            catalogs: [("tool".into(), &r)].into(),
            binaries: [("tool".into(), BinaryObservation::Missing)].into(),
        },
    )
    .unwrap();
    assert_eq!(
        preview(&p),
        serde_json::json!({"state":"deferred","reason":"description_limit"})
    );
    r.description.home = PathBuf::from(format!("/{}", "a".repeat(2_000)));
    let m = manifest(
        (0..512)
            .map(|i| tool(&format!("t{i}"), Provider::Mise, &format!("t{i}"), None))
            .collect(),
    );
    let observations = Observations {
        catalogs: m.tools.iter().map(|t| (t.id.clone(), &r)).collect(),
        binaries: m
            .tools
            .iter()
            .map(|t| (t.id.clone(), BinaryObservation::Missing))
            .collect(),
    };
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &BTreeSet::from(["base".into()]),
        &observations,
    )
    .unwrap();
    let mut bytes = 0;
    let mut deferred = 0;
    for row in &p.rows {
        let value = serde_json::to_value(&row.install_preview).unwrap();
        if value["state"] == "preview_only" {
            let size = serde_json::to_vec(&row.install_preview).unwrap().len();
            assert!(size <= MAX_COMMAND_BYTES);
            bytes += size;
        } else {
            assert_eq!(value["reason"], "description_limit");
            deferred += 1;
        }
    }
    assert!(deferred > 0);
    assert!(bytes <= MAX_TOTAL_COMMAND_BYTES);
}

#[test]
fn dependency_review_blocks_dependent_command_and_metadata_serialization_never_grants_approval() {
    let mut child = tool("child", Provider::Mise, "child", None);
    child.requires = vec!["root".into()];
    let m = manifest(vec![child, tool("root", Provider::Mise, "root", None)]);
    let r = report(Query::MiseInstalled, &[]);
    let p = planning::plan(
        &m,
        Platform::Ubuntu,
        &BTreeSet::from(["base".into()]),
        &Observations {
            catalogs: [("root".into(), &r), ("child".into(), &r)].into(),
            binaries: [
                ("root".into(), BinaryObservation::Missing),
                ("child".into(), BinaryObservation::Missing),
            ]
            .into(),
        },
    )
    .unwrap();
    assert!(
        serde_json::to_value(
            &p.rows
                .iter()
                .find(|r| r.id == "child")
                .unwrap()
                .install_preview
        )
        .unwrap()
        .get("command")
        .is_none()
    );
    let root = p.rows.iter().find(|r| r.id == "root").unwrap();
    let value = serde_json::to_value(&root.install_preview).unwrap();
    assert_eq!(value["command"]["execution_authorized"], false);
    assert!(value.get("approval").is_none());
}
