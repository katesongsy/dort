mod support;

use dort_core::git::Git;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};
use support::TestHome;

struct Fixture {
    home: TestHome,
    root: PathBuf,
    executable: PathBuf,
    git: Git,
}

impl Fixture {
    fn new() -> Option<Self> {
        // Optional real-Git integration coverage; deterministic fake-backend and
        // missing-executable cases below never require an installed Git.
        let executable = [
            "/usr/bin/git",
            "/usr/local/bin/git",
            "/opt/homebrew/bin/git",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file());
        let Some(executable) = executable else {
            eprintln!("skipping real-Git fixture: Git not installed");
            return None;
        };
        let home = TestHome::new();
        let root = home.sandbox.path().canonicalize().unwrap().join("repo");
        fs::create_dir(&root).unwrap();
        let git = Git::new(executable.clone()).unwrap();
        let fixture = Self {
            home,
            root,
            executable,
            git,
        };
        fixture.command(&["init", "--initial-branch=main"]);
        Some(fixture)
    }

    fn command(&self, args: &[&str]) {
        let output = Command::new(&self.executable)
            .env_clear()
            .env("HOME", &self.home.host.home)
            .env("PATH", "/no-external-programs")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("LC_ALL", "C")
            .current_dir(&self.root)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit(&self) {
        self.command(&["add", "--all"]);
        self.command(&["commit", "-m", "fixture"]);
    }
}

// Compare identities/modes/mtime, directory layouts and file bytes, including
// index, config, refs and linked source. Ignore atime: read-only is not no-access-time.
type FileSnapshot = (PathBuf, u64, u32, i64, i64, Vec<u8>);
fn snapshot(root: &Path) -> Vec<FileSnapshot> {
    fn visit(path: &Path, result: &mut Vec<FileSnapshot>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        if metadata.is_dir() {
            result.push((
                path.into(),
                metadata.ino(),
                metadata.mode(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                Vec::new(),
            ));
            for entry in fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), result);
            }
        } else {
            let bytes = if metadata.is_symlink() {
                fs::read_link(path)
                    .unwrap()
                    .into_os_string()
                    .into_encoded_bytes()
            } else {
                fs::read(path).unwrap()
            };
            result.push((
                path.into(),
                metadata.ino(),
                metadata.mode(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                bytes,
            ));
        }
    }
    let mut result = Vec::new();
    visit(root, &mut result);
    result.sort();
    result
}

#[test]
fn unborn_dirty_staged_rename_and_unusual_paths_are_read_only() {
    let Some(f) = Fixture::new() else {
        return;
    };
    fs::write(f.root.join("space and\nnewline"), "first body\n").unwrap();
    let status = f.git.status(&f.root).unwrap();
    assert!(status.branch.iter().any(|h| h.contains("initial")));
    assert_eq!(status.entries[0].path, "space and\nnewline");
    assert!(
        f.git
            .diff(&f.root, false, false)
            .unwrap()
            .entries
            .is_empty()
    );
    f.command(&["add", "--all"]);
    assert_eq!(
        f.git.diff(&f.root, true, false).unwrap().entries[0].change,
        "A"
    );
    f.command(&["commit", "-m", "first"]);
    f.command(&["mv", "space and\nnewline", "new\tname"]);
    let rename = f.git.status(&f.root).unwrap();
    assert_eq!(
        rename.entries[0].original_path.as_deref(),
        Some("space and\nnewline")
    );
    assert_eq!(rename.entries[0].path, "new\tname");
    fs::write(f.root.join("new\tname"), "secret body\x1b[31m\n").unwrap();
    symlink(f.root.join("new\tname"), f.home.host.home.join(".live")).unwrap();
    let before = snapshot(f.home.sandbox.path());
    let summary = f.git.diff(&f.root, false, false).unwrap();
    assert!(summary.patch.is_none());
    assert!(
        !serde_json::to_string(&summary)
            .unwrap()
            .contains("secret body")
    );
    let report = f.git.diff(&f.root, false, true).unwrap();
    assert!(!format!("{report:?}").contains("secret body"));
    let patch = report.patch.unwrap();
    assert!(patch.contains("secret body\\x1b[31m"));
    assert!(!patch.contains('\x1b'));
    assert_eq!(f.git.diff(&f.root, true, false).unwrap().entries.len(), 2); // no rename detection
    f.git.status(&f.root).unwrap();
    assert_eq!(before, snapshot(f.home.sandbox.path()));
    assert_eq!(
        fs::read(f.home.host.home.join(".live")).unwrap(),
        b"secret body\x1b[31m\n"
    );
    assert!(!f.home.host.config_dir.exists());
    assert!(!f.home.host.state_dir.exists());
}

#[test]
fn configured_helpers_do_not_execute_and_filters_partial_clones_fail_closed() {
    let Some(f) = Fixture::new() else {
        return;
    };
    fs::write(f.root.join("tracked"), "first\n").unwrap();
    f.commit();
    fs::write(f.root.join("tracked"), "second\n").unwrap();
    let marker = f.root.join("EXECUTED");
    let script = f.root.join("helper");
    fs::write(
        &script,
        format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let program = script.to_str().unwrap();
    f.command(&["config", "core.fsmonitor", program]);
    let hooks = f.root.join("hooks");
    fs::create_dir(&hooks).unwrap();
    fs::copy(&script, hooks.join("post-index-change")).unwrap();
    f.command(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    f.command(&["config", "diff.external", program]);
    f.command(&["config", "diff.danger.command", program]);
    f.command(&["config", "diff.danger.textconv", program]);
    fs::write(
        f.root.join(".gitattributes"),
        "tracked diff=danger filter=danger\n",
    )
    .unwrap();
    let before = snapshot(&f.root);
    f.git.status(&f.root).unwrap();
    f.git.diff(&f.root, false, true).unwrap();
    assert_eq!(before, snapshot(&f.root));
    assert!(!marker.exists());
    for key in [
        "filter.danger.clean",
        "extensions.partialClone",
        "remote.origin.promisor",
    ] {
        f.command(&["config", key, program]);
        let before = snapshot(&f.root);
        for error in [
            f.git.status(&f.root).unwrap_err(),
            f.git.diff(&f.root, false, true).unwrap_err(),
        ] {
            assert!(error.to_string().contains("refused"));
            assert!(!error.to_string().contains(program));
        }
        assert_eq!(before, snapshot(&f.root));
        assert!(!marker.exists());
        f.command(&["config", "--unset", key]);
    }
    // Included repository configuration must be inspected too.
    let included = f.root.join("included-config");
    fs::write(
        &included,
        "[filter \"included\"]\nclean = must-not-execute\n",
    )
    .unwrap();
    f.command(&["config", "include.path", included.to_str().unwrap()]);
    assert!(
        f.git
            .status(&f.root)
            .unwrap_err()
            .to_string()
            .contains("refused")
    );
}

#[test]
fn exact_root_detached_head_and_conflict_metadata() {
    let Some(f) = Fixture::new() else {
        return;
    };
    fs::write(f.root.join("file"), "base\n").unwrap();
    f.commit();
    fs::create_dir(f.root.join("nested")).unwrap();
    assert!(f.git.status(&f.root.join("nested")).is_err());
    assert!(f.git.status(&f.home.host.home).is_err());
    f.command(&["checkout", "--detach"]);
    assert!(
        f.git
            .status(&f.root)
            .unwrap()
            .branch
            .iter()
            .any(|b| b.contains("detached"))
    );
    // Create an unmerged index without invoking merge machinery or external helpers.
    let output = Command::new(&f.executable)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(&f.root)
        .args(["rev-parse", "HEAD:file"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let hash = String::from_utf8(output.stdout).unwrap();
    use std::io::Write;
    let mut child = Command::new(&f.executable)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(&f.root)
        .args(["update-index", "--index-info"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let hash = hash.trim();
    write!(
        child.stdin.take().unwrap(),
        "0 {hash}\tfile\n100644 {hash} 1\tfile\n100644 {hash} 2\tfile\n100644 {hash} 3\tfile\n"
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(f.git.status(&f.root).unwrap().entries[0].kind, "unmerged");
    f.git.diff(&f.root, false, false).unwrap();
}

#[test]
fn binary_patches_unborn_index_and_linked_worktrees_remain_read_only() {
    let Some(f) = Fixture::new() else {
        return;
    };
    fs::write(f.root.join("binary"), b"\0PRIVATE-BINARY-BODY\xff").unwrap();
    f.command(&["add", "--all"]);
    let patch = f.git.diff(&f.root, true, true).unwrap().patch.unwrap();
    assert!(patch.contains("Binary files"));
    assert!(!patch.contains("PRIVATE-BINARY-BODY"));
    assert!(!patch.contains("GIT binary patch"));
    f.command(&["commit", "-m", "binary"]);
    let worktree = f
        .home
        .sandbox
        .path()
        .canonicalize()
        .unwrap()
        .join("worktree");
    f.command(&["worktree", "add", "--detach", worktree.to_str().unwrap()]);
    let before = snapshot(f.home.sandbox.path());
    assert!(f.git.status(&worktree).unwrap().entries.is_empty());
    assert!(
        f.git
            .diff(&worktree, true, true)
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(before, snapshot(f.home.sandbox.path()));
    let bare = f.home.sandbox.path().canonicalize().unwrap().join("bare");
    f.command(&["init", "--bare", bare.to_str().unwrap()]);
    assert!(f.git.status(&bare).is_err());
    f.command(&["config", "remote.origin.promisor", "false"]);
    assert!(
        f.git
            .status(&f.root)
            .unwrap_err()
            .to_string()
            .contains("refused")
    );
}

#[test]
fn missing_git_invalid_protocol_and_stderr_are_safe() {
    let h = TestHome::new();
    let root = h.sandbox.path().canonicalize().unwrap();
    assert!(Git::new(PathBuf::from("git")).is_err());
    let missing = Git::new(root.join("missing-git")).unwrap();
    assert!(
        missing
            .status(&root)
            .unwrap_err()
            .to_string()
            .contains("unavailable")
    );
    let fake = root.join("git");
    for (body, expected) in [
        (
            "printf 'PRIVATE-ERROR' >&2; exit 5",
            "configuration inspection failed",
        ),
        (
            "for arg do case \"$arg\" in config) exit 1;; rev-parse) pwd -P; exit 0;; status) printf 'bad'; exit 0;; esac; done",
            "NUL protocol",
        ),
    ] {
        fs::write(&fake, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let error = Git::new(fake.clone())
            .unwrap()
            .status(&root)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("PRIVATE-ERROR"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_paths_fail_without_lossy_identity() {
    use std::os::unix::ffi::OsStringExt;
    let Some(f) = Fixture::new() else {
        return;
    };
    fs::write(
        f.root.join(std::ffi::OsString::from_vec(vec![b'x', 255])),
        "body",
    )
    .unwrap();
    assert!(
        f.git
            .status(&f.root)
            .unwrap_err()
            .to_string()
            .contains("non-UTF-8")
    );
}
