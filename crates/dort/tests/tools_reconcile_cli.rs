use assert_cmd::Command;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    manifest: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let bin = root.join("tools");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        let manifest = root.join("dort.toml");
        fs::write(&manifest, "schema = 1\n[[tools]]\nid = 'node'\nbinary = 'node'\n[tools.install]\nmacos = { provider = 'mise', package = 'node', version = '22' }\nubuntu = { provider = 'mise', package = 'node', version = '22' }\n[[tools]]\nid = 'rg'\nbinary = 'rg'\n[tools.install]\nmacos = { provider = 'mise', package = 'rg' }\nubuntu = { provider = 'mise', package = 'rg' }\n").unwrap();
        Self {
            _temp: temp,
            root,
            home,
            bin,
            manifest,
        }
    }
    fn scope(&self, id: &str, body: &str) -> String {
        let executable = self.root.join(format!("provider-{id}"));
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '{id}\\n' >> \"$HOME/calls\"\n{body}\n"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        serde_json::json!({"id":id,"provider":"mise","executable":executable,"search_dirs":[self.root.join("provider-search")]}).to_string()
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/no-external-programs")
            .current_dir(&self.root)
            .args(["tools", "reconcile", "--manifest"])
            .arg(&self.manifest)
            .arg("--tool-search-dir")
            .arg(&self.bin);
        c
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.join("calls")).unwrap_or_default()
    }
}
const NODE: &str =
    r#"printf '{"node":[{"version":"22.1.0","installed":true,"source":{"path":"PRIVATE"}}]}'"#;
const RG: &str = r#"printf '{"rg":[{"version":"14.1.0","installed":true}]}'"#;
#[test]
fn multiple_explicit_scopes_are_bound_without_merging_and_candidates_are_never_executed() {
    let f = Fixture::new();
    let node = f.scope("one", NODE);
    let rg = f.scope("two", RG);
    fs::write(
        f.bin.join("node"),
        format!(
            "#!/bin/sh\nprintf PRIVATE > '{}'\n",
            f.home.join("candidate-executed").display()
        ),
    )
    .unwrap();
    fs::set_permissions(f.bin.join("node"), fs::Permissions::from_mode(0o700)).unwrap();
    let out = f
        .command()
        .arg("--scope")
        .arg(&node)
        .arg("--scope")
        .arg(&rg)
        .args(["--bind", "node=one", "--bind", "rg=two", "--yes", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["snapshots"].as_array().unwrap().len(), 2);
    let rows = v["reconciliation"]["plan"]["rows"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["action"] == "no_change"));
    assert_eq!(v["reconciliation"]["bindings"]["node"], "one");
    assert_eq!(f.calls(), "one\ntwo\n");
    assert!(!f.home.join("candidate-executed").exists());
    assert!(!String::from_utf8(out).unwrap().contains("PRIVATE"));
    assert!(!f.home.join(".config").exists() && !f.home.join(".local").exists());
    // Deliberately choosing a different same-provider scope never searches the other one.
    let out = f
        .command()
        .arg("--scope")
        .arg(&node)
        .arg("--scope")
        .arg(&rg)
        .args(["--bind", "node=two", "--bind", "rg=one", "--yes", "--json"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["reconciliation"]["plan"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["package"]["state"] == "absent")
    );
}
#[test]
fn dry_run_and_invalid_bindings_never_query_or_probe_binary_contents() {
    let f = Fixture::new();
    let scope = f.scope("chosen", "exit 99");
    let out = f
        .command()
        .arg("--scope")
        .arg(&scope)
        .args(["--bind", "node=chosen", "--dry-run", "--yes", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["inspections"][0]["review"]["provider"]["warning"]
            .as_str()
            .unwrap()
            .contains("plugins")
    );
    assert!(f.calls().is_empty());
    f.command()
        .arg("--scope")
        .arg(&scope)
        .args(["--bind", "unknown=chosen", "--yes"])
        .assert()
        .code(2);
    f.command()
        .arg("--scope")
        .arg(&scope)
        .arg("--scope")
        .arg(&scope)
        .arg("--yes")
        .assert()
        .code(2);
    f.command()
        .args([
            "--scope",
            "{\"PRIVATE\":true,\"args\":[\"install\"]}",
            "--yes",
            "--json",
        ])
        .assert()
        .code(2);
    assert!(f.calls().is_empty());
}
#[test]
fn unbound_tools_remain_unobserved_even_with_a_unique_matching_provider_snapshot() {
    let f = Fixture::new();
    let scope = f.scope("only", NODE);
    let out = f
        .command()
        .arg("--scope")
        .arg(scope)
        .args(["--yes", "--json"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["reconciliation"]["plan"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["package"]["state"] == "not_observed")
    );
}
#[test]
fn unavailable_provider_or_directory_is_not_a_healthy_empty_reconciliation() {
    let f = Fixture::new();
    let scope = f.scope("failed", "printf PRIVATE; printf PRIVATE >&2; exit 7");
    let out = f
        .command()
        .arg("--scope")
        .arg(scope)
        .args(["--bind", "node=failed", "--yes", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["snapshots"][0]["report"]["provider"]["observation"]["state"],
        "unavailable"
    );
    assert!(!String::from_utf8(out).unwrap().contains("PRIVATE"));
    let scope = f.scope("ok", NODE);
    let missing = f.root.join("missing-directory");
    let out = f
        .command()
        .arg("--tool-search-dir")
        .arg(&missing)
        .arg("--scope")
        .arg(scope)
        .args(["--bind", "node=ok", "--yes", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["reconciliation"]["binaries"]["tools"]["node"]["observation"]["state"],
        "unavailable"
    );
}
#[test]
fn manifest_changes_during_inspection_are_refused_without_source_or_automatic_retry() {
    let f = Fixture::new();
    let scope = f.scope(
        "edit",
        &format!(
            "printf 'PRIVATE malformed' > '{}'\n{NODE}",
            f.manifest.display()
        ),
    );
    let out = f
        .command()
        .arg("--scope")
        .arg(scope)
        .args(["--bind", "node=edit", "--yes", "--json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(!String::from_utf8(out.stderr).unwrap().contains("PRIVATE"));
    assert_eq!(f.calls(), "edit\n");
}
#[test]
fn npm_reconciliation_uses_exact_runtime_scope_and_keeps_private_provenance_out_of_json() {
    let f = Fixture::new();
    fs::write(&f.manifest, "schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='22'}\nubuntu={provider='mise',package='node',version='22'}\n[[tools]]\nid='cli'\nbinary='cli'\n[tools.install]\nmacos={provider='npm',package='@scope/cli',runtime_tool='node'}\nubuntu={provider='npm',package='@scope/cli',runtime_tool='node'}\n").unwrap();
    let node_scope = f.scope("mise", NODE);
    let node = f.bin.join("node");
    fs::write(&node, "#!/bin/sh\nprintf npm >> \"$HOME/calls\"\nprintf '{\"dependencies\":{\"@scope/cli\":{\"version\":\"1.0.0\",\"resolved\":\"PRIVATE\"}}}'\n").unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
    let npm = serde_json::json!({"id":"npm","provider":"npm","executable":node,"search_dirs":[f.bin],"runtime_tool":"node","npm_cli":f.root.join("npm-cli.js"),"prefix":f.root.join("prefix")}).to_string();
    let out = f
        .command()
        .arg("--scope")
        .arg(node_scope)
        .arg("--scope")
        .arg(npm)
        .args([
            "--bind",
            "node=mise",
            "--bind",
            "cli=npm",
            "--yes",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["snapshots"][1]["report"]["provider"]["description"]["npm_binding"]["runtime_tool"],
        "node"
    );
    assert!(v["snapshots"][1]["report"].get("source").is_none());
    assert!(!String::from_utf8(out).unwrap().contains("PRIVATE"));
    assert!(!f.root.join("prefix").exists());
}

#[test]
fn missing_package_command_is_metadata_only_and_never_dispatched_even_with_yes() {
    let f = Fixture::new();
    let scope = f.scope(
        "preview",
        "if [ \"$1\" = install ]; then printf ran > \"$HOME/INSTALL-RAN\"; fi\nprintf '{}' ",
    );
    let out = f
        .command()
        .arg("--scope")
        .arg(scope)
        .args(["--bind", "node=preview", "--yes", "--json"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let node = value["reconciliation"]["plan"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "node")
        .unwrap();
    assert_eq!(node["install_preview"]["state"], "preview_only");
    assert_eq!(
        node["install_preview"]["command"]["arguments"],
        serde_json::json!(["install", "--jobs=1", "node@22"])
    );
    assert_eq!(
        node["install_preview"]["command"]["execution_authorized"],
        false
    );
    assert_eq!(
        value["snapshots"][0]["report"]["provider"]["description"]["arguments"],
        serde_json::json!(["ls", "--installed", "--json"])
    );
    assert_eq!(f.calls(), "preview\n");
    assert!(!f.home.join("INSTALL-RAN").exists());
}

#[test]
fn composite_nonterminal_execution_still_requires_explicit_approval() {
    let f = Fixture::new();
    let scope = f.scope("one", NODE);
    f.command()
        .arg("--scope")
        .arg(scope)
        .arg("--json")
        .assert()
        .code(2);
    assert!(f.calls().is_empty());
}
