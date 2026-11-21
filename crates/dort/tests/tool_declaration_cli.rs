use assert_cmd::Command;
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{fs, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
}
const INPUT: &str =
    r#"{"id":"demo","binary":"demo","install":{"provider":"brew","package":"demo"}}"#;
impl Fixture {
    fn new(register: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "# retain\nschema=1\n").unwrap();
        let host = HostContext::new(
            home,
            None,
            None,
            Platform::detect().unwrap(),
            Architecture::detect().unwrap(),
        )
        .unwrap();
        if register {
            let lock = WriterLock::acquire(&host).unwrap();
            RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        }
        Self {
            _temp: temp,
            host,
            repo,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.host.home)
            .env("PATH", "/no-programs")
            .current_dir(self.repo.parent().unwrap());
        c
    }
    fn history(&self) -> serde_json::Value {
        serde_json::from_slice(
            &self
                .cmd()
                .args(["--json", "history"])
                .assert()
                .success()
                .get_output()
                .stdout,
        )
        .unwrap()
    }
}
#[test]
fn registered_declaration_preview_write_noop_and_guarded_schema5_recovery() {
    let f = Fixture::new(true);
    let initial = f.history();
    let config = fs::read(f.host.local_config_path()).unwrap();
    for flags in [
        vec!["--json", "--dry-run"],
        vec!["--json", "--dry-run", "--yes"],
    ] {
        let output = f
            .cmd()
            .args(flags)
            .args(["tools", "declare", "--declaration", INPUT])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let p: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(p["no_op"], false);
        assert_eq!(f.history(), initial);
        assert_eq!(
            fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
            "# retain\nschema=1\n"
        );
    }
    f.cmd()
        .args(["--json", "tools", "declare", "--declaration", INPUT])
        .assert()
        .code(2);
    assert_eq!(f.history(), initial);
    let output = f
        .cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "declare",
            "--declaration",
            INPUT,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let receipt: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(receipt["operation"], "declare-tool");
    let history = f.history();
    let row = history["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["journal_schema"] == 5)
        .unwrap();
    assert_eq!(row["phase"], "completed");
    let repo_id = row["repository_id"].as_str().unwrap();
    let tx = row["transaction_id"].as_str().unwrap();
    let output = f
        .cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "declare",
            "--declaration",
            INPUT,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap()["transaction_id"].is_null()
    );
    assert_eq!(f.history(), history);
    f.cmd()
        .args(["--json", "--yes", "recovery", "rollback", repo_id, tx])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
        "# retain\nschema=1\n"
    );
    assert_eq!(fs::read(f.host.local_config_path()).unwrap(), config);
}
#[test]
fn unregistered_conflicting_and_unknown_fields_are_refused_without_provider_execution() {
    let f = Fixture::new(false);
    f.cmd()
        .arg("--repo")
        .arg(&f.repo)
        .args([
            "--json",
            "--dry-run",
            "tools",
            "declare",
            "--declaration",
            INPUT,
        ])
        .assert()
        .code(2);
    assert!(!f.host.state_dir.exists());
    assert!(!f.host.config_dir.exists());
    let f = Fixture::new(true);
    let initial = f.history();
    let output = f
        .cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "declare",
            "--declaration",
            r#"{"id":"PRIVATE_INPUT","unknown":"WITHHELD"}"#,
        ])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!error.contains("PRIVATE_INPUT") && !error.contains("WITHHELD"));
    assert_eq!(f.history(), initial);
    f.cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "declare",
            "--declaration",
            INPUT,
        ])
        .assert()
        .success();
    let history = f.history();
    let conflict = INPUT.replace("\"binary\":\"demo\"", "\"binary\":\"other\"");
    f.cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "declare",
            "--declaration",
            &conflict,
        ])
        .assert()
        .code(2);
    assert_eq!(f.history(), history);
}

#[test]
fn edit_remove_preview_approval_noop_reference_guards_and_schema6_recovery() {
    let f = Fixture::new(true);
    f.cmd()
        .args(["--yes", "tools", "declare", "--declaration", INPUT])
        .assert()
        .success();
    let edited = INPUT.replace("\"binary\":\"demo\"", "\"binary\":\"new-demo\"");
    let initial = fs::read(f.repo.join("dort.toml")).unwrap();
    for (command, arg) in [("edit", edited.as_str()), ("remove", "demo")] {
        let mut c = f.cmd();
        c.args(["--json", "--dry-run", "tools", command]);
        if command == "edit" {
            c.arg("--declaration");
        }
        let out = c.arg(arg).assert().success().get_output().stdout.clone();
        let p: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(p["change"], command);
        assert_eq!(p["before"]["id"], "demo");
        let mut c = f.cmd();
        c.args(["tools", command]);
        if command == "edit" {
            c.arg("--declaration");
        }
        c.arg(arg).assert().code(2); // nonterminal needs --yes
        assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), initial);
    }
    f.cmd()
        .args(["--yes", "tools", "edit", "--declaration", &edited])
        .assert()
        .success();
    let n = f.history()["transactions"].as_array().unwrap().len();
    f.cmd()
        .args(["--yes", "tools", "edit", "--declaration", &edited])
        .assert()
        .success();
    assert_eq!(f.history()["transactions"].as_array().unwrap().len(), n);
    let edited_bytes = fs::read(f.repo.join("dort.toml")).unwrap();
    f.cmd()
        .args(["--yes", "tools", "remove", "demo"])
        .assert()
        .success();
    assert!(
        !fs::read_to_string(f.repo.join("dort.toml"))
            .unwrap()
            .contains("new-demo")
    );
    let history = f.history();
    let tx = history["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| {
            d["journal_schema"] == 6
                && d["transaction_id"].as_str().is_some_and(|id| {
                    let path = dort_core::local::repository_state_path(
                        &f.host,
                        d["repository_id"].as_str().unwrap(),
                    )
                    .unwrap()
                    .join("transactions")
                    .join(id)
                    .join("manifest.backup");
                    fs::read(path).unwrap() == edited_bytes
                })
        })
        .unwrap();
    f.cmd()
        .args([
            "--yes",
            "recovery",
            "rollback",
            tx["repository_id"].as_str().unwrap(),
            tx["transaction_id"].as_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), edited_bytes);
}
