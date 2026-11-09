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
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
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
fn only_absolute_executable_metadata_and_path_precedence_are_used() {
    let f = Fixture::new();
    let first = f.program("first/brew");
    f.program("second/brew");
    f.program("first/mise");
    fs::write(f.root.join("first/dpkg-query"), "not executable").unwrap();
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
    assert_eq!(defaults.choices.len(), 4);
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
    assert!(defaults.choices[0].selected && defaults.choices[2].selected);
    assert!(!defaults.choices[1].selected && !defaults.choices[3].selected);
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
    let manifest = f.manifest("[[tools]]\nid='node-runtime'\nbinary='node'\n[tools.install.ubuntu]\nprovider='mise'\npackage='node'\n");
    let incomplete = inputs.clone().detect(&f.host, Some(manifest.clone()));
    assert!(!incomplete.choices[3].selected);
    assert_eq!(
        incomplete.choices[3]
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
    let request = complete.choices[3].request.as_ref().unwrap();
    assert_eq!(request.prefix.as_ref().unwrap(), &f.root.join("runtime"));
    assert_eq!(request.npm_cli.as_ref().unwrap(), &cli);
    assert!(complete.choices[3].preview.is_some() && complete.choices[3].selected);
    let ambiguous =
        f.manifest("[[tools]]\nid='one'\nbinary='node'\n[tools.install.ubuntu]\nprovider='mise'\npackage='node'\n[[tools]]\nid='two'\nbinary='node'\n[tools.install.ubuntu]\nprovider='mise'\npackage='node'\n");
    assert!(!inputs.clone().detect(&f.host, Some(ambiguous)).choices[3].selected);
    assert!(!inputs.detect(&f.host, None).choices[3].selected);
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
            .preview
            .as_ref()
            .unwrap()
            .review()
            .provider
            .executable,
        selected
    );
    assert!(!f.host.home.join("calls").exists());
}
