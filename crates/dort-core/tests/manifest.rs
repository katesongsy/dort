mod support;
use dort_core::{
    Manifest,
    context::HostContext,
    platform::{Architecture, Platform},
};

const TOOL: &str = r#"
[[tools]]
id = "node"
binary = "node"
[tools.install]
macos = { provider = "mise", package = "node", version = "24" }
ubuntu = { provider = "mise", package = "node", version = "24" }
"#;
const FILE: &str = r#"
[[files]]
id = "shell"
source = "files/zshrc"
target = "~/.zshrc"
mode = "link"
"#;

fn invalid(body: &str, expected: &str) {
    let error = Manifest::parse(body).unwrap_err().to_string();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error}"
    );
}

#[test]
fn minimal_manifest_defaults_to_base() {
    let m = Manifest::parse("schema=1").unwrap();
    let fixture = support::TestHome::new();
    assert_eq!(
        m.active_profiles(&fixture.host, &[])
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["base"]
    );
}

#[test]
fn versioned_example_is_valid_on_both_platforms() {
    let manifest = Manifest::parse(include_str!("../../../examples/dort.toml")).unwrap();
    for platform in [Platform::Macos, Platform::Ubuntu] {
        let host = HostContext::new(
            "/home/example".into(),
            None,
            None,
            platform,
            Architecture::Aarch64,
        )
        .unwrap();
        manifest
            .validate_for_host(&host, &["dev".into()], None)
            .unwrap();
    }
}

#[test]
fn strict_schema_and_nested_fields() {
    invalid("schema=2", "unsupported manifest schema");
    invalid("schema=1\nsurprise=true", "unknown field");
    invalid("schema=1\n[profiles]\nunknown=[]", "unknown field");
    invalid(
        &format!("schema=1\n{TOOL}").replace("version = \"24\"", "command = \"anything\""),
        "unknown field",
    );
    invalid(
        &format!("schema=1\n{FILE}").replace("mode = \"link\"", "mode = \"execute\""),
        "unknown variant",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace("ubuntu =", "windows ="),
        "unknown variant",
    );
}

#[test]
fn duplicate_ids_are_rejected_but_tool_and_file_namespaces_are_distinct() {
    invalid(&format!("schema=1\n{TOOL}{TOOL}"), "duplicate tool IDs");
    invalid(&format!("schema=1\n{FILE}{FILE}"), "duplicate file IDs");
    Manifest::parse(&format!("schema=1\n{TOOL}{FILE}").replace("id = \"shell\"", "id = \"node\""))
        .unwrap();
}

#[test]
fn unknown_empty_and_duplicate_profiles_are_rejected() {
    invalid(
        &format!("schema=1\n{FILE}")
            .replace("mode = \"link\"", "mode = \"link\"\nprofiles=[\"dev\"]"),
        "unknown profile dev",
    );
    invalid(
        &format!("schema=1\n{FILE}").replace("mode = \"link\"", "mode = \"link\"\nprofiles=[]"),
        "profiles cannot be empty",
    );
    invalid(
        "schema=1\n[profiles]\navailable=[\"dev\", \"dev\"]",
        "duplicate available profiles",
    );
    invalid(
        "schema=1\n[profiles]\ndefault=[\"dev\"]",
        "unknown profile dev",
    );
}

#[test]
fn profile_union_is_deterministic_and_base_cannot_be_disabled() {
    let m = Manifest::parse(
        r#"schema=1
[profiles]
available=["work","dev","ubuntu"]
default=["dev"]
[profiles.platform]
ubuntu=["ubuntu"]
"#,
    )
    .unwrap();
    let fixture = support::TestHome::new();
    let active = m.active_profiles(&fixture.host, &["work".into()]).unwrap();
    assert_eq!(
        active.into_iter().collect::<Vec<_>>(),
        ["base", "dev", "ubuntu", "work"]
    );
    assert!(m.active_profiles(&fixture.host, &["typo".into()]).is_err());
}

#[test]
fn dependencies_must_exist_and_be_acyclic() {
    invalid(
        &format!("schema=1\n{FILE}").replace(
            "mode = \"link\"",
            "mode = \"link\"\nrequires=[\"starship\"]",
        ),
        "unknown required tool starship",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace(
            "binary = \"node\"",
            "binary = \"node\"\nrequires=[\"node\"]",
        ),
        "dependency cycle",
    );
}

#[test]
fn npm_runtime_is_an_implicit_ordered_dependency_and_cannot_create_cycles() {
    let npm = r#"
[[tools]]
id="pi"
binary="pi"
[tools.install]
ubuntu={provider="npm", package="@scope/cli", runtime_tool="node"}
macos={provider="npm", package="@scope/cli", runtime_tool="node"}
"#;
    let m = Manifest::parse(&format!("schema=1\n{TOOL}{npm}")).unwrap();
    let fixture = support::TestHome::new();
    let active = m.active_profiles(&fixture.host, &[]).unwrap();
    assert_eq!(
        m.dependency_order(Platform::Ubuntu, &active).unwrap(),
        ["node", "pi"]
    );
    invalid(
        &format!("schema=1\n{TOOL}{npm}")
            .replace("binary = \"node\"", "binary = \"node\"\nrequires=[\"pi\"]"),
        "dependency cycle",
    );
    invalid(
        &format!("schema=1\n{TOOL}{npm}")
            .replace("runtime_tool=\"node\"", "runtime_tool=\"missing\""),
        "unknown required tool missing",
    );
}

#[test]
fn inactive_dependencies_and_missing_platform_definitions_block_selection() {
    let text = format!("schema=1\n[profiles]\navailable=[\"dev\"]\n{TOOL}{FILE}")
        .replace("binary = \"node\"", "binary = \"node\"\nprofiles=[\"dev\"]")
        .replace("mode = \"link\"", "mode = \"link\"\nrequires=[\"node\"]");
    let m = Manifest::parse(&text).unwrap();
    let fixture = support::TestHome::new();
    assert!(
        m.validate_for_host(&fixture.host, &[], None)
            .unwrap_err()
            .to_string()
            .contains("inactive")
    );
    m.validate_for_host(&fixture.host, &["dev".into()], None)
        .unwrap();
    let m = Manifest::parse(&format!("schema=1\n{TOOL}").replace(
        "ubuntu = { provider = \"mise\", package = \"node\", version = \"24\" }",
        "",
    ))
    .unwrap();
    assert!(
        m.validate_for_host(&fixture.host, &[], None)
            .unwrap_err()
            .to_string()
            .contains("no installation definition")
    );
}

#[test]
fn provider_policy_rejects_unenforceable_versions_platforms_and_injection() {
    invalid(
        &format!("schema=1\n{TOOL}").replace("provider = \"mise\"", "provider = \"brew\""),
        "cannot enforce this version",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace("provider = \"mise\"", "provider = \"apt\""),
        "unsupported on this platform",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace("package = \"node\"", "package = \"--run-command\""),
        "invalid provider package",
    );
    invalid(
        &format!("schema=1\n{TOOL}")
            .replace("package = \"node\"", "package = \"node; touch /tmp/x\""),
        "invalid provider package",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace("provider = \"mise\"", "provider = \"npm\""),
        "npm requires an explicit runtime_tool",
    );
    invalid(
        &format!("schema=1\n{TOOL}").replace("version = \"24\"", "version = \"^24\""),
        "cannot enforce this version",
    );
}

#[test]
fn source_and_target_paths_and_permissions_are_validated() {
    for source in [
        "../zshrc",
        "/etc/zshrc",
        "files/../zshrc",
        "files/./zshrc",
        ".git/config",
        "dort.toml",
        "~/zshrc",
    ] {
        invalid(
            &format!("schema=1\n{FILE}").replace("files/zshrc", source),
            "source must be repository-relative",
        );
    }
    for target in ["relative", "~/../outside", "~", "/"] {
        invalid(
            &format!("schema=1\n{FILE}").replace("~/.zshrc", target),
            "target must be",
        );
    }
    invalid(
        &format!("schema=1\n{FILE}").replace(
            "mode = \"link\"",
            "mode = \"template\"\npermissions=\"0999\"",
        ),
        "four octal",
    );
    invalid(
        &format!("schema=1\n{FILE}")
            .replace("mode = \"link\"", "mode = \"secret\"\npermissions=\"0644\""),
        "secret permissions",
    );
    invalid(
        &format!("schema=1\n{FILE}")
            .replace("mode = \"link\"", "mode = \"link\"\npermissions=\"0600\""),
        "not supported for link",
    );
}

#[test]
fn destinations_conflict_including_expanded_home_aliases() {
    let second = FILE.replace("\"shell\"", "\"other\"");
    invalid(
        &format!("schema=1\n{FILE}{second}"),
        "conflicting destinations",
    );
    let fixture = support::TestHome::new();
    let second = second.replace(
        "~/.zshrc",
        &fixture.host.home.join(".zshrc").display().to_string(),
    );
    let m = Manifest::parse(&format!("schema=1\n{FILE}{second}")).unwrap();
    assert!(
        m.validate_for_host(&fixture.host, &[], None)
            .unwrap_err()
            .to_string()
            .contains("expanded destinations")
    );
}
