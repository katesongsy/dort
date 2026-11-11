//! CLI fixtures execute only disposable fake programs, never workstation APT/sudo.
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
    programs: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("lookup");
        let programs = root.join("programs");
        for p in [&home, &repo, &bin, &programs] {
            fs::create_dir(p).unwrap();
        }
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join("dort.toml"),"schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nubuntu={provider='apt',package='demo'}\n").unwrap();
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
        for (name, body) in [
            (
                "dpkg-query",
                "printf D >> \"$HOME/calls\"; if [ -f \"$HOME/installed\" ]; then printf 'demo\\t2.7\\tinstalled\\tok\\n'; fi",
            ),
            (
                "apt-cache",
                "printf C >> \"$HOME/calls\"; printf 'demo:\\n  Installed: (none)\\n  Candidate: 2.7\\n'",
            ),
            (
                "sudo",
                "printf S >> \"$HOME/calls\"; if [ -f \"$HOME/denied\" ]; then printf PRIVATE_POLICY >&2; exit 1; fi; shift 6; \"$@\"",
            ),
            (
                "apt-get",
                "printf I >> \"$HOME/calls\"; printf PRIVATE_OUTPUT; printf installed > \"$HOME/installed\"",
            ),
        ] {
            let p = programs.join(name);
            fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            _temp: temp,
            host,
            repo,
            bin,
            programs,
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
        let scope = serde_json::json!({"id":"apt","provider":"apt","executable":self.programs.join("dpkg-query"),"search_dirs":[self.bin]});
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
        .args(["--bind", "demo=apt"]);
        c
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.host.home.join("calls")).unwrap_or_default()
    }
}
#[test]
fn apt_cli_dry_run_and_nonterminal_no_approval_are_process_free() {
    let f = Fixture::new();
    let out = f.reconcile().arg("--dry-run").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(if f.host.platform == Platform::Ubuntu {
            0
        } else {
            2
        })
    );
    assert!(f.calls().is_empty());
    assert!(!f.host.state_dir.join("tools-attempts").exists());
    let out = f.reconcile().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(f.calls().is_empty());
}
#[test]
fn apt_cli_install_and_noninteractive_policy_failure_have_honest_typed_results() {
    if Platform::detect().unwrap() != Platform::Ubuntu {
        println!(
            "SKIP: APT execution is Ubuntu-only; portable core/TUI use explicit Ubuntu fake contexts"
        );
        return;
    }
    for denied in [false, true] {
        let f = Fixture::new();
        if denied {
            fs::write(f.host.home.join("denied"), "flag").unwrap();
        }
        let out = f
            .reconcile()
            .arg("--yes")
            .arg("--apt-cache")
            .arg(f.programs.join("apt-cache"))
            .arg("--apt-get")
            .arg(f.programs.join("apt-get"))
            .arg("--sudo")
            .arg(f.programs.join("sudo"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(if denied { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("PRIVATE") && !text.contains("exact_present"));
        assert!(text.contains(if denied {
            "possible_partial_effects\": true"
        } else {
            "provider_current_present"
        }));
        assert_eq!(f.calls().matches('S').count(), 1);
        assert!(
            f.cmd()
                .args(["--json", "tools", "attempts"])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
#[test]
fn apt_damaged_closure_needs_only_explicit_dpkg_scope_not_root_inputs() {
    let f = Fixture::new();
    let root = f.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let id = "c".repeat(64);
    fs::create_dir(root.join(&id)).unwrap();
    fs::set_permissions(root.join(&id), fs::Permissions::from_mode(0o700)).unwrap();
    let out = f
        .cmd()
        .args([
            "--json",
            "--yes",
            "tools",
            "close-attempt",
            &id,
            "--provider",
            "apt",
            "--executable",
        ])
        .arg(f.programs.join("dpkg-query"))
        .arg("--search-dir")
        .arg(&f.bin)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(f.calls(), "D");
}
