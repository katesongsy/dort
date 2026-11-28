use assert_cmd::Command;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};
use tempfile::TempDir;
struct Sandbox {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let repo = root.join("repo");
        let bin = root.join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::write(repo.join("dort.toml"), "schema = 1\n").unwrap();
        fs::write(bin.join("git"),"#!/bin/sh\nfor arg do\ncase \"$arg\" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf '# branch.head main\\000';exit 0;;\nesac\ndone\nexit 94\n").unwrap();
        fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _temp: temp,
            root,
            home,
            repo,
            bin,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_dort"));
        c.env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("PATH", &self.bin)
            .current_dir(&self.root);
        c
    }
    fn repo(&self) -> Command {
        let mut c = self.cmd();
        c.arg("--repo").arg(&self.repo);
        c
    }
    fn init(&self) {
        self.cmd()
            .args(["--json", "--yes", "init"])
            .arg(&self.repo)
            .assert()
            .success();
    }
    fn assert_no_state(&self) {
        assert!(!self.home.join(".config").exists());
        assert!(!self.home.join(".local").exists());
    }
    fn add(&self) {
        fs::write(self.home.join(".a"), "ordinary settings\n").unwrap();
        self.repo()
            .args([
                "--json", "--yes", "files", "add", "~/.a", "--id", "a", "--source", "files/a",
            ])
            .assert()
            .success();
    }
    fn history(&self) -> serde_json::Value {
        let o = self
            .cmd()
            .args(["--json", "history"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&o).unwrap()
    }
}
#[test]
fn dry_run_and_noninteractive_missing_approval_never_create_state() {
    let s = Sandbox::new();
    s.cmd()
        .args(["--json", "--dry-run", "init"])
        .arg(&s.repo)
        .assert()
        .success();
    s.assert_no_state();
    s.cmd()
        .args(["--json", "init"])
        .arg(&s.repo)
        .assert()
        .code(2);
    s.assert_no_state();
    s.cmd().arg("init").arg(&s.repo).assert().code(2);
    s.assert_no_state();
    // Missing destinations can be previewed without invoking Git init.
    s.cmd()
        .args(["--json", "--dry-run", "--yes", "init"])
        .arg(s.root.join("new-repo"))
        .assert()
        .success();
    assert!(!s.root.join("new-repo").exists());
    s.assert_no_state();
    fs::write(s.home.join(".a"), "ordinary settings\n").unwrap();
    for flags in [vec!["--json", "--dry-run"], vec!["--dry-run", "--yes"]] {
        s.repo()
            .args(flags)
            .args(["files", "add", "~/.a", "--id", "a", "--source", "files/a"])
            .assert()
            .success();
        s.assert_no_state();
    }
    s.repo()
        .args([
            "--json", "files", "add", "~/.a", "--id", "a", "--source", "files/a",
        ])
        .assert()
        .code(2);
    s.assert_no_state();
    assert!(!s.repo.join("files").exists());
}
#[test]
fn attach_adopt_status_apply_keep_and_guarded_rollback_work_end_to_end() {
    let s = Sandbox::new();
    s.init();
    let manifest_inode = fs::metadata(s.repo.join("dort.toml")).unwrap().ino();
    s.add();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        fs::read(s.repo.join("files/a")).unwrap(),
        b"ordinary settings\n"
    );
    assert_ne!(
        manifest_inode,
        fs::metadata(s.repo.join("dort.toml")).unwrap().ino()
    );
    s.cmd().args(["--json", "status"]).assert().success();
    s.cmd().args(["--json", "plan"]).assert().success();
    let count = s.history()["transactions"].as_array().unwrap().len();
    let output = s
        .repo()
        .args(["--json", "--yes", "files", "apply"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap()["transaction_id"].is_null()
    );
    assert_eq!(s.history()["transactions"].as_array().unwrap().len(), count);
    s.repo()
        .args([
            "--json",
            "--dry-run",
            "files",
            "remove",
            "a",
            "--mode",
            "keep",
        ])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    s.repo()
        .args(["--json", "--yes", "files", "remove", "a", "--mode", "keep"])
        .assert()
        .success();
    assert!(fs::symlink_metadata(s.home.join(".a")).unwrap().is_file());
    let history = s.history();
    let tx = history["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["journal_schema"] == 2)
        .unwrap();
    let rid = tx["repository_id"].as_str().unwrap();
    let tid = tx["transaction_id"].as_str().unwrap();
    let before = fs::read_to_string(s.repo.join("dort.toml")).unwrap();
    s.cmd()
        .args(["--json", "--dry-run", "rollback", rid, tid])
        .assert()
        .success();
    assert_eq!(
        before,
        fs::read_to_string(s.repo.join("dort.toml")).unwrap()
    );
    s.cmd()
        .args(["--json", "rollback", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--yes", "rollback", rid, tid])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    assert!(s.repo.join("files/a").exists());
}
#[test]
fn remove_binds_selected_id_and_optional_guard_must_match_retaining_source() {
    let s = Sandbox::new();
    s.init();
    s.add();
    s.repo()
        .args([
            "--json",
            "--yes",
            "--confirm-remove",
            "other",
            "files",
            "remove",
            "a",
            "--mode",
            "remove",
        ])
        .assert()
        .code(2);
    assert!(
        fs::symlink_metadata(s.home.join(".a"))
            .unwrap()
            .is_symlink()
    );
    s.repo()
        .args([
            "--json", "--yes", "files", "remove", "a", "--mode", "remove",
        ])
        .assert()
        .success();
    assert!(!s.home.join(".a").exists());
    assert_eq!(
        fs::read(s.repo.join("files/a")).unwrap(),
        b"ordinary settings\n"
    );
}
#[test]
fn failed_bootstrap_blocks_writes_until_explicit_abandon_and_dry_runs_stay_readonly() {
    let s = Sandbox::new();
    let missing = s.root.join("new");
    // Fake Git deliberately fails its init operation, leaving only private preparation.
    s.cmd()
        .args(["--json", "--yes", "init"])
        .arg(&missing)
        .assert()
        .code(2);
    assert!(!missing.exists());
    let h = s
        .cmd()
        .args(["--json", "history"])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let h: serde_json::Value = serde_json::from_slice(&h).unwrap();
    let d = &h["transactions"][0];
    assert_eq!(d["journal_schema"], 3);
    let rid = d["repository_id"].as_str().unwrap();
    let tid = d["transaction_id"].as_str().unwrap();
    let blocked = s
        .cmd()
        .args(["--json", "--yes", "init"])
        .arg(&s.repo)
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    let blocked: serde_json::Value = serde_json::from_slice(&blocked).unwrap();
    assert_eq!(blocked["kind"], "validation");
    let message = blocked["error"].as_str().unwrap();
    for fact in [
        "An earlier operation did not finish",
        "New changes are paused",
        "Repository setup",
        rid,
        tid,
        "dort recovery list",
    ] {
        assert!(message.contains(fact), "{message}");
    }
    let human = s
        .cmd()
        .args(["--yes", "init"])
        .arg(&s.repo)
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains(tid) && human.contains("Open Recovery"));
    assert!(
        !human.contains("incomplete or malformed") && !human.contains("Possible partial effects")
    );
    s.cmd()
        .args(["--json", "--dry-run", "recovery", "resume", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--dry-run", "recovery", "abandon", rid, tid])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().code(4);
    s.cmd()
        .args(["--json", "recovery", "abandon", rid, tid])
        .assert()
        .code(2);
    s.cmd()
        .args(["--json", "--yes", "recovery", "abandon", rid, tid])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().success();
    assert!(!missing.exists());
    s.init();
}

#[test]
fn blocked_plans_and_lock_contention_do_not_execute() {
    let s = Sandbox::new();
    s.init();
    s.repo()
        .args([
            "--json",
            "--yes",
            "files",
            "add",
            "~/.missing",
            "--id",
            "a",
            "--source",
            "files/a",
        ])
        .assert()
        .code(4);
    assert!(!s.repo.join("files").exists());
    let host = dort_core::context::HostContext::new(
        s.home.clone(),
        None,
        None,
        dort_core::platform::Platform::Linux,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let _lock = dort_core::state::WriterLock::acquire(&host).unwrap();
    s.repo()
        .args(["--json", "--yes", "files", "apply"])
        .assert()
        .code(3);
    s.repo()
        .args(["--json", "--dry-run", "files", "apply"])
        .assert()
        .success();
    s.cmd().args(["--json", "history"]).assert().success();
}

#[test]
fn saved_profile_off_keeps_installed_link_and_temporary_flag_wins_once() {
    let s = Sandbox::new();
    fs::write(s.repo.join("source"), "settings\n").unwrap();
    fs::write(s.repo.join("dort.toml"), "schema=1\n[profiles]\navailable=['work']\ndefault=['work']\n[[files]]\nid='workrc'\nsource='source'\ntarget='~/.workrc'\nmode='link'\nprofiles=['work']\n").unwrap();
    s.init();
    s.cmd().args(["--yes", "files", "apply"]).assert().success();
    let link = s.home.join(".workrc");
    let target = fs::read_link(&link).unwrap();
    s.cmd()
        .args(["--yes", "profiles", "set", "work", "off"])
        .assert()
        .success();
    assert!(
        String::from_utf8_lossy(
            &s.cmd()
                .args(["profiles", "list"])
                .assert()
                .success()
                .get_output()
                .stdout
        )
        .contains("work: Off (saved choice)")
    );
    assert!(
        String::from_utf8_lossy(&s.cmd().arg("plan").assert().success().get_output().stdout)
            .contains("workrc: No longer selected")
    );
    assert!(
        String::from_utf8_lossy(
            &s.cmd()
                .args(["plan", "--profile", "work"])
                .assert()
                .success()
                .get_output()
                .stdout
        )
        .contains("work: On (temporary for this run)")
    );
    assert!(
        String::from_utf8_lossy(
            &s.cmd()
                .args(["profiles", "list"])
                .assert()
                .success()
                .get_output()
                .stdout
        )
        .contains("work: Off (saved choice)")
    );
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read_to_string(&link).unwrap(), "settings\n");
    s.cmd()
        .args(["--yes", "profiles", "set", "base", "off"])
        .assert()
        .code(2);
}

#[test]
fn clone_review_and_invalid_manifest_keep_home_targets_untouched() {
    let s = Sandbox::new();
    let git = std::process::Command::new("git")
        .arg("--version")
        .output()
        .unwrap();
    assert!(git.status.success());
    let source = s.root.join("remote");
    fs::create_dir(&source).unwrap();
    let run = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .current_dir(&source)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    run(&["init", "-q"]);
    fs::write(source.join("dort.toml"), "schema=999\n").unwrap();
    run(&["add", "."]);
    run(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.org",
        "commit",
        "-qm",
        "invalid",
    ]);
    let invoke = |flags: &[&str], name: &str| {
        let mut c = s.cmd();
        c.env("PATH", std::env::var_os("PATH").unwrap())
            .args(flags)
            .arg("clone")
            .arg(&source)
            .arg(s.root.join(name));
        c
    };
    invoke(&["--dry-run"], "invalid").assert().success();
    invoke(&[], "invalid").assert().code(2);
    assert!(!s.root.join("invalid").exists());
    s.assert_no_state();
    invoke(&["--yes"], "invalid").assert().code(2);
    assert!(s.root.join("invalid/.git").is_dir());
    assert!(!s.home.join(".config").exists());
    fs::write(
        source.join("dort.toml"),
        "schema=1\n[[files]]\nid='editor'\nsource='settings'\ntarget='~/.editor'\nmode='link'\n",
    )
    .unwrap();
    fs::write(source.join("settings"), "settings\n").unwrap();
    run(&["add", "."]);
    run(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.org",
        "commit",
        "-qm",
        "valid",
    ]);
    invoke(&["--yes"], "valid").assert().success();
    invoke(&["--yes"], "valid").assert().code(2);
    assert!(!s.home.join(".editor").exists());
    s.cmd().args(["profiles", "list"]).assert().success();
}

#[test]
fn local_input_cli_reviews_private_values_and_keeps_repository_and_mapping_scope() {
    let s = Sandbox::new();
    s.init();
    let manifest = r#"schema = 1
[[files]]
id = "editor"
mode = "template"
target = "~/settings"
syntax = "jinja"
local_inputs = ["name", "unused"]
template = "editor={{ local.name }}"
[[files]]
id = "other"
mode = "template"
target = "~/other"
syntax = "jinja"
local_inputs = ["name"]
template = "{{ local.name }}"
"#;
    let path = s.repo.join("dort.toml");
    fs::write(&path, manifest).unwrap();
    let alternate = s.root.join("alternate");
    fs::create_dir_all(alternate.join(".git")).unwrap();
    fs::write(alternate.join("dort.toml"), manifest).unwrap();
    let output = s
        .cmd()
        .arg("--repo")
        .arg(&alternate)
        .args(["files", "local-inputs", "editor"])
        .assert()
        .failure();
    assert!(
        String::from_utf8_lossy(&output.get_output().stderr).contains("Register this repository")
    );
    s.cmd()
        .args(["--yes", "init"])
        .arg(&alternate)
        .assert()
        .success();
    let json = |mut cmd: Command, args: &[&str]| {
        let output = cmd.args(args).assert().success();
        serde_json::from_slice::<serde_json::Value>(&output.get_output().stdout).unwrap()
    };
    let rows = |id| json(s.repo(), &["--json", "files", "local-inputs", id]);
    assert_eq!(
        rows("editor")["inputs"][0]["value"],
        serde_json::Value::Null
    );
    s.repo()
        .args([
            "--yes",
            "files",
            "set-local-input",
            "editor",
            "name",
            "--dry-run",
        ])
        .assert()
        .failure();
    assert_eq!(
        rows("editor")["inputs"][0]["value"],
        serde_json::Value::Null
    );
    let proposal = json(
        s.repo(),
        &[
            "--json",
            "--dry-run",
            "--yes",
            "files",
            "set-local-input",
            "editor",
            "name",
            "",
        ],
    );
    assert_eq!(proposal["previous"], serde_json::Value::Null);
    assert_eq!(proposal["proposed"], "");
    assert_eq!(
        rows("editor")["inputs"][0]["value"],
        serde_json::Value::Null
    );
    let set = [
        "files",
        "set-local-input",
        "editor",
        "name",
        "a\tprivate\nvalue",
    ];
    let denied = s.repo().args(set).assert().failure().stdout("");
    assert!(String::from_utf8_lossy(&denied.get_output().stderr).contains("write requires --yes"));
    let mut set_cmd = s.repo();
    set_cmd.args(["--yes", "--json"]);
    let result = json(set_cmd, &set);
    assert_eq!(result["changed"], true);
    assert!(!result.to_string().contains("private"));
    assert_eq!(rows("editor")["inputs"][0]["value"], "a\tprivate\nvalue");
    assert_eq!(
        rows("editor")["inputs"][1]["value"],
        serde_json::Value::Null
    );
    assert_eq!(rows("other")["inputs"][0]["value"], serde_json::Value::Null);
    let mut alt_cmd = s.cmd();
    alt_cmd.arg("--repo").arg(&alternate);
    assert_eq!(
        json(alt_cmd, &["--json", "files", "local-inputs", "editor"])["inputs"][0]["value"],
        serde_json::Value::Null
    );
    let output = s
        .repo()
        .args(["files", "local-inputs", "editor"])
        .assert()
        .success();
    assert!(
        String::from_utf8_lossy(&output.get_output().stdout).contains("\"a\\tprivate\\nvalue\"")
    );
    let rendered = json(
        s.repo(),
        &["--json", "files", "preview-generated", "editor"],
    );
    assert_eq!(rendered["rendered"], "editor=a\tprivate\nvalue");
    assert_eq!(
        rendered["resolved_inputs"]["local.name"]["provenance"],
        "machine_local"
    );
    let output = s
        .repo()
        .args([
            "--yes",
            "files",
            "set-local-input",
            "editor",
            "undeclared",
            "not-recorded",
        ])
        .assert()
        .failure();
    assert!(String::from_utf8_lossy(&output.get_output().stderr).contains("Declare the input"));
    s.repo()
        .args(["--yes", "files", "set-local-input", "editor", "name", ""])
        .assert()
        .success();
    assert_eq!(rows("editor")["inputs"][0]["value"], "");
    assert_eq!(
        json(
            s.repo(),
            &[
                "--json",
                "--dry-run",
                "files",
                "clear-local-input",
                "editor",
                "name"
            ]
        )["proposed"],
        serde_json::Value::Null
    );
    assert_eq!(rows("editor")["inputs"][0]["value"], "");
    s.repo()
        .args(["--yes", "files", "clear-local-input", "editor", "name"])
        .assert()
        .success();
    assert_eq!(
        rows("editor")["inputs"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(fs::read_to_string(path).unwrap(), manifest);
    assert_eq!(
        fs::read_to_string(alternate.join("dort.toml")).unwrap(),
        manifest
    );
    assert!(!s.home.join("settings").exists());
    assert!(!s.home.join("other").exists());
}

#[test]
fn template_files_cli_reviews_exact_bytes_and_keeps_displaced_copies() {
    let s = Sandbox::new();
    s.init();
    let source = s.root.join("draft.txt");
    let path = "templates/editor.jinja";
    let destination = s.repo.join(path);
    let original = "key=1 \r\nlast";
    fs::write(&source, original).unwrap();
    let write = |verb: &str, flags: &[&str]| {
        s.repo()
            .args(flags)
            .args(["files", "templates", verb, path, "--from"])
            .arg(&source)
            .assert()
    };
    let missing = write("create", &["--dry-run"]).code(2);
    assert!(
        String::from_utf8_lossy(&missing.get_output().stderr)
            .contains("Template folder is missing")
    );
    assert!(!s.repo.join("templates").exists());
    fs::create_dir(s.repo.join("templates")).unwrap();
    let review = write("create", &["--json", "--dry-run", "--yes"]).success();
    let review: serde_json::Value = serde_json::from_slice(&review.get_output().stdout).unwrap();
    assert_eq!(review["after"], original);
    assert!(!destination.exists());
    write("create", &[]).code(2);
    assert!(!destination.exists());
    write("create", &["--yes"])
        .success()
        .stdout("Created template file.\n");
    assert_eq!(fs::read(&destination).unwrap(), original.as_bytes());
    s.repo()
        .args(["files", "templates", "list"])
        .assert()
        .success()
        .stdout("templates/editor.jinja | 0 mappings | Editable\n");
    let shown = s
        .repo()
        .args(["files", "templates", "show", path])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&shown.get_output().stdout).contains("|key=1 \\r\\n|\n|last|"));

    fs::write(&source, "key=2\n").unwrap();
    let result = write("edit", &["--json", "--yes"]).success();
    let result: serde_json::Value = serde_json::from_slice(&result.get_output().stdout).unwrap();
    assert_eq!(result["outcome"], "edited");
    assert_eq!(fs::read(&destination).unwrap(), b"key=2\n");
    for name in ["reviewed", "displaced"] {
        assert_eq!(
            fs::read(result["retained"][name].as_str().unwrap()).unwrap(),
            original.as_bytes()
        );
    }
    assert_eq!(fs::read(s.repo.join("dort.toml")).unwrap(), b"schema = 1\n");
}
