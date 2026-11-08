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
    let path = sandbox.manifest(include_str!("../examples/dort.toml"));
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
        include_str!("../examples/dort.toml")
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
fn discovery_honors_persistent_ignores_without_writes_and_rejects_malformed_preferences() {
    let sandbox = Sandbox::new();
    fs::create_dir_all(sandbox.home.join(".tool/sub")).unwrap();
    fs::write(sandbox.home.join(".tool/sub/config"), "ordinary fixture").unwrap();
    fs::write(sandbox.home.join(".tool-other"), "other").unwrap();
    let config = sandbox.home.join(".config/dort");
    fs::create_dir_all(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
    let path = config.join("discovery-ignore.toml");
    fs::write(&path, "paths = ['.tool']\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let output = sandbox
        .command()
        .args(["--json", "files", "discover"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let candidates = report["candidates"].as_array().unwrap();
    assert!(
        candidates
            .iter()
            .all(|c| !c["entry"]["path"].as_str().unwrap().contains(".tool/"))
    );
    assert!(candidates.iter().any(|c| {
        c["entry"]["path"]
            .as_str()
            .unwrap()
            .ends_with(".tool-other")
    }));
    assert!(!sandbox.home.join(".local").exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), "paths = ['.tool']\n");
    fs::write(&path, "paths = ['../outside']\n").unwrap();
    sandbox
        .command()
        .args(["--json", "files", "discover"])
        .assert()
        .failure();
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
fn recovery_diagnosis_is_read_only_and_never_exposes_replay_commands() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .args(["--json", "recovery", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["read_only"], true);
    assert_eq!(report["write_blocked"], false);
    assert_eq!(report["transactions"].as_array().unwrap().len(), 0);
    sandbox
        .command()
        .args(["recovery", "rollback"])
        .assert()
        .code(2);
    sandbox.assert_readonly();
}

#[test]
fn malformed_transaction_diagnosis_retains_intent_and_redacts_record_contents() {
    let sandbox = Sandbox::new();
    let state = sandbox.home.join(".local/state/dort");
    let tx = state
        .join("repositories")
        .join("a".repeat(64))
        .join("transactions")
        .join("b".repeat(64));
    fs::create_dir_all(&tx).unwrap();
    for directory in tx.ancestors().take_while(|p| p.starts_with(&state)) {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let checkpoint = tx.join("000000.json");
    fs::write(&checkpoint, b"MALFORMED-SENSITIVE-SENTINEL").unwrap();
    fs::set_permissions(&checkpoint, fs::Permissions::from_mode(0o600)).unwrap();
    let output = sandbox
        .command()
        .args(["--json", "recovery", "list"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["write_blocked"], true);
    assert_eq!(report["transactions"][0]["valid_checkpoints"], 0);
    assert!(
        !String::from_utf8(output)
            .unwrap()
            .contains("MALFORMED-SENSITIVE-SENTINEL")
    );
    assert_eq!(
        fs::read(checkpoint).unwrap(),
        b"MALFORMED-SENSITIVE-SENTINEL"
    );
    assert!(!state.join("lock").exists());
}

#[test]
fn schema2_history_is_reported_without_locking_replaying_or_creating_checkpoints() {
    use dort_core::{
        context::HostContext,
        lifecycle,
        local::{RepositoryRegistry, repository_state_path},
        planner::{self, Request},
        platform::{Architecture, Platform},
        state::WriterLock,
    };
    let sandbox = Sandbox::new();
    let manifest = sandbox.manifest(
        "schema=1\n[[files]]\nid='editor'\nsource='editor'\ntarget='~/.editor'\nmode='link'\n",
    );
    let repo = manifest.parent().unwrap();
    fs::create_dir(repo.join(".git")).unwrap();
    fs::write(repo.join("editor"), "SCHEMA2-CONTENT-SENTINEL").unwrap();
    let host = HostContext::new(
        sandbox.home.clone(),
        None,
        None,
        Platform::Ubuntu,
        Architecture::X86_64,
    )
    .unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let id = RepositoryRegistry::register(&host, &lock, repo).unwrap();
    let approval = lifecycle::Approval::from_plan(planner::plan(
        planner::observe(&host, repo, &[], Request::Apply).unwrap(),
    ))
    .unwrap();
    assert!(
        lifecycle::execute(&host, &lock, approval, |event| {
            if event.sequence == 2 {
                Err(dort_core::Error::Invalid("injected stop".into()))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let history = lifecycle::history(&host).unwrap();
    let tx = repository_state_path(&host, &id)
        .unwrap()
        .join("transactions")
        .join(&history[0].transaction_id);
    let before = fs::read_dir(&tx).unwrap().count();
    // Deliberately retain the writer lock: read-only diagnosis must still work.
    let output = sandbox
        .command()
        .args(["--json", "recovery", "list"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["transactions"][0]["journal_schema"], 2);
    assert_eq!(report["write_blocked"], true);
    assert!(
        !String::from_utf8(output)
            .unwrap()
            .contains("SCHEMA2-CONTENT-SENTINEL")
    );
    sandbox
        .command()
        .args(["recovery", "list"])
        .assert()
        .code(4);
    assert!(!sandbox.home.join(".editor").exists());
    assert_eq!(fs::read_dir(tx).unwrap().count(), before);
    assert_eq!(
        fs::read_to_string(manifest).unwrap(),
        "schema=1\n[[files]]\nid='editor'\nsource='editor'\ntarget='~/.editor'\nmode='link'\n"
    );
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
