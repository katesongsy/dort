mod support;
use dort_core::{
    Manifest,
    model::{InstallSpec, Provider, Tool},
    tools::{
        binaries::{self, ProbeState},
        planning::BinaryObservation,
    },
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    },
    path::PathBuf,
};
use support::TestHome;
fn manifest(names: &[&str]) -> Manifest {
    Manifest {
        schema: 1,
        profiles: Default::default(),
        files: vec![],
        tools: names
            .iter()
            .map(|name| Tool {
                id: (*name).into(),
                binary: (*name).into(),
                profiles: vec!["base".into()],
                requires: vec![],
                install: InstallSpec {
                    provider: Provider::Brew,
                    package: (*name).into(),
                    version: None,
                    runtime_tool: None,
                },
            })
            .collect(),
    }
}
fn active() -> BTreeSet<String> {
    ["base".into()].into()
}

#[test]
fn empty_existing_explicit_scope_is_missing_not_a_global_path_search() {
    let h = TestHome::new();
    let bin = h.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    let ignored = h.host.home.join("unsearched");
    fs::create_dir(&ignored).unwrap();
    fs::write(ignored.join("node"), "PRIVATE").unwrap();
    let r = binaries::observe(&manifest(&["node"]), &active(), &[bin]).unwrap();
    assert!(r.available());
    assert_eq!(r.tools["node"].observation, BinaryObservation::Missing);
    assert_eq!(r.tools["node"].probes.len(), 1);
    assert!(!serde_json::to_string(&r).unwrap().contains("PRIVATE"));
}
#[test]
fn all_occupied_kinds_are_unattributed_candidates_without_content_reads_or_execution() {
    let h = TestHome::new();
    let bin = h.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    fs::write(
        bin.join("run"),
        format!(
            "#!/bin/sh\nprintf ran > '{}'\nPRIVATE",
            h.host.home.join("executed").display()
        ),
    )
    .unwrap();
    fs::set_permissions(bin.join("run"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(bin.join("plain"), "PRIVATE ordinary file").unwrap();
    fs::create_dir(bin.join("directory")).unwrap();
    symlink("../not-present", bin.join("link")).unwrap();
    let _socket = UnixListener::bind(bin.join("socket")).unwrap();
    let r = binaries::observe(
        &manifest(&["run", "plain", "directory", "link", "socket"]),
        &active(),
        std::slice::from_ref(&bin),
    )
    .unwrap();
    assert!(r.available());
    for name in ["run", "plain", "directory", "link", "socket"] {
        assert_eq!(
            r.tools[name].observation,
            BinaryObservation::Candidates {
                paths: vec![bin.join(name)]
            }
        );
        assert!(matches!(
            r.tools[name].probes[0].observation,
            ProbeState::Present { .. }
        ));
    }
    assert!(
        matches!(&r.tools["link"].probes[0].observation, ProbeState::Present { link_target: Some(target), .. } if target == &PathBuf::from("../not-present"))
    );
    assert!(!h.host.home.join("executed").exists());
    assert!(!serde_json::to_string(&r).unwrap().contains("PRIVATE"));
}
#[test]
fn missing_or_symlinked_directories_make_the_scope_unavailable_not_healthy_empty() {
    let h = TestHome::new();
    let bin = h.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    let alias = h.host.home.join("alias");
    symlink(&bin, &alias).unwrap();
    for invalid in [
        alias,
        h.host.home.join("missing"),
        h.host.home.join("alias/subdir"),
    ] {
        let r =
            binaries::observe(&manifest(&["node"]), &active(), &[bin.clone(), invalid]).unwrap();
        assert!(!r.available());
        assert_eq!(r.tools["node"].observation, BinaryObservation::Unavailable);
    }
}
#[test]
fn non_utf8_directory_or_link_target_is_explicitly_unavailable_never_lossy() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let h = TestHome::new();
    let bin = h.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    let bad = PathBuf::from(OsString::from_vec(b"/bad\xff".to_vec()));
    assert!(binaries::observe(&manifest(&["node"]), &active(), &[bad]).is_err());
    symlink(OsString::from_vec(b"bad\xff".to_vec()), bin.join("node")).unwrap();
    let r = binaries::observe(&manifest(&["node"]), &active(), &[bin]).unwrap();
    assert_eq!(r.tools["node"].observation, BinaryObservation::Unavailable);
    assert!(!serde_json::to_string(&r).unwrap().contains("\u{fffd}"));
}
#[test]
fn profiles_and_scope_limits_are_validated_without_environment_or_directory_discovery() {
    let mut m = manifest(&["node", "other"]);
    m.profiles.available = vec!["extra".into()];
    m.tools[1].profiles = vec!["extra".into()];
    let h = TestHome::new();
    let bin = h.host.home.join("bin");
    fs::create_dir(&bin).unwrap();
    let r = binaries::observe(&m, &active(), std::slice::from_ref(&bin)).unwrap();
    assert_eq!(r.tools.len(), 1);
    for dirs in [
        vec![],
        vec![bin.clone(), bin.clone()],
        vec![PathBuf::from("relative")],
        vec![PathBuf::from(format!("{}/.", bin.display()))],
        vec![PathBuf::from("/bad:separator")],
        (0..33)
            .map(|i| PathBuf::from(format!("/scope/{i}")))
            .collect(),
    ] {
        assert!(binaries::validate(&m, &active(), &dirs).is_err());
    }
    assert!(binaries::validate(&m, &BTreeSet::new(), &[bin]).is_err());
}
