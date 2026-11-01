use assert_cmd::Command;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use tempfile::TempDir;

struct Sandbox {
    _temporary: TempDir,
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        Self {
            _temporary: temporary,
            home,
            root,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dort"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("PATH", "/no-external-programs")
            .current_dir(&self.root);
        command
    }

    fn manifest(&self, text: &str) -> PathBuf {
        let repository = self.root.join("repository");
        fs::create_dir_all(&repository).unwrap();
        let path = repository.join("dort.toml");
        fs::write(&path, text).unwrap();
        path
    }

    fn assert_readonly(&self) {
        assert!(!self.home.join(".config").exists());
        assert!(!self.home.join(".local").exists());
        assert_eq!(fs::read_dir(&self.home).unwrap().count(), 0);
    }
}

#[test]
fn help_and_version_work_without_a_host_context() {
    let sandbox = Sandbox::new();
    sandbox
        .command()
        .env_remove("HOME")
        .arg("--help")
        .assert()
        .success();
    sandbox.command().arg("--version").assert().success();
    sandbox.assert_readonly();
}

#[test]
fn doctor_reports_host_without_creating_state_or_running_external_commands() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .args(["--json", "doctor"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["mode"], "read-only foundation");
    assert_eq!(json["registered_repositories"], 0);
    assert_eq!(json["host"]["home"], sandbox.home.to_str().unwrap());
    sandbox.assert_readonly();
}

#[test]
fn example_check_reports_unsupported_features_without_deployment() {
    let sandbox = Sandbox::new();
    let path = sandbox.manifest(include_str!("../../../examples/dort.toml"));
    let output = sandbox
        .command()
        .args(["--json", "manifest", "check", "--manifest"])
        .arg(&path)
        .args(["--profile", "dev"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["selected_tools"], 3);
    assert_eq!(json["filesystem_and_tools_inspected"], false);
    assert!(json["unsupported_capabilities"].as_array().unwrap().len() >= 3);
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        include_str!("../../../examples/dort.toml")
    );
    sandbox.assert_readonly();
}

#[test]
fn invalid_manifest_returns_json_error_and_nonzero_exit() {
    let sandbox = Sandbox::new();
    let path = sandbox.manifest("schema=99");
    let assertion = sandbox
        .command()
        .args(["--json", "manifest", "check", "--manifest"])
        .arg(path)
        .assert()
        .code(2);
    let output = assertion.get_output();
    assert!(output.stdout.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(json["kind"], "validation");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("unsupported manifest schema")
    );
    sandbox.assert_readonly();
}

#[test]
fn explicit_repository_override_does_not_register_or_write() {
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest("schema=1");
    sandbox
        .command()
        .arg("--repo")
        .arg(manifest.parent().unwrap())
        .args(["manifest", "check"])
        .assert()
        .success();
    sandbox.assert_readonly();
}

#[test]
fn missing_repository_and_relative_xdg_fail_without_writes() {
    let sandbox = Sandbox::new();
    sandbox
        .command()
        .args(["manifest", "check"])
        .assert()
        .code(2);
    sandbox
        .command()
        .env("XDG_STATE_HOME", "relative")
        .arg("doctor")
        .assert()
        .code(2);
    sandbox.assert_readonly();
}

#[test]
fn unknown_profile_and_conflicting_selection_flags_are_errors() {
    let sandbox = Sandbox::new();
    let path = sandbox.manifest("schema=1");
    sandbox
        .command()
        .args(["manifest", "check", "--manifest"])
        .arg(&path)
        .args(["--profile", "typo"])
        .assert()
        .code(2);
    sandbox
        .command()
        .arg("--repo")
        .arg(&sandbox.root)
        .args(["manifest", "check", "--manifest"])
        .arg(&path)
        .assert()
        .code(2);
    sandbox.assert_readonly();
}

#[test]
fn discovery_needs_no_repository_and_does_not_read_candidate_contents() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.home.join(".zshrc"), "password=DO-NOT-PRINT").unwrap();
    fs::create_dir(sandbox.home.join(".ssh")).unwrap();
    fs::write(sandbox.home.join(".ssh/id_ed25519"), "DO-NOT-PRINT").unwrap();
    let output = sandbox
        .command()
        .args(["--json", "files", "discover"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["candidates"][0]["entry"]["sha256"],
        serde_json::Value::Null
    );
    assert!(!text.contains("DO-NOT-PRINT"));
    assert!(!text.contains("id_ed25519"));
    assert!(!sandbox.home.join(".config").exists());
    assert!(!sandbox.home.join(".local").exists());
}

#[test]
fn adoption_json_and_human_previews_do_not_write_any_files_or_print_contents() {
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest("schema=1\n");
    let target = sandbox.home.join(".zshrc");
    fs::write(&target, "ADOPTION-CONTENT-SENTINEL").unwrap();
    let root = manifest.parent().unwrap();
    let output = sandbox
        .command()
        .arg("--repo")
        .arg(root)
        .args(["--json", "files", "plan", "adopt"])
        .arg(&target)
        .args(["--id", "zshrc", "--source", "files/zshrc"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["preview_only"], true);
    assert_eq!(json["schema"], 1);
    assert_eq!(json["files"][0]["effects"][2]["action"], "copy_source");
    assert!(!text.contains("ADOPTION-CONTENT-SENTINEL"));
    let output = sandbox
        .command()
        .arg("--repo")
        .arg(root)
        .args(["files", "plan", "adopt"])
        .arg(&target)
        .args(["--id", "zshrc", "--source", "files/zshrc"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(output).unwrap();
    assert!(human.contains("Backup"));
    assert!(human.contains("AddMapping"));
    assert!(human.contains("permissions"));
    assert!(!human.contains("ADOPTION-CONTENT-SENTINEL"));
    assert_eq!(fs::read_to_string(&manifest).unwrap(), "schema=1\n");
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "ADOPTION-CONTENT-SENTINEL"
    );
    assert!(!root.join("files").exists());
    assert!(!sandbox.home.join(".config").exists());
    assert!(!sandbox.home.join(".local").exists());
}

#[test]
fn blocked_preview_uses_exit_four_with_complete_json_on_stdout() {
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest(
        "schema=1\n[[files]]\nid='zshrc'\nsource='missing'\ntarget='~/.zshrc'\nmode='link'\n",
    );
    let output = sandbox
        .command()
        .arg("--repo")
        .arg(manifest.parent().unwrap())
        .args(["--json", "files", "plan", "apply"])
        .assert()
        .code(4)
        .get_output()
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["files"][0]["outcome"], "blocked");
    assert!(json["files"][0]["effects"].as_array().unwrap().is_empty());
    assert!(output.stderr.is_empty());
    sandbox.assert_readonly();
}

#[test]
fn inventory_apply_noop_and_both_unmanage_modes_are_readonly() {
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest(
        "schema=1\n[[files]]\nid='zshrc'\nsource='zshrc'\ntarget='~/.zshrc'\nmode='link'\n",
    );
    let root = manifest.parent().unwrap();
    fs::write(root.join("zshrc"), "safe").unwrap();
    let target = sandbox.home.join(".zshrc");
    std::os::unix::fs::symlink(root.join("zshrc"), &target).unwrap();
    for arguments in [
        vec!["files", "list"],
        vec!["files", "plan", "apply"],
        vec!["files", "plan", "unmanage", "zshrc", "--mode", "keep"],
        vec!["files", "plan", "unmanage", "zshrc", "--mode", "remove"],
    ] {
        let output = sandbox
            .command()
            .arg("--repo")
            .arg(root)
            .arg("--json")
            .args(&arguments)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        if arguments.last() == Some(&"apply") {
            assert_eq!(json["files"][0]["outcome"], "no_op");
        }
        if arguments.last() == Some(&"remove") {
            assert_eq!(json["files"][0]["requires_removal_acknowledgement"], true);
        }
    }
    assert!(target.is_symlink());
    assert_eq!(fs::read_to_string(root.join("zshrc")).unwrap(), "safe");
    assert!(!sandbox.home.join(".config").exists());
    assert!(!sandbox.home.join(".local").exists());
}

#[test]
fn plan_profile_selection_and_readonly_help_are_explicit() {
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest("schema=1\n[profiles]\navailable=['dev']\n");
    sandbox
        .command()
        .arg("--repo")
        .arg(manifest.parent().unwrap())
        .args(["files", "plan", "apply", "--profile", "dev"])
        .assert()
        .success();
    sandbox
        .command()
        .args(["files", "plan", "apply"])
        .assert()
        .code(2);
    // No approval flags or actual mutation commands are exposed in this slice.
    sandbox.command().args(["files", "apply"]).assert().code(2);
    sandbox
        .command()
        .env_remove("HOME")
        .args(["files", "plan", "--help"])
        .assert()
        .success();
    sandbox.assert_readonly();
}

#[test]
fn malformed_local_configuration_does_not_leak_variable_values() {
    let sandbox = Sandbox::new();
    let config = sandbox.home.join(".config/dort");
    fs::create_dir_all(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
    let local = config.join("local.toml");
    fs::write(&local, "schema = 'SENSITIVE-SENTINEL'").unwrap();
    fs::set_permissions(&local, fs::Permissions::from_mode(0o600)).unwrap();
    let output = sandbox
        .command()
        .args(["--json", "doctor"])
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    assert!(
        !String::from_utf8(output)
            .unwrap()
            .contains("SENSITIVE-SENTINEL")
    );
    assert!(!sandbox.home.join(".local").exists());
}
