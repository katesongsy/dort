use assert_cmd::Command;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    bin: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        let f = Self {
            _temp: temp,
            home,
            bin,
        };
        f.program("npm-launcher", "exec node \"$@\"");
        symlink("npm-launcher", f.bin.join("npm")).unwrap();
        f.program(
            "node",
            r#"[ "$PWD" = / ] || exit 81
[ "${NODE_OPTIONS-unset}" = unset ] || exit 82
if [ "$1" = prefix ]; then
printf P >> "$HOME/calls"
IFS= read -r prefix < "$HOME/npm-global-location"
printf '%s\n' "$prefix"
else
printf L >> "$HOME/calls"
for arg do :; done
[ "$arg" = "$HOME/configured global packages" ] || exit 83
printf '{"dependencies":{"formatter":{"version":"1.2.3","PRIVATE_PACKAGE_FIELD":"withheld"}}}'
fi"#,
        );
        fs::write(
            f.home.join("npm-global-location"),
            format!("{}\n", f.home.join("configured global packages").display()),
        )
        .unwrap();
        f
    }
    fn program(&self, name: &str, body: &str) {
        let path = self.bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.home)
            .env("PATH", &self.bin)
            .env("NODE_OPTIONS", "PRIVATE_PARENT_OPTIONS")
            .current_dir(self._temp.path());
        c
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.home.join("calls")).unwrap_or_default()
    }
}
#[test]
fn inspect_npm_from_path_needs_no_repository_and_reviews_both_queries_before_execution() {
    let f = Fixture::new();
    let out = f
        .cmd()
        .args(["--json", "--dry-run", "--yes", "tools", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let p: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let queries = p["inspection_review"]["inspections"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["provider"]["query"], "npm_globals");
    assert_eq!(queries[0]["prefix_probe"]["arguments"][0], "prefix");
    assert_eq!(queries[0]["provider"]["arguments"][0], "ls");
    assert!(queries[0]["provider"]["npm_binding"].is_null());
    assert!(f.calls().is_empty());
    f.cmd().args(["tools", "inspect"]).assert().code(2);
    assert!(
        f.calls().is_empty(),
        "A nonterminal query still needs approval"
    );
    let out = f
        .cmd()
        .args(["--json", "--yes", "tools", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let p: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let current = &p["reports"][0]["provider"];
    assert_eq!(
        current["observation"]["installations"][0]["package"],
        "formatter"
    );
    assert!(current["description"]["npm_binding"].is_null());
    assert_eq!(
        current["description"]["npm_global_prefix"],
        f.home.join("configured global packages").to_str().unwrap()
    );
    assert_eq!(f.calls(), "PL");
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("PRIVATE_") && !text.contains("runtime_hint"));
    assert!(!f.home.join(".local/state/dort").exists());
}
#[test]
fn human_npm_inspection_is_readable_and_an_invalid_prefix_stays_unavailable() {
    let f = Fixture::new();
    let out = f
        .cmd()
        .args(["--dry-run", "tools", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("Global location query")
            && text.contains("prefix --global")
            && text.contains("ls --global")
    );
    assert!(!text.contains("declaring one Node") && f.calls().is_empty());
    let out = f
        .cmd()
        .args(["--yes", "tools", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("formatter")
            && text.contains("Global prefix:")
            && !text.trim_start().starts_with('{')
    );
    f.program(
        "node",
        "printf P >> \"$HOME/calls\"; printf 'PRIVATE_PREFIX\\n/other\\n'",
    );
    let out = f
        .cmd()
        .args(["--yes", "tools", "inspect"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("unavailable") && !text.contains("PRIVATE_PREFIX"));
    assert_eq!(f.calls(), "PLP");
}
