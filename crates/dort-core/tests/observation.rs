mod support;
use dort_core::{
    discovery,
    observation::{self, Kind, LinkState},
};
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    },
};
use support::TestHome;

#[test]
fn regular_snapshots_hash_without_exposing_contents_and_change_with_edits() {
    let home = TestHome::new();
    let path = home.host.home.join(".zshrc");
    fs::write(&path, "SAFE-PRIVATE-SENTINEL").unwrap();
    let first = observation::inspect(&path, true).unwrap();
    assert_eq!(first.kind, Kind::Regular);
    assert_eq!(first.sha256.as_ref().unwrap().len(), 64);
    assert!(!format!("{first:?}").contains("SAFE-PRIVATE-SENTINEL"));
    assert!(!first.parents.is_empty());
    fs::write(&path, "edited").unwrap();
    let second = observation::inspect(&path, true).unwrap();
    assert_ne!(first.sha256, second.sha256);
    assert_ne!(first.fingerprint, second.fingerprint);
    assert!(!home.host.state_dir.exists());
}

#[test]
fn racing_terminal_symlink_substitution_never_reads_the_referent() {
    use sha2::{Digest, Sha256};
    let home = TestHome::new();
    let path = home.host.home.join("file");
    let saved = home.host.home.join("saved");
    let outside = home.repository("outside").join("foreign");
    fs::write(&path, "safe").unwrap();
    fs::write(&outside, "FOREIGN-CONTENT-SENTINEL").unwrap();
    let expected = format!("{:x}", Sha256::digest(b"safe"));
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..200 {
            fs::rename(&writer_path, &saved).unwrap();
            symlink(&outside, &writer_path).unwrap();
            fs::remove_file(&writer_path).unwrap();
            fs::rename(&saved, &writer_path).unwrap();
        }
    });
    for _ in 0..200 {
        if let Ok(entry) = observation::inspect(&path, true) {
            if let Some(hash) = &entry.sha256 {
                assert_eq!(hash, &expected);
            }
            assert!(!format!("{entry:?}").contains("FOREIGN-CONTENT-SENTINEL"));
        }
    }
    writer.join().unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "safe");
}

#[test]
fn missing_parents_are_snapshotted_without_creating_them() {
    let home = TestHome::new();
    let path = home.host.home.join("absent/child/file");
    let entry = observation::inspect(&path, true).unwrap();
    assert_eq!(entry.kind, Kind::Missing);
    assert_eq!(entry.parents.last().unwrap().path, home.host.home);
    assert!(!home.host.home.join("absent").exists());
}

#[test]
fn symlinked_parents_and_parent_loops_are_never_followed() {
    let home = TestHome::new();
    let outside = home.repository("outside");
    fs::write(outside.join("file"), "not to be read").unwrap();
    symlink(&outside, home.host.home.join("alias")).unwrap();
    assert!(observation::inspect(&home.host.home.join("alias/file"), true).is_err());
    symlink("loop", home.host.home.join("loop")).unwrap();
    assert!(observation::inspect(&home.host.home.join("loop/file"), true).is_err());
    fs::write(home.host.home.join("ordinary"), "x").unwrap();
    assert!(observation::inspect(&home.host.home.join("ordinary/child"), true).is_err());
}

#[test]
fn terminal_links_are_classified_without_hashing_their_referents() {
    let home = TestHome::new();
    fs::write(home.host.home.join("file"), "x").unwrap();
    for (name, target, expected) in [
        ("good", "file", LinkState::Regular),
        ("broken", "missing", LinkState::Broken),
        ("loop", "loop", LinkState::Loop),
        ("directory", ".", LinkState::Directory),
    ] {
        let path = home.host.home.join(name);
        symlink(target, &path).unwrap();
        let entry = observation::inspect(&path, true).unwrap();
        assert_eq!(entry.kind, Kind::Symlink);
        assert!(entry.sha256.is_none());
        assert_eq!(observation::link_state(&entry).unwrap(), expected);
    }
    symlink("b", home.host.home.join("a")).unwrap();
    symlink("a", home.host.home.join("b")).unwrap();
    assert_eq!(
        observation::link_state(&observation::inspect(&home.host.home.join("a"), false).unwrap())
            .unwrap(),
        LinkState::Loop
    );
}

#[test]
fn special_files_directories_and_hardlinks_are_not_read() {
    let home = TestHome::new();
    let fifo = home.host.home.join("pipe");
    let _socket = std::os::unix::net::UnixListener::bind(&fifo).unwrap();
    assert_eq!(
        observation::inspect(&fifo, true).unwrap().kind,
        Kind::Special
    );
    assert_eq!(
        observation::inspect(&home.host.home, true).unwrap().kind,
        Kind::Directory
    );
    let path = home.host.home.join("file");
    fs::write(&path, "never hash hardlinks").unwrap();
    fs::hard_link(&path, home.host.home.join("other")).unwrap();
    let entry = observation::inspect(&path, true).unwrap();
    assert_eq!(entry.fingerprint.unwrap().links, 2);
    assert!(entry.sha256.is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn fifo_is_observed_without_blocking_or_reading() {
    let home = TestHome::new();
    let path = home.host.home.join("fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let entry = observation::inspect(&path, true).unwrap();
    assert_eq!(entry.kind, Kind::Special);
    assert!(entry.sha256.is_none());
}

#[test]
fn discovery_rejects_unbounded_directories_and_stops_at_config_depth_limit() {
    let home = TestHome::new();
    fs::create_dir_all(home.host.home.join(".config/a/b/c")).unwrap();
    fs::write(home.host.home.join(".config/a/b/c/too-deep"), "x").unwrap();
    let report = discovery::discover(&home.host, None).unwrap();
    assert!(
        !report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with("too-deep"))
    );
    for index in 0..4097 {
        fs::write(home.host.home.join(format!("visible-{index}")), "").unwrap();
    }
    assert!(
        discovery::discover(&home.host, None)
            .unwrap_err()
            .to_string()
            .contains("limit")
    );
}

#[test]
fn non_utf8_paths_and_link_targets_fail_explicitly_without_panicking() {
    let home = TestHome::new();
    let path = home.host.home.join(OsString::from_vec(vec![b'.', 0xff]));
    assert!(
        observation::inspect(&path, false)
            .unwrap_err()
            .to_string()
            .contains("non-UTF-8")
    );
    // APFS rejects non-UTF-8 names at creation; Linux also exercises real entries.
    #[cfg(target_os = "linux")]
    {
        fs::write(&path, "x").unwrap();
        assert!(
            discovery::discover(&home.host, None)
                .unwrap_err()
                .to_string()
                .contains("non-UTF-8")
        );
        let link = home.host.home.join("link");
        symlink(OsString::from_vec(vec![0xff]), &link).unwrap();
        assert!(
            observation::inspect(&link, false)
                .unwrap_err()
                .to_string()
                .contains("non-UTF-8")
        );
    }
}

#[test]
fn sensitivity_blocks_hashes_and_bounded_reads_reject_large_files() {
    let home = TestHome::new();
    let path = home.host.home.join(".zshrc");
    fs::write(&path, "-----BEGIN OPENSSH PRIVATE KEY-----\nSENTINEL").unwrap();
    let entry = observation::inspect(&path, true).unwrap();
    assert!(entry.sensitive);
    assert!(entry.sha256.is_none());
    assert!(!format!("{entry:?}").contains("SENTINEL"));
    let sensitive = home.host.home.join(".env");
    fs::write(&sensitive, "SENTINEL").unwrap();
    fs::set_permissions(&sensitive, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(observation::inspect(&sensitive, true).unwrap().sensitive);
    let large = home.host.home.join("large");
    fs::File::create(&large)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        observation::inspect(&large, true)
            .unwrap_err()
            .to_string()
            .contains("limit")
    );
}

#[test]
fn discovery_is_metadata_only_bounded_sorted_and_excludes_sensitive_generated_data() {
    let home = TestHome::new();
    fs::create_dir_all(home.host.home.join(".config/editor")).unwrap();
    for file in [".zshrc", ".gitconfig", ".config/editor/settings"] {
        fs::write(
            home.host.home.join(file),
            "password=CONTENT-MUST-NOT-BE-READ",
        )
        .unwrap();
    }
    for dir in [
        ".ssh",
        ".cache",
        ".config/chromium",
        ".config/credentials",
        ".config/keyrings",
        ".config/editor/cache",
    ] {
        fs::create_dir_all(home.host.home.join(dir)).unwrap();
        fs::write(home.host.home.join(dir).join("excluded"), "SENTINEL").unwrap();
    }
    fs::write(home.host.home.join(".env.production"), "x").unwrap();
    fs::write(home.host.home.join(".zsh_history"), "x").unwrap();
    fs::write(home.host.home.join("visible"), "x").unwrap();
    let report = discovery::discover(&home.host, None).unwrap();
    let paths: Vec<_> = report
        .candidates
        .iter()
        .map(|c| {
            c.entry
                .path
                .strip_prefix(&home.host.home)
                .unwrap()
                .to_str()
                .unwrap()
        })
        .collect();
    assert_eq!(
        paths,
        [
            ".config/editor",
            ".config/editor/settings",
            ".gitconfig",
            ".zshrc"
        ]
    );
    assert!(report.candidates.iter().all(|c| c.entry.sha256.is_none()));
    assert!(report.sensitivity_notice.contains("heuristic"));
    assert!(!home.host.state_dir.exists());
}

#[test]
fn discovery_never_recurses_into_directory_symlinks_or_protected_roots() {
    let home = TestHome::new();
    let repository = home.host.home.join(".dotfiles");
    fs::create_dir(&repository).unwrap();
    fs::write(repository.join("sentinel"), "x").unwrap();
    fs::create_dir_all(home.host.home.join(".config/dort")).unwrap();
    fs::write(
        home.host.home.join(".config/dort/local.toml"),
        "private config",
    )
    .unwrap();
    fs::create_dir(home.host.home.join(".config/app")).unwrap();
    symlink(&repository, home.host.home.join(".config/app/foreign")).unwrap();
    let report = discovery::discover(&home.host, Some(&repository)).unwrap();
    assert!(
        report
            .candidates
            .iter()
            .all(|c| !c.entry.path.starts_with(&repository)
                && !c.entry.path.starts_with(&home.host.config_dir))
    );
    assert!(
        report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with("foreign") && !c.supported_for_adoption)
    );
    assert!(
        !report
            .candidates
            .iter()
            .any(|c| c.entry.path.ends_with("sentinel"))
    );
}
