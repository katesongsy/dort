use assert_cmd::Command;
use std::fs;

#[test]
fn nonterminal_tui_entrypoints_fail_without_state_or_prompts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    fs::create_dir(&home).unwrap();
    for args in [
        vec![],
        vec!["tui"],
        vec!["--json"],
        vec!["--json", "tui"],
        vec!["--yes", "tui"],
        vec!["--dry-run", "tui"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dort"))
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/no-external-programs")
            .args(&args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("TUI requires terminal"));
        assert!(!stderr.contains("Type yes"));
        if args.contains(&"--json") {
            serde_json::from_str::<serde_json::Value>(&stderr).unwrap();
        }
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
    }
}
