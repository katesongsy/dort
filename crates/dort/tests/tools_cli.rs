use assert_cmd::Command;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    home: PathBuf,
    executable: PathBuf,
}
impl Fixture {
    fn new(response: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let home = base.join("home");
        fs::create_dir(&home).unwrap();
        let executable = base.join("provider");
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf ran >> \"$HOME/calls\"\n{response}\n"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            base,
            home,
            executable,
        }
    }
    fn command(&self, provider: &str) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/no-external-programs")
            .current_dir(&self.base)
            .args(["tools", "inspect", provider, "--executable"])
            .arg(&self.executable)
            .arg("--search-dir")
            .arg(self.base.join("trusted-bin"));
        c
    }
    fn untouched(&self) {
        assert_eq!(fs::read_dir(&self.home).unwrap().count(), 0);
    }
}
#[test]
fn inspection_preview_is_process_free_even_with_yes_and_is_independent_of_registration() {
    let f = Fixture::new("exit 99");
    let out = f
        .command("brew")
        .args(["--dry-run", "--yes", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        value["provider"]["arguments"],
        serde_json::json!(["info", "--json=v2", "--installed", "--formula"])
    );
    assert!(
        value["provider"]["warning"]
            .as_str()
            .unwrap()
            .contains("plugins")
    );
    f.untouched();
    fs::create_dir_all(f.home.join(".config/dort")).unwrap();
    fs::write(
        f.home.join(".config/dort/config.toml"),
        "PRIVATE invalid configuration",
    )
    .unwrap();
    f.command("brew").arg("--dry-run").assert().success();
    assert!(!f.home.join("calls").exists());
}
#[test]
fn nonterminal_and_json_queries_require_explicit_approval() {
    for args in [vec![], vec!["--json"]] {
        let f = Fixture::new("exit 99");
        let out = f
            .command("mise")
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        assert!(
            String::from_utf8(out.stderr)
                .unwrap()
                .contains("requires --yes")
        );
        f.untouched();
    }
}
#[test]
fn explicit_approval_runs_each_fixed_provider_once_and_preserves_metadata_only() {
    for (provider, response, expected) in [
        (
            "brew",
            r#"printf '{"formulae":[{"full_name":"ripgrep","installed":[{"version":"14.1.0"}],"desc":"PRIVATE"}]}'"#,
            "ripgrep",
        ),
        (
            "apt",
            "printf 'libc6:arm64\\t2:2.39-1~ubuntu1\\tinstalled\\tok\\n'",
            "libc6:arm64",
        ),
        (
            "mise",
            r#"printf '{"node":[{"version":"22.0.0","installed":true,"source":{"path":"PRIVATE"}}]}'"#,
            "node",
        ),
    ] {
        let f = Fixture::new(response);
        let out = f
            .command(provider)
            .args(["--yes", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["provider"]["observation"]["state"], "complete");
        assert_eq!(
            value["provider"]["observation"]["installations"][0]["package"],
            expected
        );
        assert!(!String::from_utf8(out).unwrap().contains("PRIVATE"));
        assert_eq!(fs::read_to_string(f.home.join("calls")).unwrap(), "ran");
        assert!(!f.home.join(".config").exists());
        assert!(!f.home.join(".local").exists());
    }
}
#[test]
fn npm_requires_explicit_declared_runtime_node_cli_and_prefix() {
    let f = Fixture::new(
        r#"[ "$1" = "$HOME/npm-cli.js" ] || exit 91
printf '{"dependencies":{"@scope/pkg":{"version":"1.2.3","resolved":"PRIVATE"}}}'"#,
    );
    let manifest = f.base.join("dort.toml");
    fs::write(&manifest, include_str!("../examples/dort.toml")).unwrap();
    let cli = f.home.join("npm-cli.js");
    let prefix = f.base.join("global-prefix");
    let out = f
        .command("npm")
        .args(["--yes", "--json", "--manifest"])
        .arg(&manifest)
        .args(["--runtime-tool", "node", "--npm-cli"])
        .arg(&cli)
        .arg("--prefix")
        .arg(&prefix)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        value["provider"]["description"]["npm_binding"]["runtime_tool"],
        "node"
    );
    assert_eq!(value["manifest"], manifest.to_str().unwrap());
    assert!(!String::from_utf8(out).unwrap().contains("PRIVATE"));
    assert!(!prefix.exists());
    let f = Fixture::new("exit 99");
    f.command("npm").arg("--yes").assert().code(2);
    f.untouched();
}
#[test]
fn failure_is_unavailable_not_empty_and_does_not_retain_or_print_raw_output() {
    for response in [
        "printf PRIVATE; printf PRIVATE >&2; exit 7",
        "printf PRIVATE",
    ] {
        let f = Fixture::new(response);
        let out = f
            .command("brew")
            .args(["--yes", "--json"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        assert!(
            !String::from_utf8(out.stdout.clone())
                .unwrap()
                .contains("PRIVATE")
        );
        assert!(!String::from_utf8(out.stderr).unwrap().contains("PRIVATE"));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["provider"]["observation"]["state"], "unavailable");
    }
}
#[test]
fn invalid_manifest_and_irrelevant_binding_flags_fail_without_execution_or_content() {
    let f = Fixture::new("exit 99");
    let manifest = f.base.join("dort.toml");
    fs::write(&manifest, "PASSWORD = 'PRIVATE'\nBAD invalid").unwrap();
    let out = f
        .command("npm")
        .arg("--manifest")
        .arg(&manifest)
        .args(["--yes", "--json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(!String::from_utf8(out.stderr).unwrap().contains("PRIVATE"));
    f.command("brew")
        .args(["--runtime-tool", "node", "--yes"])
        .assert()
        .code(2);
    f.untouched();
}
