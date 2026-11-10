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
    node: PathBuf,
    cli: PathBuf,
    mise: PathBuf,
    prefix: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("lookup");
        for p in [&home, &repo, &bin] {
            fs::create_dir(p).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='20.1.0'}\nubuntu={provider='mise',package='node',version='20.1.0'}\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='npm',package='demo',version='1.2.3',runtime_tool='node'}\nubuntu={provider='npm',package='demo',version='1.2.3',runtime_tool='node'}\n").unwrap();
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
        let node = bin.join("node");
        let cli = root.join("npm-cli.js");
        let mise = root.join("mise");
        let prefix = host.home.join("prefix");
        fs::write(&node, r##"#!/bin/sh
shift
if [ "$1" = ls ]; then
 printf N >> "$HOME/calls"
 if [ -f "$HOME/installed" ]; then printf '{"dependencies":{"demo":{"version":"1.2.3"}}}'; else printf '{"dependencies":{}}'; fi
 exit 0
fi
printf I >> "$HOME/calls"
printf '%s\n' "$@" > "$HOME/argv"
printf PRIVATE_NPM_OUTPUT; printf PRIVATE_NPM_ERROR >&2
if [ -f "$HOME/collision" ]; then
 for d in "$HOME"/.local/state/dort/tools-attempts/*; do printf bad > "$d/outcome.json"; done
 exit 9
fi
printf installed > "$HOME/installed"
"##).unwrap();
        fs::write(&mise, "#!/bin/sh\nprintf M >> \"$HOME/calls\"\nprintf '{\"node\":[{\"version\":\"20.1.0\",\"installed\":true}]}'\n").unwrap();
        fs::write(&cli, "TRUSTED_SCRIPT_NOT_LAUNCHED_DIRECTLY").unwrap();
        for p in [&node, &mise] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            _temp: temp,
            host,
            repo,
            bin,
            node,
            cli,
            mise,
            prefix,
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
        let runtime = serde_json::json!({"id":"runtime","provider":"mise","executable":self.mise,"search_dirs":[self.bin]}).to_string();
        let npm = serde_json::json!({"id":"npm","provider":"npm","executable":self.node,"search_dirs":[self.bin],"runtime_tool":"node","npm_cli":self.cli,"prefix":self.prefix}).to_string();
        let mut c = self.cmd();
        c.args(["--json", "tools", "reconcile", "--manifest"])
            .arg(self.repo.join("dort.toml"))
            .arg("--tool-search-dir")
            .arg(&self.bin)
            .args([
                "--scope",
                &runtime,
                "--scope",
                &npm,
                "--bind",
                "node=runtime",
                "--bind",
                "demo=npm",
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
fn npm_dry_run_is_process_free_and_exact_install_uses_independent_runtime_catalog() {
    let f = Fixture::new();
    f.install().args(["--dry-run", "--yes"]).assert().success();
    assert_eq!(f.calls(), "");
    assert!(!f.prefix.exists());
    assert!(!f.host.state_dir.join("tools-attempts").exists());
    f.install().assert().code(2);
    assert_eq!(f.calls(), "");
    let output = f
        .install()
        .arg("--yes")
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    let r: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(r["outcome"]["package"], "exact_present");
    assert_eq!(r["scope"]["npm"]["binding"]["runtime_tool"], "node");
    assert!(
        !text.contains("PRIVATE_NPM")
            && !String::from_utf8(output.stderr)
                .unwrap()
                .contains("PRIVATE_NPM")
    );
    assert_eq!(f.calls(), "MNNM INM".replace(' ', "")); // Collection M,N; fresh catalogs N,M; install; post N,M.
    let argv = fs::read_to_string(f.host.home.join("argv")).unwrap();
    for arg in [
        "--global",
        "--ignore-scripts",
        "--registry=https://registry.npmjs.org/",
        "--userconfig=/dev/null",
        "--globalconfig=/dev/null",
        "demo@1.2.3",
    ] {
        assert!(argv.lines().any(|a| a == arg));
    }
    assert!(!f.attempts()[0]["unresolved"].as_bool().unwrap());
}
#[test]
fn npm_attempts_have_known_and_damaged_scope_closure_without_current_manifest() {
    let f = Fixture::new();
    fs::write(f.host.home.join("collision"), "fixture").unwrap();
    f.install().arg("--yes").assert().code(2);
    let a = f.attempts();
    let id = a[0]["id"].as_str().unwrap();
    assert_eq!(a[0]["unresolved"], true);
    fs::write(f.repo.join("dort.toml"), "INVALID_CURRENT_MANIFEST").unwrap();
    let calls = f.calls();
    f.cmd()
        .args(["--json", "--dry-run", "tools", "close-attempt", id])
        .assert()
        .success();
    assert_eq!(calls, f.calls());
    f.cmd()
        .args(["--json", "--yes", "tools", "close-attempt", id])
        .assert()
        .success();
    assert_eq!(f.attempts()[0]["explicitly_closed"], true);
    fs::write(
        f.host
            .state_dir
            .join("tools-attempts")
            .join(id)
            .join("intent.json"),
        "PRIVATE_DAMAGE",
    )
    .unwrap();
    f.cmd()
        .args(["--json", "--yes", "tools", "close-attempt", id])
        .assert()
        .code(2);
    f.cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "close-attempt",
            id,
            "--provider",
            "npm",
            "--executable",
        ])
        .arg(&f.node)
        .arg("--search-dir")
        .arg(&f.bin)
        .arg("--npm-cli")
        .arg(&f.cli)
        .arg("--prefix")
        .arg(&f.prefix)
        .assert()
        .success();
    assert_eq!(f.attempts()[0]["unresolved"], false);
}
