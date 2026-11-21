mod support;
use dort_core::tools::inspection::{Failure, NpmBinding, Observation, Preview, Query};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use support::TestHome;

fn preview(home: &TestHome, query: Query, script: &str) -> Preview {
    let executable = home.host.home.join("fake-provider");
    fs::write(&executable, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    Preview::new(
        query,
        executable,
        home.host.home.clone(),
        vec![home.host.home.join("trusted-bin")],
    )
    .unwrap()
}

#[test]
fn preview_is_inert_inspectable_and_approval_is_single_use() {
    let home = TestHome::new();
    let p = preview(
        &home,
        Query::BrewFormulae,
        "printf ran > \"$HOME/marker\"; printf '{\"formulae\":[]}'",
    );
    assert!(!home.host.home.join("marker").exists());
    let description = serde_json::to_string(p.description()).unwrap();
    assert!(description.contains("--json=v2"));
    assert!(description.contains("configuration"));
    assert!(!home.host.home.join("marker").exists());
    assert!(
        matches!(p.approve().inspect().observation, Observation::Complete { installations } if installations.is_empty())
    );
    assert!(home.host.home.join("marker").exists());
}

#[test]
fn fixed_brew_query_has_sanitized_explicit_environment_and_root_cwd() {
    let home = TestHome::new();
    let p = preview(
        &home,
        Query::BrewFormulae,
        r#"
[ "$#" = 4 ] && [ "$1" = info ] && [ "$2" = --json=v2 ] && [ "$3" = --installed ] && [ "$4" = --formula ]
[ "$PWD" = / ]
[ "$XDG_CONFIG_HOME" = "$HOME/.config" ] && [ "$XDG_CACHE_HOME" = "$HOME/.cache" ]
[ "$XDG_DATA_HOME" = "$HOME/.local/share" ] && [ "$XDG_STATE_HOME" = "$HOME/.local/state" ]
[ "$PATH" = "$HOME/trusted-bin" ] && [ "$LC_ALL" = C ]
[ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ] && [ "$HOMEBREW_NO_ANALYTICS" = 1 ]
[ "$HOMEBREW_NO_INSTALL_FROM_API" = 1 ]
[ "${NODE_OPTIONS-unset}" = unset ] && [ "${NPM_CONFIG_USERCONFIG-unset}" = unset ]
[ "${RUBYOPT-unset}" = unset ] && [ "${BASH_ENV-unset}" = unset ]
[ "${MISE_CONFIG_FILE-unset}" = unset ] && [ "${GIT_CONFIG_GLOBAL-unset}" = unset ]
printf '{"formulae":[{"full_name":"ripgrep","installed":[{"version":"14.1.0"}]}]}'
"#,
    );
    match p.approve().inspect().observation {
        Observation::Complete { installations } => assert_eq!(installations[0].package, "ripgrep"),
        other => panic!("fixture query failed: {other:?}"),
    }
}

#[test]
fn failed_missing_and_malformed_queries_are_not_healthy_empty_or_raw_diagnostics() {
    for (script, expected) in [
        (
            "printf PRIVATE; printf PRIVATE >&2; exit 2",
            Failure::QueryFailed,
        ),
        ("printf PRIVATE", Failure::Malformed),
    ] {
        let home = TestHome::new();
        let report = preview(&home, Query::BrewFormulae, script)
            .approve()
            .inspect();
        assert!(
            matches!(report.observation, Observation::Unavailable { reason } if reason == expected)
        );
        assert!(!format!("{report:?}").contains("PRIVATE"));
        assert!(!serde_json::to_string(&report).unwrap().contains("PRIVATE"));
    }
    let home = TestHome::new();
    let p = Preview::new(
        Query::BrewFormulae,
        home.host.home.join("missing"),
        home.host.home.clone(),
        vec![PathBuf::from("/no-external-programs")],
    )
    .unwrap();
    assert!(matches!(
        p.approve().inspect().observation,
        Observation::Unavailable {
            reason: Failure::Unavailable
        }
    ));
}

#[test]
fn npm_uses_selected_node_cli_and_prefix_and_retains_runtime_binding() {
    let home = TestHome::new();
    // This fake Node models invocation, without requiring installed Node/npm or
    // interpreting any JavaScript. The CLI is deliberately not executable.
    let fake_node = preview(
        &home,
        Query::BrewFormulae,
        r#"
[ "$#" = 13 ] && [ "$1" = "$HOME/npm-cli.js" ]
shift
[ "$1" = ls ] && [ "$2" = --global ] && [ "$3" = --json ] && [ "$4" = --depth=0 ]
[ "$5" = --offline ] && [ "$6" = --ignore-scripts ] && [ "$7" = --logs-max=0 ]
[ "$8" = --update-notifier=false ] && [ "$9" = --audit=false ]
shift 9
[ "$1" = --fund=false ] && [ "$2" = --prefix ] && [ "$3" = "$HOME/global-prefix" ]
[ "${NODE_OPTIONS-unset}" = unset ] && [ "${NODE_PATH-unset}" = unset ]
printf '{"dependencies":{"@scope/pkg":{"version":"1.0.0","resolved":"PRIVATE"}}}'
"#,
    );
    let node = fake_node.description().executable.clone();
    let npm_cli = home.host.home.join("npm-cli.js");
    fs::write(&npm_cli, "fixture, never executed by a shell").unwrap();
    let manifest = dort_core::Manifest::parse(include_str!("../examples/dort.toml")).unwrap();
    let binding = NpmBinding {
        runtime_tool: "node".into(),
        node,
        npm_cli,
        prefix: home.host.home.join("global-prefix"),
    };
    let p = Preview::npm(
        &manifest,
        binding,
        home.host.home.clone(),
        vec![home.host.home.join("trusted-bin")],
    )
    .unwrap();
    assert_eq!(
        p.description().npm_binding.as_ref().unwrap().runtime_tool,
        "node"
    );
    assert!(!home.host.home.join("global-prefix").exists());
    let report = p.approve().inspect();
    assert!(
        matches!(&report.observation, Observation::Complete { installations } if installations.len() == 1)
    );
    assert_eq!(report.description.npm_binding.unwrap().runtime_tool, "node");
    assert!(!home.host.home.join("global-prefix").exists());
}

#[test]
fn npm_rejects_unbound_queries_and_undeclared_or_relative_runtime_bindings() {
    let home = TestHome::new();
    assert!(matches!(
        Preview::new(
            Query::NpmGlobals,
            "/node".into(),
            home.host.home.clone(),
            vec!["/bin".into()]
        ),
        Err(Failure::InvalidNpmBinding)
    ));
    let manifest = dort_core::Manifest::parse(include_str!("../examples/dort.toml")).unwrap();
    let binding = NpmBinding {
        runtime_tool: "undeclared".into(),
        node: "/node".into(),
        npm_cli: "/npm-cli.js".into(),
        prefix: "/prefix".into(),
    };
    assert!(matches!(
        Preview::npm(
            &manifest,
            binding.clone(),
            home.host.home.clone(),
            vec!["/bin".into()]
        ),
        Err(Failure::InvalidNpmBinding)
    ));
    let mut binding = binding;
    binding.runtime_tool = "node".into();
    for field in ["node", "cli", "prefix"] {
        let mut invalid = binding.clone();
        match field {
            "node" => invalid.node = "relative".into(),
            "cli" => invalid.npm_cli = "relative".into(),
            _ => invalid.prefix = "relative".into(),
        }
        assert!(matches!(
            Preview::npm(
                &manifest,
                invalid,
                home.host.home.clone(),
                vec!["/bin".into()]
            ),
            Err(Failure::InvalidPath)
        ));
    }
}

#[test]
fn relative_non_utf8_and_implicit_path_search_are_rejected_without_execution() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for path in [
        PathBuf::from("brew"),
        PathBuf::from("/trusted/../brew"),
        PathBuf::from("/trusted/./brew"),
        PathBuf::from(OsString::from_vec(b"/bad\xff".to_vec())),
        PathBuf::from("/bad\0"),
    ] {
        assert!(matches!(
            Preview::new(
                Query::BrewFormulae,
                path,
                "/home".into(),
                vec!["/bin".into()]
            ),
            Err(Failure::InvalidPath)
        ));
    }
    for paths in [
        vec![],
        vec![PathBuf::from("")],
        vec![PathBuf::from("relative")],
        vec![PathBuf::from("/has:separator")],
    ] {
        assert!(matches!(
            Preview::new(
                Query::BrewFormulae,
                "/trusted/brew".into(),
                "/home".into(),
                paths
            ),
            Err(Failure::InvalidSearchPath)
        ));
    }
}
