mod support;
use dort_core::{Manifest, platform::Platform};

#[test]
fn mutually_exclusive_runtime_edges_do_not_form_a_false_cycle() {
    let manifest = Manifest::parse(
        r#"schema=1
[[tools]]
id="node-a"
binary="node"
[tools.install]
macos={provider="npm",package="a",runtime_tool="node-b"}
ubuntu={provider="mise",package="node"}
[[tools]]
id="node-b"
binary="node"
[tools.install]
macos={provider="mise",package="node"}
ubuntu={provider="npm",package="b",runtime_tool="node-a"}
"#,
    )
    .unwrap();
    let f = support::TestHome::new();
    let active = manifest.active_profiles(&f.host, &[]).unwrap();
    assert_eq!(
        manifest.dependency_order(Platform::Macos, &active).unwrap(),
        ["node-b", "node-a"]
    );
    assert_eq!(
        manifest
            .dependency_order(Platform::Ubuntu, &active)
            .unwrap(),
        ["node-a", "node-b"]
    );
}

#[test]
fn ordering_does_not_silently_drop_inactive_dependencies() {
    let manifest = Manifest::parse(
        r#"schema=1
[profiles]
available=["dev"]
[[tools]]
id="node"
binary="node"
profiles=["dev"]
[tools.install]
ubuntu={provider="mise",package="node"}
[[tools]]
id="pi"
binary="pi"
[tools.install]
ubuntu={provider="npm",package="pi",runtime_tool="node"}
"#,
    )
    .unwrap();
    let f = support::TestHome::new();
    let active = manifest.active_profiles(&f.host, &[]).unwrap();
    assert!(
        manifest
            .dependency_order(Platform::Ubuntu, &active)
            .unwrap_err()
            .to_string()
            .contains("inactive")
    );
    let active = manifest.active_profiles(&f.host, &["dev".into()]).unwrap();
    assert_eq!(
        manifest
            .dependency_order(Platform::Ubuntu, &active)
            .unwrap(),
        ["node", "pi"]
    );
}
