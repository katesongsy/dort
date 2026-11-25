use assert_cmd::Command;
use dort_core::{
    context::HostContext,
    local::RepositoryRegistry,
    platform::{Architecture, Platform},
    state::WriterLock,
};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    host: HostContext,
    repo: PathBuf,
    bin: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("bin");
        for path in [&home, &repo, &bin] {
            fs::create_dir(path).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        let host = HostContext::new(
            home,
            None,
            None,
            Platform::detect().unwrap(),
            Architecture::detect().unwrap(),
        )
        .unwrap();
        let tools = ["a", "b"].map(|id| format!("[[tools]]\nid='{id}'\nbinary='{id}'\n[tools.install]\nprovider='brew'\npackage='{id}'\n")).join("");
        fs::write(repo.join("dort.toml"), format!("schema=1\n{tools}")).unwrap();
        let lock = WriterLock::acquire(&host).unwrap();
        RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        drop(lock);
        let f = Self {
            _temp: temp,
            host,
            repo,
            bin,
        };
        f.program("brew", r#"
case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\n' "$HOME/prefix";;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --installed --formula')
printf Q >> "$HOME/calls"
printf '{"formulae":['
if [ -f "$HOME/a" ]; then printf '{"full_name":"a","installed":[{"version":"1.2.3"}]},'; fi
if [ -f "$HOME/b" ]; then printf '{"full_name":"b","installed":[{"version":"1.2.3"}]},'; fi
printf '{"full_name":"unrelated","installed":[{"version":"1.2.3"}]}]}';;
'info --json=v2 --formula homebrew/core/'*)
for arg in "$@"; do :; done
name=${arg##*/}
printf '{"formulae":[{"name":"%s","full_name":"%s","tap":"homebrew/core","disabled":false,"versions":{"stable":"1.2.3"},"installed":[' "$name" "$name"
if [ -f "$HOME/$name" ]; then printf '{"version":"1.2.3"}'; fi
printf ']}]}';;
'install --formula --no-ask --skip-post-install homebrew/core/a') printf A >> "$HOME/calls"; if [ -f "$HOME/fail-a" ]; then exit 9; fi; printf installed > "$HOME/a";;
'install --formula --no-ask --skip-post-install homebrew/core/b') printf B >> "$HOME/calls"; printf installed > "$HOME/b";;
*) exit 90;;
esac
"#);
        f
    }
    fn program(&self, name: &str, body: &str) {
        let path = if name == "brew" {
            let prefix = self.host.home.join("prefix");
            for dir in [
                "Homebrew/bin",
                "Homebrew/Library",
                "Cellar",
                "bin",
                "etc",
                "include",
                "lib",
                "opt",
                "sbin",
                "share",
                "var/homebrew",
            ] {
                fs::create_dir_all(prefix.join(dir)).unwrap();
            }
            let path = prefix.join("Homebrew/bin/brew");
            if !self.bin.join(name).exists() {
                std::os::unix::fs::symlink(&path, self.bin.join(name)).unwrap();
            }
            path
        } else {
            self.bin.join(name)
        };
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn cmd(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dort"));
        command
            .env_clear()
            .env("HOME", &self.host.home)
            .env("PATH", &self.bin)
            .current_dir(&self.repo);
        command
    }
    fn json(&self, args: &[&str], exit: i32) -> Value {
        let output = self
            .cmd()
            .args(args)
            .assert()
            .code(exit)
            .get_output()
            .stdout
            .clone();
        assert!(!String::from_utf8_lossy(&output).contains("PRIVATE_PROVIDER_OUTPUT"));
        serde_json::from_slice(&output).unwrap()
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
}

#[test]
fn installed_discovery_is_reviewed_process_free_until_approval_and_has_no_snapshot_interface() {
    let f = Fixture::new();
    let preview = f.json(&["--json", "--dry-run", "tools", "inspect"], 0);
    assert_eq!(
        preview["inspection_review"]["inspections"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(f.calls().is_empty());
    f.cmd().args(["tools", "inspect"]).assert().code(2);
    assert!(f.calls().is_empty());
    let current = f.json(&["--json", "--yes", "tools", "inspect"], 0);
    assert_eq!(current["reports"].as_array().unwrap().len(), 1);
    assert_eq!(
        current["reports"][0]["provider"]["observation"]["installations"][0]["package"],
        "unrelated"
    );
    assert!(current.get("snapshots").is_none() && current.get("bindings").is_none());
    assert_eq!(f.calls(), "Q");
    f.cmd()
        .args(["tools", "inspect", "--bind", "tool=s1"])
        .assert()
        .code(2);
    assert_eq!(f.calls(), "Q");
}
#[test]
fn human_tools_reports_use_names_and_actions_instead_of_json_or_internal_bindings() {
    let f = Fixture::new();
    for args in [
        vec!["--dry-run", "tools", "check"],
        vec!["--dry-run", "tools", "inspect"],
    ] {
        let out = f
            .cmd()
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("brew") && text.contains("Program:"));
        assert!(
            !text.contains("check-brew")
                && !text.contains("\"bindings\"")
                && !text.contains("\"schema\"")
        );
        assert!(serde_json::from_str::<Value>(&text).is_err());
    }
    assert!(f.calls().is_empty());
    let out = f
        .cmd()
        .args(["--yes", "tools", "install-missing"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("a: Missing")
            && text.contains("2 ready to install")
            && text.contains("Installation results")
    );
    assert!(
        !text.contains("PRIVATE_PROVIDER_OUTPUT")
            && !text.contains("\"snapshots\"")
            && !text.contains("\"statuses\"")
    );
}
#[test]
fn malformed_configuration_and_manifest_errors_withhold_file_contents() {
    let f = Fixture::new();
    fs::write(
        f.repo.join("dort.toml"),
        "schema=1\nPRIVATE_MANIFEST_TOKEN invalid",
    )
    .unwrap();
    let output = f
        .cmd()
        .args(["--json", "--yes", "tools", "check"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_MANIFEST_TOKEN"));
    fs::write(f.host.local_config_path(), "PRIVATE_CONFIG_TOKEN invalid").unwrap();
    let output = f
        .cmd()
        .args(["--json", "--yes", "tools", "check"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONFIG_TOKEN"));
    assert!(f.calls().is_empty());
}

#[test]
fn normal_commands_require_approval_and_dry_run_never_queries_or_installs() {
    let f = Fixture::new();
    let manifest = fs::read(f.repo.join("dort.toml")).unwrap();
    for action in ["check", "install-missing"] {
        f.cmd().args(["--json", "tools", action]).assert().code(2);
        let review = f.json(&["--json", "--dry-run", "--yes", "tools", action], 0);
        assert_eq!(review["preview_only"], true);
        assert_eq!(
            review["check_review"]["inspections"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(f.calls().is_empty());
        assert!(!f.host.home.join("a").exists() && !f.host.home.join("b").exists());
    }
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), manifest);
    assert!(!f.host.state_dir.join("tools-attempts").exists());
}

#[test]
fn check_uses_only_declared_providers_and_returns_per_tool_status() {
    let f = Fixture::new();
    f.program("npm", "printf forbidden > \"$HOME/npm-ran\"; exit 99");
    let report = f.json(&["--json", "--yes", "tools", "check"], 4);
    assert_eq!(f.calls(), "Q");
    assert!(!f.host.home.join("npm-ran").exists());
    let rows = report["statuses"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["status"] == "missing"));
    assert_eq!(report["check"]["snapshots"].as_array().unwrap().len(), 1);
    assert!(!f.host.state_dir.join("tools-attempts").exists());
}

#[test]
fn one_combined_approval_installs_missing_serially_then_check_reports_installed() {
    let f = Fixture::new();
    let report = f.json(&["--json", "--yes", "tools", "install-missing"], 0);
    assert_eq!(
        report["installation"]["results"].as_array().unwrap().len(),
        2
    );
    assert!(
        report["installation"]["not_started"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let calls = f.calls();
    assert!(calls.find('A').unwrap() < calls.find('B').unwrap());
    assert_eq!(calls.matches('A').count(), 1);
    assert_eq!(calls.matches('B').count(), 1);
    let report = f.json(&["--json", "--yes", "tools", "check"], 0);
    assert!(
        report["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "installed")
    );
    let report = f.json(&["--json", "--yes", "tools", "install-missing"], 0);
    assert!(report["installation"].is_null());
    assert_eq!(f.calls().matches('A').count(), 1);
    assert_eq!(f.calls().matches('B').count(), 1);
}

#[test]
fn failed_first_install_stops_and_reports_unstarted_tools_without_private_output() {
    let f = Fixture::new();
    fs::write(f.host.home.join("fail-a"), "fixture").unwrap();
    let report = f.json(&["--json", "--yes", "tools", "install-missing"], 1);
    assert_eq!(
        report["installation"]["results"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        report["installation"]["not_started"],
        serde_json::json!(["b"])
    );
    assert_eq!(f.calls().matches('A').count(), 1);
    assert!(!f.calls().contains('B'));
    assert!(!f.host.home.join("a").exists() && !f.host.home.join("b").exists());
}

#[test]
fn unavailable_is_not_missing_and_current_claims_never_trigger_upgrades() {
    let f = Fixture::new();
    fs::write(f.host.home.join("a"), "fixture").unwrap();
    let report = f.json(&["--json", "--yes", "tools", "install-missing"], 0);
    assert_eq!(
        report["before_installation"]["statuses"][0]["status"],
        "installed"
    );
    assert!(!f.calls().contains('A') && f.calls().contains('B'));
    f.program("brew", "printf PRIVATE_PROVIDER_OUTPUT; exit 9");
    let report = f.json(&["--json", "--yes", "tools", "check"], 1);
    assert!(
        report["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "could_not_check")
    );
}

#[test]
fn environment_plan_stops_on_uncertain_result_and_resumes_without_repeating_installs() {
    let f = Fixture::new();
    f.program("brew", include_str!("support/environment-brew.sh"));
    fs::write(
        f.repo.join("dort.toml"),
        include_str!("support/environment.toml"),
    )
    .unwrap();
    fs::write(f.repo.join("settings"), "editor settings\n").unwrap();
    let editor = f.host.home.join(".editor");
    fs::write(f.host.home.join("fail-b"), "").unwrap();
    let unknown = f.json(&["--json", "environment", "plan"], 4);
    assert_eq!(unknown["rows"][0]["status"], "Unknown");
    f.json(&["--json", "--dry-run", "environment", "check"], 0);
    assert_eq!(f.calls(), "");
    f.json(&["--json", "--yes", "environment", "check"], 0);
    assert_eq!(f.calls(), "Q");
    let review = f.json(&["--json", "--dry-run", "environment", "apply"], 0);
    let ids: Vec<_> = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["item"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["a", "b", "editor"]);
    assert_eq!(f.calls(), "Q");
    let run = f.json(&["--json", "--yes", "environment", "apply"], 4);
    assert_eq!(run["steps"][0]["progress"]["state"], "confirmed", "{run:#}");
    assert_eq!(run["steps"][1]["progress"]["state"], "uncertain", "{run:#}");
    assert_eq!(run["steps"][2]["progress"]["state"], "pending");
    assert!(!editor.exists());
    assert!(f.host.home.join("prefix/Cellar/b/1.2.3/bin/b").exists());
    let calls = f.calls();
    f.json(&["--json", "--yes", "environment", "resume"], 4);
    assert_eq!(f.calls(), calls);
    f.json(&["--json", "--yes", "environment", "check"], 0);
    let run = f.json(&["--json", "--yes", "environment", "resume"], 0);
    assert!(
        run["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["progress"]["state"] == "confirmed")
    );
    assert_eq!(fs::read_link(&editor).unwrap(), f.repo.join("settings"));
    assert_eq!(
        f.calls().chars().filter(|c| *c != 'Q').collect::<String>(),
        "AB"
    );
}

fn interrupted_environment() -> Fixture {
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    f.program("brew", include_str!("support/environment-brew.sh"));
    fs::write(
        f.repo.join("dort.toml"),
        include_str!("support/environment.toml"),
    )
    .unwrap();
    fs::write(f.repo.join("settings"), "editor settings\n").unwrap();
    fs::write(f.host.home.join("hold-b"), "").unwrap();
    f.json(&["--json", "--yes", "environment", "check"], 0);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_dort"))
        .env_clear()
        .env("HOME", &f.host.home)
        .env("PATH", &f.bin)
        .current_dir(&f.repo)
        .args(["--json", "--yes", "environment", "apply"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let wait = |file: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !f.host.home.join(file).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        f.host.home.join(file).exists()
    };
    let reached = wait("provider-ready");
    let _ = child.kill();
    child.wait().unwrap();
    fs::remove_file(f.host.home.join("hold-b")).unwrap();
    assert!(reached && wait("provider-done"));
    f
}

#[test]
fn environment_process_interruption_leaves_durable_intent_and_never_replays() {
    let f = interrupted_environment();
    let review = f.json(&["--json", "--dry-run", "environment", "resume"], 4);
    assert_eq!(
        review["last_run"]["steps"][0]["progress"]["state"],
        "confirmed"
    );
    assert_eq!(
        review["last_run"]["steps"][1]["progress"]["state"],
        "in_progress"
    );
    assert_eq!(
        review["last_run"]["steps"][2]["progress"]["state"],
        "pending"
    );
    assert!(!f.host.home.join(".editor").exists());
    f.json(&["--json", "--yes", "environment", "check"], 0);
    f.json(&["--json", "--yes", "environment", "resume"], 0);
    assert_eq!(
        fs::read_link(f.host.home.join(".editor")).unwrap(),
        f.repo.join("settings")
    );
    assert_eq!(
        f.calls().chars().filter(|c| *c != 'Q').collect::<String>(),
        "AB"
    );
}

#[test]
fn environment_abandon_archives_and_requires_fresh_check_and_install_close() {
    let f = interrupted_environment();
    let editor = f.host.home.join(".editor");
    let before = f.calls();
    // Simulate an interrupted install that left no usable command or package claim.
    fs::remove_file(f.host.home.join("b")).unwrap();
    fs::remove_file(f.bin.join("b")).unwrap();
    let blocked = f.json(&["--json", "environment", "plan"], 4);
    assert!(blocked["blockers"].to_string().contains("unfinished run"));
    let review = f.json(&["--json", "--dry-run", "environment", "abandon"], 0);
    let archive = Path::new(review["archive"].as_str().unwrap());
    assert!(!archive.exists());
    assert_eq!(f.calls(), before);
    let abandoned = f.json(&["--json", "--yes", "environment", "abandon"], 0);
    assert_eq!(abandoned["abandoned"], true);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(archive).unwrap()).unwrap(),
        review["run"]
    );
    assert_eq!(f.calls(), before);
    assert!(f.bin.join("a").exists());
    assert!(!editor.exists());
    let blocked = f.json(&["--json", "environment", "plan"], 4);
    assert!(
        blocked["blockers"]
            .to_string()
            .contains("fresh Environment check")
    );
    let resume = f.json(&["--json", "--dry-run", "environment", "resume"], 4);
    assert!(resume["blockers"].to_string().contains("run was abandoned"));
    f.json(&["--json", "--yes", "environment", "check"], 0);
    let blocked = f.json(&["--json", "environment", "plan"], 4);
    assert!(
        blocked["blockers"]
            .to_string()
            .contains("install is unresolved")
    );
    let attempts = f.json(&["--json", "tools", "attempts"], 0);
    let attempt = attempts
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["unresolved"] == true)
        .unwrap();
    f.json(
        &[
            "--json",
            "--yes",
            "tools",
            "close-attempt",
            attempt["id"].as_str().unwrap(),
        ],
        0,
    );
    f.json(&["--json", "environment", "plan"], 0);
    assert_eq!(
        f.calls().chars().filter(|c| *c != 'Q').collect::<String>(),
        "AB"
    );
    assert!(!editor.exists());
    f.json(&["--json", "--yes", "environment", "apply"], 0);
    assert_eq!(
        f.calls().chars().filter(|c| *c != 'Q').collect::<String>(),
        "ABB"
    );
    assert_eq!(fs::read_link(&editor).unwrap(), f.repo.join("settings"));
    assert!(archive.exists());
}

#[test]
fn environment_links_multiple_files_in_the_same_new_directory() {
    let f = Fixture::new();
    fs::write(
        f.repo.join("dort.toml"),
        "schema = 1\n[[files]]\nid = 'first'\nsource = 'a'\ntarget = '~/.settings/a'\nmode = 'link'\n\n[[files]]\nid = 'second'\nsource = 'b'\ntarget = '~/.settings/b'\nmode = 'link'\n\n[[files]]\nid = 'third'\nsource = 'c'\ntarget = '~/.settings/c'\nmode = 'link'\n",
    )
    .unwrap();
    for name in ["a", "b", "c"] {
        fs::write(f.repo.join(name), name).unwrap();
    }
    f.json(&["--json", "--yes", "environment", "apply"], 0);
    for name in ["a", "b", "c"] {
        assert_eq!(
            fs::read_link(f.host.home.join(".settings").join(name)).unwrap(),
            f.repo.join(name)
        );
    }
}

#[test]
fn environment_review_blocks_inactive_dependency_without_enabling_it() {
    let f = Fixture::new();
    f.program("brew", include_str!("support/environment-brew.sh"));
    fs::write(
        f.repo.join("dort.toml"),
        include_str!("support/environment.toml"),
    )
    .unwrap();
    fs::write(f.repo.join("settings"), "editor settings\n").unwrap();
    f.cmd()
        .args(["--yes", "profiles", "set", "work", "off"])
        .assert()
        .success();
    f.json(&["--json", "--yes", "environment", "check"], 0);
    let review = f.json(&["--json", "--yes", "environment", "apply"], 4);
    assert_eq!(review["rows"][0]["item"]["id"], "b");
    assert_eq!(
        review["rows"][0]["blockers"][0],
        "Required tool a is off. Enable one of: work."
    );
    assert_eq!(review["rows"][1]["status"], "Blocked");
    assert_eq!(f.calls(), "Q");
    assert!(!f.host.home.join(".editor").exists());
}
