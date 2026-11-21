mod support;
use dort_core::{Manifest, platform::Platform};

#[test]
fn runtime_dependency_order_is_platform_independent() {
    let manifest = Manifest::parse(
        r#"schema=1
[[tools]]
id="node-a"
binary="node"
[tools.install]
provider="npm"
package="a"
runtime_tool="node-b"
[[tools]]
id="node-b"
binary="node"
[tools.install]
provider="brew"
package="node"
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
        manifest.dependency_order(Platform::Linux, &active).unwrap(),
        ["node-b", "node-a"]
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
provider="brew"
package="node"
[[tools]]
id="pi"
binary="pi"
[tools.install]
provider="npm"
package="pi"
runtime_tool="node"
"#,
    )
    .unwrap();
    let f = support::TestHome::new();
    let active = manifest.active_profiles(&f.host, &[]).unwrap();
    assert!(
        manifest
            .dependency_order(Platform::Linux, &active)
            .unwrap_err()
            .to_string()
            .contains("inactive")
    );
    let active = manifest.active_profiles(&f.host, &["dev".into()]).unwrap();
    assert_eq!(
        manifest.dependency_order(Platform::Linux, &active).unwrap(),
        ["node", "pi"]
    );
}
