use assert_cmd::Command;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use tempfile::TempDir;

struct Sandbox {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        Self {
            _temp: temp,
            root,
            home,
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dort"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .current_dir(&self.root)
            .arg("--repo")
            .arg(&self.root);
        command
    }
    fn assert_readonly(&self) {
        assert_eq!(fs::read_dir(&self.home).unwrap().count(), 0);
    }
    fn fake(&self, script: &str) -> PathBuf {
        let bin = self.root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("git");
        fs::write(&fake, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        bin
    }
}

#[test]
fn missing_and_relative_path_entries_fail_without_state() {
    let sandbox = Sandbox::new();
    // Even an executable ./git must not be selected through a relative PATH.
    let bin = sandbox.fake("exit 93");
    fs::copy(bin.join("git"), sandbox.root.join("git")).unwrap();
    for path in ["/no-external-programs", ".", ":relative:"] {
        let output = sandbox
            .command()
            .env("PATH", path)
            .args(["--json", "git", "status"])
            .assert()
            .code(2)
            .get_output()
            .stderr
            .clone();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Git executable not found")
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_dort"))
        .env_clear()
        .env("HOME", &sandbox.home)
        .env("PATH", "/no-external-programs")
        .arg("--repo")
        .arg(sandbox.root.join("missing-\x1b[31m"))
        .args(["git", "status"])
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    assert!(!output.contains(&0x1b));
    assert!(!String::from_utf8(output).unwrap().contains("missing-"));
    sandbox.assert_readonly();
}

#[test]
fn fixed_arguments_clean_environment_summaries_and_explicit_patch() {
    let sandbox = Sandbox::new();
    let bin = sandbox.fake(r#"
[ "${GIT_DIR+x}" != x ] || exit 81
[ "${GIT_INDEX_FILE+x}" != x ] || exit 82
[ "${GIT_CONFIG_PARAMETERS+x}" != x ] || exit 83
[ "${GIT_EXTERNAL_DIFF+x}" != x ] || exit 84
[ "$GIT_OPTIONAL_LOCKS" = 0 ] || exit 85
[ "$GIT_CONFIG_GLOBAL" = /dev/null ] || exit 86
[ "$PATH" = /no-external-programs ] || exit 87
case " $* " in *" --no-optional-locks "*) ;; *) exit 88;; esac
case " $* " in *" core.fsmonitor=false "*) ;; *) exit 89;; esac
for arg do
case "$arg" in
config) exit 1;;
rev-parse) pwd -P; exit 0;;
status) printf '# branch.head main\000? strange\nname\000'; exit 0;;
diff)
case " $* " in
*" --patch "*) printf 'PRIVATE-PATCH\033[31m\n';;
*) printf ':100644 100644 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 M\000strange\nname\000';;
esac
exit 0;;
esac
done
exit 90
"#);
    for (command, patch) in [
        (vec!["git", "status"], false),
        (vec!["git", "diff"], false),
        (vec!["git", "diff", "--staged"], false),
        (vec!["git", "diff", "--patch"], true),
        (vec!["git", "diff", "--staged", "--patch"], true),
    ] {
        for json in [false, true] {
            let mut process = sandbox.command();
            process
                .env("PATH", &bin)
                .env("GIT_DIR", "poison")
                .env("GIT_INDEX_FILE", "poison")
                .env("GIT_CONFIG_PARAMETERS", "poison")
                .env("GIT_EXTERNAL_DIFF", "poison");
            if json {
                process.arg("--json");
            }
            let output = process
                .args(&command)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            assert!(!output.contains(&0x1b));
            let output = String::from_utf8(output).unwrap();
            assert_eq!(output.contains("PRIVATE-PATCH"), patch);
            if json {
                let value: serde_json::Value = serde_json::from_str(&output).unwrap();
                assert_eq!(value["read_only"], true);
                assert_eq!(value["entries"][0]["path"], "strange\nname");
                if patch {
                    assert!(value["patch"].as_str().unwrap().contains("\\x1b"));
                }
            } else {
                assert!(!output.contains("strange\nname"));
            }
        }
    }
    sandbox.assert_readonly();
}

#[test]
fn unicode_controls_are_terminal_safe_and_json_identity_round_trips() {
    let sandbox = Sandbox::new();
    let name = "unicode-\u{009b}\u{202e}😀";
    let bin = sandbox.fake(&format!(
        "for arg do case \"$arg\" in config) exit 1;; rev-parse) pwd -P; exit 0;; status) printf '? {name}\\000'; exit 0;; esac; done"
    ));
    let output = sandbox
        .command()
        .env("PATH", &bin)
        .args(["--json", "git", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(output.is_ascii());
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["entries"][0]["path"], name);
    let output = sandbox
        .command()
        .env("PATH", &bin)
        .args(["git", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains('\u{009b}'));
    assert!(!output.contains('\u{202e}'));
    sandbox.assert_readonly();
}

#[test]
fn command_failure_and_malformed_output_never_echo_stderr_or_partial_patches() {
    let sandbox = Sandbox::new();
    for script in [
        "printf 'PRIVATE-ERROR' >&2; exit 5",
        "for arg do case \"$arg\" in config) exit 1;; rev-parse) pwd -P; exit 0;; diff) printf 'PRIVATE-BODY'; exit 0;; esac; done",
    ] {
        let bin = sandbox.fake(script);
        let output = sandbox
            .command()
            .env("PATH", bin)
            .args(["--json", "git", "diff", "--patch"])
            .assert()
            .code(2)
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        assert!(
            !String::from_utf8(output.stderr)
                .unwrap()
                .contains("PRIVATE")
        );
    }
    sandbox.assert_readonly();
}
