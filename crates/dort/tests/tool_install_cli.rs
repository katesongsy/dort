use assert_cmd::Command;
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
    bin: PathBuf,
    executable: PathBuf,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("lookup");
        for p in [&home, &repo, &bin] {
            fs::create_dir(p).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='mise',package='demo',version='1.2.3'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n").unwrap();
        let host = HostContext::new(
            home,
            None,
            None,
            Platform::detect().unwrap(),
            Architecture::detect().unwrap(),
        )
        .unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        drop(lock);
        let executable = root.join("mise");
        let behavior = match mode {
            "fail" => "exit 9",
            "collision" => {
                "for d in \"$HOME\"/.local/state/dort/tools-attempts/*; do printf malformed > \"$d/outcome.json\"; done; exit 9"
            }
            _ => "printf installed > \"$HOME/installed\"",
        };
        fs::write(&executable, format!("#!/bin/sh\nif [ \"$1\" = ls ]; then\n printf Q >> \"$HOME/calls\"\n if [ -f \"$HOME/installed\" ]; then printf '{{\"demo\":[{{\"version\":\"1.2.3\",\"installed\":true}}]}}'; else printf '{{}}'; fi\n exit 0\nfi\nprintf I >> \"$HOME/calls\"\nprintf '%s\\n' \"$@\" > \"$HOME/argv\"\nprintf PRIVATE_OUTPUT; printf PRIVATE_ERROR >&2\n{behavior}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
            bin,
            executable,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.host.home)
            .env("PATH", "/no-programs")
            .current_dir(&self.repo);
        c
    }
    fn install(&self) -> Command {
        let scope = serde_json::json!({"id":"chosen","provider":"mise","executable":self.executable,"search_dirs":[self.bin]}).to_string();
        let mut c = self.cmd();
        c.args(["--json", "tools", "reconcile", "--manifest"])
            .arg(self.repo.join("dort.toml"))
            .arg("--tool-search-dir")
            .arg(&self.bin)
            .args([
                "--scope",
                &scope,
                "--bind",
                "demo=chosen",
                "--install-tool",
                "demo",
            ]);
        c
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
    fn attempts(&self) -> serde_json::Value {
        serde_json::from_slice(
            &self
                .cmd()
                .args(["--json", "tools", "attempts"])
                .assert()
                .success()
                .get_output()
                .stdout,
        )
        .unwrap()
    }
}
#[test]
fn dry_run_and_nonterminal_refusal_never_query_and_exact_install_has_private_attempt() {
    let f = Fixture::new("ok");
    let output = f
        .install()
        .args(["--dry-run", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("requested_install")
    );
    assert_eq!(f.calls(), "");
    assert!(!f.host.state_dir.join("tools-attempts").exists());
    f.install().assert().code(2);
    assert_eq!(f.calls(), "");
    let out = f
        .install()
        .arg("--yes")
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("PRIVATE"));
    assert!(!String::from_utf8(out.stderr).unwrap().contains("PRIVATE"));
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["outcome"]["package"], "exact_present");
    assert_eq!(report["readiness_verified"], false);
    assert_eq!(f.calls(), "QQIQ"); // Collection, fresh inspection, installer, post-inspection.
    assert_eq!(
        fs::read_to_string(f.host.home.join("argv")).unwrap(),
        "--no-config\n--no-env\n--no-hooks\ninstall\n--jobs=1\ndemo@1.2.3\n"
    );
    let attempts = f.attempts();
    assert_eq!(attempts.as_array().unwrap().len(), 1);
    assert_eq!(attempts[0]["unresolved"], false);
    let id = attempts[0]["id"].as_str().unwrap();
    let record = f
        .cmd()
        .args(["--json", "tools", "attempts", "--id", id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8(record).unwrap().contains("PRIVATE"));
}
#[test]
fn unregistered_install_scope_refuses_before_collecting_any_inventory() {
    let f = Fixture::new("ok");
    fs::remove_file(f.host.local_config_path()).unwrap();
    f.install().arg("--yes").assert().code(2);
    assert_eq!(f.calls(), "");
}

#[test]
fn failed_install_reports_observed_failure_without_claiming_readiness() {
    let f = Fixture::new("fail");
    let output = f
        .install()
        .arg("--yes")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["outcome"]["process"], "failed");
    assert_eq!(report["outcome"]["package"], "absent");
    assert_eq!(report["possible_partial_effects"], true);
    assert_eq!(f.calls(), "QQIQ");
    assert_eq!(f.attempts()[0]["unresolved"], false);
}
#[test]
fn unresolved_attempt_blocks_only_installs_and_closes_only_after_fresh_confirmation() {
    let f = Fixture::new("collision");
    f.install().arg("--yes").assert().failure();
    let id = f.attempts()[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(f.attempts()[0]["unresolved"], true);
    let before = f.calls();
    f.cmd()
        .args(["--json", "--dry-run", "tools", "close-attempt", &id])
        .assert()
        .success();
    assert_eq!(f.calls(), before);
    f.cmd()
        .args(["--json", "tools", "close-attempt", &id])
        .assert()
        .code(2);
    assert_eq!(f.calls(), before);
    f.install().arg("--yes").assert().failure();
    assert_eq!(f.calls(), format!("{before}Q")); // New collection only; no locked query/spawn.
    f.cmd()
        .args(["--json", "--yes", "tools", "close-attempt", &id])
        .assert()
        .success();
    assert_eq!(f.calls(), format!("{before}QQ"));
    assert_eq!(f.attempts()[0]["unresolved"], false);
    assert_eq!(f.attempts()[0]["explicitly_closed"], true);
}
