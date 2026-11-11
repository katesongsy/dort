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
    brew: PathBuf,
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
        fs::write(repo.join("dort.toml"),"schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='brew',package='demo'}\n").unwrap();
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
        for d in [
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
            fs::create_dir_all(host.home.join("prefix").join(d)).unwrap();
        }
        let brew = host.home.join("prefix/Homebrew/bin/brew");
        fs::write(&brew,r##"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\n' "$HOME/prefix";;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --formula homebrew/core/demo') printf '{"formulae":[{"name":"demo","full_name":"demo","tap":"homebrew/core","disabled":false,"versions":{"stable":"2.7"},"installed":['; if [ -f "$HOME/installed" ]; then printf '{"version":"2.7"}'; fi; printf ']}]}';;
'info --json=v2 --installed --formula') if [ -f "$HOME/installed" ]; then printf '{"formulae":[{"full_name":"demo","installed":[{"version":"2.7"}]}]}'; else printf '{"formulae":[]}'; fi;;
'install --formula --no-ask --skip-post-install homebrew/core/demo') printf PRIVATE_OUTPUT; printf PRIVATE_ERROR >&2; printf installed > "$HOME/installed";;
*) exit 99;;
esac
"##).unwrap();
        fs::set_permissions(&brew, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            host,
            repo,
            bin,
            brew,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.host.home)
            .env("PATH", "/no-tools")
            .current_dir(&self.repo);
        c
    }
    fn reconcile(&self) -> Command {
        let mut c = self.cmd();
        let scope = serde_json::json!({"id":"brew","provider":"brew","executable":self.brew,"search_dirs":[self.bin]});
        c.args([
            "--json",
            "tools",
            "reconcile",
            "--install-tool",
            "demo",
            "--manifest",
        ])
        .arg(self.repo.join("dort.toml"))
        .arg("--tool-search-dir")
        .arg(&self.bin)
        .arg("--scope")
        .arg(scope.to_string())
        .args(["--bind", "demo=brew"]);
        c
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
}
#[test]
fn brew_cli_dry_run_does_not_execute_provider_or_create_attempts() {
    let f = Fixture::new();
    let out = f.reconcile().arg("--dry-run").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(if f.host.platform == Platform::Macos {
            0
        } else {
            2
        })
    );
    assert!(f.calls().is_empty());
    assert!(!f.host.state_dir.join("tools-attempts").exists());
}
#[test]
fn brew_cli_native_install_uses_current_outcome_and_explicit_expected_prefix() {
    if Platform::detect().unwrap() != Platform::Macos {
        println!(
            "SKIP: Homebrew declarations are macOS-only; core/TUI use explicit macOS fake-provider contexts"
        );
        return;
    }
    let f = Fixture::new();
    let out = f
        .reconcile()
        .args(["--yes", "--brew-prefix"])
        .arg(f.host.home.join("prefix"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("provider_current_present") && text.contains("2.7"));
    assert!(!text.contains("exact_present") && !text.contains("PRIVATE"));
    assert_eq!(f.calls().matches("install --formula").count(), 1);
    let out = f
        .cmd()
        .args(["--json", "tools", "attempts"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("provider_current_present")
    );
}
#[test]
fn damaged_brew_closure_is_process_free_until_one_approval_without_current_manifest() {
    let f = Fixture::new();
    let id = "a".repeat(64);
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let dir = root.join(&id);
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_file(f.repo.join("dort.toml")).unwrap();
    let command = || {
        let mut c = f.cmd();
        c.args([
            "--json",
            "tools",
            "close-attempt",
            &id,
            "--provider",
            "brew",
            "--executable",
        ])
        .arg(&f.brew)
        .arg("--search-dir")
        .arg(&f.bin)
        .arg("--prefix")
        .arg(f.host.home.join("prefix"));
        c
    };
    command().arg("--dry-run").assert().success();
    assert!(f.calls().is_empty());
    command().assert().code(2);
    assert!(f.calls().is_empty());
    command().arg("--yes").assert().success();
    assert_eq!(
        f.calls(),
        "--prefix\ninfo --json=v2 --installed --formula\n"
    );
    assert!(!f.calls().contains("install --formula"));
}
