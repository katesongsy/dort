use assert_cmd::Command;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};
use tempfile::TempDir;
struct Sandbox {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::write(repo.join("dort.toml"), "schema = 1\n").unwrap();
        fs::write(bin.join("git"),"#!/bin/sh\nfor arg do\ncase \"$arg\" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf '# branch.head main\\000';exit 0;;\nesac\ndone\nexit 94\n").unwrap();
        fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            root,
            home,
            repo,
            bin,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("PATH", &self.bin)
            .current_dir(&self.root);
        c
    }
    fn repo(&self) -> Command {
        let mut c = self.cmd();
        c.arg("--repo").arg(&self.repo);
        c
    }
    fn init(&self) {
        self.cmd()
            .args(["--json", "--yes", "init"])
            .arg(&self.repo)
            .assert()
            .success();
    }
    fn assert_no_state(&self) {
        assert!(!self.home.join(".config").exists());
        assert!(!self.home.join(".local").exists());
    }
    fn add(&self) {
        fs::write(self.home.join(".a"), "ordinary settings\n").unwrap();
        self.repo()
            .args([
                "--json", "--yes", "files", "add", "~/.a", "--id", "a", "--source", "files/a",
            ])
            .assert()
            .success();
    }
    fn history(&self) -> serde_json::Value {
        let o = self
            .cmd()
            .args(["--json", "history"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&o).unwrap()
    }
}
#[test]
fn dry_run_and_noninteractive_missing_approval_never_create_state() {
    let s = Sandbox::new();
    s.cmd()
        .args(["--json", "--dry-run", "init"])
        .arg(&s.repo)
        .assert()
        .success();
    s.assert_no_state();
    s.cmd()
        .args(["--json", "init"])
        .arg(&s.repo)
        .assert()
        .code(2);
    s.assert_no_state();
    s.cmd().arg("init").arg(&s.repo).assert().code(2);
    s.assert_no_state();
    // Missing destinations can be previewed without invoking Git init.
    s.cmd()
        .args(["--json", "--dry-run", "--yes", "init"])
        .arg(s.root.join("new-repo"))
        .assert()
        .success();
    assert!(!s.root.join("new-repo").exists());
    s.assert_no_state();
    fs::write(s.home.join(".a"), "ordinary settings\n").unwrap();
    for flags in [vec!["--json", "--dry-run"], vec!["--dry-run", "--yes"]] {
        s.repo()
            .args(flags)
            .args(["files", "add", "~/.a", "--id", "a", "--source", "files/a"])
            .assert()
            .success();
        s.assert_no_state();
    }
    s.repo()
        .args([
            "--json", "files", "add", "~/.a", "--id", "a", "--source", "files/a",
        ])
        .assert()
        .code(2);
    s.assert_no_state();
    assert!(!s.repo.join("files").exists());
}
#[test]
fn attach_adopt_status_apply_keep_and_guarded_rollback_work_end_to_end() {
    let s = Sandbox::new();
    s.init();
    let manifest_inode = fs::metadata(s.repo.join("dort.toml")).unwrap().ino();
    s.add();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        fs::read(s.repo.join("files/a")).unwrap(),
        b"ordinary settings\n"
    );
    assert_ne!(
        manifest_inode,
        fs::metadata(s.repo.join("dort.toml")).unwrap().ino()
    );
    s.cmd().args(["--json", "status"]).assert().success();
    s.cmd().args(["--json", "plan"]).assert().success();
    let count = s.history()["transactions"].as_array().unwrap().len();
    let output = s
        .repo()
        .args(["--json", "--yes", "files", "apply"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap()["transaction_id"].is_null()
    );
    assert_eq!(s.history()["transactions"].as_array().unwrap().len(), count);
    s.repo()
        .args([
            "--json",
            "--dry-run",
            "files",
            "remove",
            "a",
            "--mode",
            "keep",
        ])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    s.repo()
        .args(["--json", "--yes", "files", "remove", "a", "--mode", "keep"])
        .assert()
        .success();
    assert!(fs::symlink_metadata(s.home.join(".a")).unwrap().is_file());
    let history = s.history();
    let tx = history["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["journal_schema"] == 2)
        .unwrap();
    let rid = tx["repository_id"].as_str().unwrap();
    let tid = tx["transaction_id"].as_str().unwrap();
    let before = fs::read_to_string(s.repo.join("dort.toml")).unwrap();
    s.cmd()
        .args(["--json", "--dry-run", "rollback", rid, tid])
        .assert()
        .success();
    assert_eq!(
        before,
        fs::read_to_string(s.repo.join("dort.toml")).unwrap()
    );
    s.cmd()
        .args(["--json", "rollback", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--yes", "rollback", rid, tid])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    assert!(s.repo.join("files/a").exists());
}
#[test]
fn remove_needs_separate_exact_ack_and_retains_source() {
    let s = Sandbox::new();
    s.init();
    s.add();
    for ack in [None, Some("other")] {
        let mut c = s.repo();
        c.args(["--json", "--yes"]);
        if let Some(ack) = ack {
            c.args(["--confirm-remove", ack]);
        }
        c.args(["files", "remove", "a", "--mode", "remove"])
            .assert()
            .code(2);
        assert!(
            fs::symlink_metadata(s.home.join(".a"))
                .unwrap()
                .is_symlink()
        );
    }
    s.repo()
        .args([
            "--json",
            "--yes",
            "--confirm-remove",
            "a",
            "files",
            "remove",
            "a",
            "--mode",
            "remove",
        ])
        .assert()
        .success();
    assert!(!s.home.join(".a").exists());
    assert_eq!(
        fs::read(s.repo.join("files/a")).unwrap(),
        b"ordinary settings\n"
    );
}
#[test]
fn failed_bootstrap_blocks_writes_until_explicit_abandon_and_dry_runs_stay_readonly() {
    let s = Sandbox::new();
    let missing = s.root.join("new");
    // Fake Git deliberately fails its init operation, leaving only private preparation.
    s.cmd()
        .args(["--json", "--yes", "init"])
        .arg(&missing)
        .assert()
        .code(2);
    assert!(!missing.exists());
    let h = s
        .cmd()
        .args(["--json", "history"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let h: serde_json::Value = serde_json::from_slice(&h).unwrap();
    let d = &h["transactions"][0];
    assert_eq!(d["journal_schema"], 3);
    let rid = d["repository_id"].as_str().unwrap();
    let tid = d["transaction_id"].as_str().unwrap();
    s.cmd()
        .args(["--json", "--yes", "init"])
        .arg(&s.repo)
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--dry-run", "recovery", "resume", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--dry-run", "recovery", "abandon", rid, tid])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().code(4);
    s.cmd()
        .args(["--json", "recovery", "abandon", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--yes", "recovery", "abandon", rid, tid])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().success();
    assert!(!missing.exists());
    s.init();
}

#[test]
fn blocked_plans_and_lock_contention_do_not_execute() {
    let s = Sandbox::new();
    s.init();
    s.repo()
        .args([
            "--json",
            "--yes",
            "files",
            "add",
            "~/.missing",
            "--id",
            "a",
            "--source",
            "files/a",
        ])
        .assert()
        .code(4);
    assert!(!s.repo.join("files").exists());
    let host = dort_core::context::HostContext::new(
        s.home.clone(),
        None,
        None,
        dort_core::platform::Platform::Ubuntu,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let _lock = dort_core::state::WriterLock::acquire(&host).unwrap();
    s.repo()
        .args(["--json", "--yes", "files", "apply"])
        .assert()
        .code(3);
    s.repo()
        .args(["--json", "--dry-run", "files", "apply"])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().success();
}
