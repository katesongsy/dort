#[path = "support/tool_outcomes.rs"]
mod outcomes;
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
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nprovider='npm'\npackage='demo'\nversion='1.2.3'\nruntime_tool='node'\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n").unwrap();
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
        let executable = root.join("runtime/bin/node");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        let cli = root.join("runtime/lib/node_modules/npm/bin/npm-cli.js");
        fs::create_dir_all(cli.parent().unwrap()).unwrap();
        fs::write(cli, "PRIVATE_CLI").unwrap();
        let brew = bin.join("brew");
        fs::write(&brew, "#!/bin/sh\nprintf '%s' '{\"formulae\":[{\"full_name\":\"node\",\"installed\":[{\"version\":\"24.0.0\"}]}]}'\n").unwrap();
        fs::set_permissions(brew, fs::Permissions::from_mode(0o700)).unwrap();
        let behavior = match mode {
            "fail" => "exit 9",
            "present-fail" => "printf installed > \"$HOME/installed\"; exit 9",
            "absent" => "exit 0",
            "unavailable" => "printf flag > \"$HOME/unavailable\"; exit 0",
            "conflict" => "printf flag > \"$HOME/conflict\"; exit 0",
            "collision" => {
                "for d in \"$HOME\"/.local/state/dort/tools-attempts/*; do printf malformed > \"$d/outcome.json\"; done; exit 9"
            }
            _ => "printf installed > \"$HOME/installed\"",
        };
        fs::write(&executable, format!(r##"#!/bin/sh
shift
if [ "$1" = ls ]; then
 printf Q >> "$HOME/calls"
 if [ -f "$HOME/unavailable" ]; then printf PRIVATE_QUERY; exit 4; fi
 if [ -f "$HOME/conflict" ]; then printf '{{"dependencies":{{"demo":{{"version":"2.0.0"}}}}}}'; exit 0; fi
 if [ -f "$HOME/installed" ]; then printf '{{"dependencies":{{"demo":{{"version":"1.2.3"}}}}}}'; else printf '{{}}'; fi
 exit 0
fi
printf I >> "$HOME/calls"
printf '%s\n' "$@" > "$HOME/argv"
printf PRIVATE_OUTPUT; printf PRIVATE_ERROR >&2
{behavior}
"##)).unwrap();
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
            .env(
                "PATH",
                std::env::join_paths([self.executable.parent().unwrap(), self.bin.as_path()])
                    .unwrap(),
            )
            .current_dir(&self.repo);
        c
    }
    fn install(&self) -> Command {
        let mut c = self.cmd();
        c.args([
            "--json",
            "tools",
            "install-missing",
            "--npm-prefix",
            &format!("demo={}", self.host.home.join("globals").display()),
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
            .contains("install_missing")
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
    let output: serde_json::Value = serde_json::from_str(&text).unwrap();
    let report = &output["installation"]["results"][0]["report"];
    assert_eq!(report["outcome"]["package"], "exact_present");
    assert_eq!(report["readiness_verified"], false);
    assert_eq!(report["summary"]["status"], "exact_reported");
    assert_eq!(report["summary"]["binary"], "missing");
    assert_eq!(f.calls(), "QQIQ"); // Collection, fresh inspection, installer, post-inspection.
    assert_eq!(
        fs::read_to_string(f.host.home.join("argv")).unwrap(),
        format!(
            "install\n--global\n--prefix\n{}\n--ignore-scripts\n--audit=false\n--fund=false\n--update-notifier=false\n--logs-max=0\n--loglevel=silent\n--registry=https://registry.npmjs.org/\n--userconfig=/dev/null\n--globalconfig=/dev/null\ndemo@1.2.3\n",
            f.host.home.join("globals").display()
        )
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
    let output: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let report = &output["installation"]["results"][0]["report"];
    assert_eq!(report["outcome"]["process"], "failed");
    assert_eq!(report["outcome"]["package"], "absent");
    assert_eq!(report["possible_partial_effects"], true);
    assert_eq!(f.calls(), "QQIQ");
    assert_eq!(f.attempts()[0]["unresolved"], false);
    assert_eq!(f.attempts()[0]["summary"]["status"], "process_failed");
}
#[test]
fn unresolved_attempt_blocks_only_installs_and_closes_only_after_fresh_confirmation() {
    let f = Fixture::new("collision");
    f.install().arg("--yes").assert().failure();
    let id = f.attempts()[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(f.attempts()[0]["unresolved"], true);
    assert_eq!(f.attempts()[0]["summary"]["status"], "unresolved");
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
    assert_eq!(f.attempts()[0]["summary"]["status"], "explicitly_closed");
}
