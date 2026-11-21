use super::inspection_defaults::Inputs;
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    host: HostContext,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Linux, Architecture::X86_64).unwrap();
        Self {
            _temp: temp,
            root,
            host,
        }
    }
    fn program(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "#!/bin/sh\nprintf ran > \"$HOME/calls\"\nexit 99\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn manifest(&self, tools: &str) -> PathBuf {
        let path = self.root.join("dort.toml");
        fs::write(&path, format!("schema = 1\n{tools}")).unwrap();
        path
    }
}
#[test]
fn named_tool_settings_are_process_free_and_reject_conflicts_or_internal_inputs() {
    let f = Fixture::new();
    let first = f.program("bin/brew");
    let second = f.program("other/brew");
    let manifest=f.manifest("[[tools]]\nid='a'\nbinary='a'\n[tools.install]\nprovider='brew'\npackage='a'\n[[tools]]\nid='b'\nbinary='b'\n[tools.install]\nprovider='brew'\npackage='b'\n");
    let input = Inputs {
        search_dirs: vec![f.root.join("bin")],
        ..Default::default()
    };
    let chosen = input
        .clone()
        .with_settings(
            &f.host,
            &manifest,
            &[format!("a={}", second.display())],
            &[],
            &[],
        )
        .unwrap();
    assert_eq!(chosen.overrides.len(), 1);
    assert_eq!(chosen.overrides[0].executable, second);
    for values in [
        vec!["a=relative".into()],
        vec!["PRIVATE_BAD_INPUT".into()],
        vec![
            format!("a={}", first.display()),
            format!("a={}", first.display()),
        ],
        vec![
            format!("a={}", first.display()),
            format!("b={}", second.display()),
        ],
    ] {
        let error = input
            .clone()
            .with_settings(&f.host, &manifest, &values, &[], &[])
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains("PRIVATE_BAD_INPUT"));
    }
    assert!(
        input
            .with_settings(
                &f.host,
                &manifest,
                &[],
                &[format!("a={}", first.display())],
                &[]
            )
            .is_err()
    );
    assert!(!f.host.home.join("calls").exists());
}
#[test]
fn only_absolute_executable_metadata_and_path_precedence_are_used() {
    let f = Fixture::new();
    let first = f.program("first/brew");
    f.program("second/brew");
    f.program("first/node");
    fs::write(f.root.join("first/npm"), "not executable").unwrap();
    let defaults = Inputs {
        search_dirs: vec![
            PathBuf::new(),
            "relative".into(),
            f.root.join("first"),
            f.root.join("first"),
            f.root.join("second"),
        ],
        ..Default::default()
    }
    .detect(&f.host, None);
    assert_eq!(defaults.choices.len(), 2);
    assert_eq!(
        defaults.choices[0].request.as_ref().unwrap().executable,
        first
    );
    assert_eq!(
        defaults.choices[0]
            .request
            .as_ref()
            .unwrap()
            .search_dirs
            .len(),
        2
    );
    let ready = |i: usize| {
        defaults.choices[i]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .is_ok()
    };
    assert!(ready(0));
    assert!(!ready(1));
    assert!(!f.host.home.join("calls").exists());
}
#[test]
fn npm_needs_a_unique_declared_runtime_and_complete_colocated_layout() {
    let f = Fixture::new();
    let node = f.program("runtime/bin/node");
    f.program("other/bin/npm"); // A launcher is never enough and is never run.
    let inputs = Inputs {
        search_dirs: vec![
            node.parent().unwrap().to_path_buf(),
            f.root.join("other/bin"),
        ],
        ..Default::default()
    };
    let manifest = f.manifest("[[tools]]\nid='node-runtime'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n");
    let incomplete = inputs.clone().detect(&f.host, Some(manifest.clone()));
    assert!(
        incomplete.choices[1]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .is_err()
    );
    assert_eq!(
        incomplete.choices[1]
            .request
            .as_ref()
            .unwrap()
            .runtime_tool
            .as_deref(),
        Some("node-runtime")
    );
    let cli = f.root.join("runtime/lib/node_modules/npm/bin/npm-cli.js");
    fs::create_dir_all(cli.parent().unwrap()).unwrap();
    fs::write(&cli, "DO NOT EXECUTE").unwrap();
    let complete = inputs.clone().detect(&f.host, Some(manifest));
    let request = complete.choices[1].request.as_ref().unwrap();
    assert_eq!(request.prefix.as_ref().unwrap(), &f.root.join("runtime"));
    assert_eq!(request.npm_cli.as_ref().unwrap(), &cli);
    assert!(
        complete.choices[1]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .is_ok()
    );
    let ambiguous =
        f.manifest("[[tools]]\nid='one'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n[[tools]]\nid='two'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n");
    assert!(
        inputs.clone().detect(&f.host, Some(ambiguous)).choices[1]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .is_err()
    );
    assert!(
        inputs.detect(&f.host, None).choices[1]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .is_err()
    );
    assert!(!f.host.home.join("calls").exists());
}

#[test]
fn retained_overrides_are_prepared_without_running_or_silently_replaced() {
    use super::tools::Request;
    use dort_core::tools::inspection::Query;
    let f = Fixture::new();
    f.program("bin/brew");
    let selected = f.program("chosen/brew");
    let request = Request {
        query: Query::BrewFormulae,
        executable: selected.clone(),
        search_dirs: vec![f.root.join("chosen")],
        manifest: None,
        runtime_tool: None,
        npm_cli: None,
        prefix: None,
    };
    let defaults = Inputs {
        search_dirs: vec![f.root.join("bin")],
        overrides: vec![request],
    }
    .detect(&f.host, None);
    assert_eq!(
        defaults.choices[0]
            .request
            .clone()
            .unwrap()
            .prepare(&f.host)
            .unwrap()
            .review()
            .provider
            .executable,
        selected
    );
    assert!(!f.host.home.join("calls").exists());
}
