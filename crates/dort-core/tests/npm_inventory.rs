mod support;
use dort_core::tools::inspection::{Failure, Observation, Preview, Query};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
fn program(path: &std::path::Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn fixture(body: &str) -> (support::TestHome, PathBuf) {
    let home = support::TestHome::new();
    let bin = home.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    program(&bin.join("node"), body);
    program(&bin.join("npm-launcher"), "exec node \"$@\"");
    symlink("npm-launcher", bin.join("npm")).unwrap();
    (home, bin)
}
#[test]
fn path_npm_inventory_is_inert_without_manifest_and_uses_the_reported_prefix() {
    let (home, bin) = fixture(
        r#"
[ "$PWD" = / ] && [ "$LC_ALL" = C ] && [ "$PATH" = "$HOME/bin" ]
[ "${NODE_OPTIONS-unset}" = unset ] && [ "${NPM_CONFIG_PREFIX-unset}" = unset ]
[ "$XDG_CONFIG_HOME" = "$HOME/.config" ]
if [ "$1" = prefix ]; then
[ "$#" = 8 ] && [ "$2" = --global ] && [ "$3" = --offline ]
printf P >> "$HOME/calls"
printf '%s\n' "$HOME/configured globals"
else
[ "$#" = 12 ] && [ "$1" = ls ] && [ "$2" = --global ] && [ "$3" = --json ] && [ "$4" = --depth=0 ]
for arg do :; done
[ "$arg" = "$HOME/configured globals" ]
printf L >> "$HOME/calls"
printf '{"dependencies":{"formatter":{"version":"1.2.3","PRIVATE_BODY":"withheld"}}}'
fi
"#,
    );
    let p = Preview::npm_inventory(bin.join("npm"), home.host.home.clone(), vec![bin]).unwrap();
    assert!(!home.host.home.join("calls").exists());
    assert_eq!(p.description().query, Query::NpmGlobals);
    assert!(p.description().npm_binding.is_none());
    assert_eq!(p.prefix_probe().unwrap().arguments[0], "prefix");
    assert!(p.description().warning.contains("PATH"));
    let report = p.approve().inspect();
    assert!(
        matches!(&report.observation, Observation::Complete { installations } if installations.len()==1 && installations[0].package=="formatter")
    );
    assert_eq!(
        fs::read_to_string(home.host.home.join("calls")).unwrap(),
        "PL"
    );
    assert!(report.description.npm_binding.is_none());
    assert_eq!(
        report.description.npm_global_prefix,
        Some(home.host.home.join("configured globals"))
    );
    assert!(
        !home.host.home.join("configured globals").exists(),
        "Provider path was not filesystem-observed or created"
    );
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("PRIVATE_BODY")
    );
    assert!(
        matches!(
            Preview::new(
                Query::NpmGlobals,
                home.host.home.join("npm"),
                home.host.home.clone(),
                vec![home.host.home.clone()]
            ),
            Err(Failure::InvalidNpmBinding)
        ),
        "Managed npm still needs its explicit binding"
    );
}
#[test]
fn failed_unsafe_or_oversized_prefix_never_runs_the_list_or_discloses_raw_output() {
    for (body, expected) in [
        (
            "printf PRIVATE_PREFIX; printf PRIVATE_DIAGNOSTIC >&2; exit 9",
            Failure::QueryFailed,
        ),
        ("printf '/PRIVATE_PREFIX\\n/another\\n'", Failure::Malformed),
        ("printf 'PRIVATE_PREFIX'", Failure::Malformed),
        ("printf '/a/../PRIVATE_PREFIX\\n'", Failure::Malformed),
        (
            "i=0; while [ $i -lt 10000 ]; do printf A; i=$((i+1)); done",
            Failure::OutputLimit,
        ),
    ] {
        let script = format!(
            "if [ \"$1\" = prefix ]; then\nprintf P >> \"$HOME/calls\"\n{body}\nelse\nprintf L >> \"$HOME/calls\"\nprintf '{{\"dependencies\":{{}}}}'\nfi"
        );
        let (home, bin) = fixture(&script);
        let report = Preview::npm_inventory(bin.join("npm"), home.host.home.clone(), vec![bin])
            .unwrap()
            .approve()
            .inspect();
        assert!(
            matches!(report.observation, Observation::Unavailable {reason} if reason == expected),
            "{report:?}"
        );
        assert_eq!(
            fs::read_to_string(home.host.home.join("calls")).unwrap(),
            "P"
        );
        let metadata = serde_json::to_string(&report).unwrap();
        assert!(!metadata.contains("PRIVATE_PREFIX") && !metadata.contains("PRIVATE_DIAGNOSTIC"));
        assert!(report.description.npm_global_prefix.is_none());
    }
}
#[test]
fn unavailable_or_malformed_list_is_not_a_healthy_empty_inventory() {
    for (body, expected) in [
        ("printf PRIVATE_BODY; exit 7", Failure::QueryFailed),
        ("printf PRIVATE_BODY", Failure::Malformed),
    ] {
        let script = format!(
            "if [ \"$1\" = prefix ]; then printf '%s\\n' \"$HOME/globals\"; else {body}; fi"
        );
        let (home, bin) = fixture(&script);
        let report = Preview::npm_inventory(bin.join("npm"), home.host.home.clone(), vec![bin])
            .unwrap()
            .approve()
            .inspect();
        assert!(
            matches!(report.observation, Observation::Unavailable { reason } if reason == expected)
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("PRIVATE_BODY")
        );
    }
}
