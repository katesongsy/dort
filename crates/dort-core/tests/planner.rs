mod support;
use dort_core::{
    model::{FileMode, ManagedFile},
    planner::{self, Effect, Outcome, Request, UnmanageMode},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
use support::TestHome;

fn manifest(repo: &Path, entries: &[(&str, &str, &str)]) {
    let mut text = String::from("schema=1\n");
    for (id, source, target) in entries {
        text.push_str(&format!(
            "[[files]]\nid={id:?}\nsource={source:?}\ntarget={target:?}\nmode='link'\n"
        ));
    }
    fs::write(repo.join("dort.toml"), text).unwrap();
}

fn adopt() -> Request {
    Request::Adopt {
        mapping: ManagedFile {
            id: "zshrc".into(),
            source: PathBuf::from("files/zshrc"),
            target: PathBuf::from("~/.zshrc"),
            mode: FileMode::Link,
            profiles: vec!["base".into()],
            requires: vec![],
            overrides: Default::default(),
            permissions: None,
        },
    }
}

#[test]
fn apply_is_deterministic_sorted_and_the_planner_reads_no_live_files() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    manifest(&repo, &[("z", "z", "~/.z"), ("a", "a", "~/.a")]);
    fs::write(repo.join("z"), "z").unwrap();
    fs::write(repo.join("a"), "a").unwrap();
    let snapshot = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
    let first = planner::plan(snapshot.clone());
    assert_eq!(
        first
            .files
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert!(!first.has_blockers());
    assert!(first.preview_only);
    // Delete every observed live input before planning the saved snapshot again.
    fs::remove_dir_all(&repo).unwrap();
    fs::remove_dir_all(&home.host.home).unwrap();
    let second = planner::plan(snapshot);
    assert_eq!(first.human(), second.human());
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}

#[test]
fn correct_absolute_and_relative_links_are_noops_but_foreign_links_block() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    manifest(&repo, &[("zshrc", "zshrc", "~/.zshrc")]);
    fs::write(repo.join("zshrc"), "safe").unwrap();
    let target = home.host.home.join(".zshrc");
    for link in [repo.join("zshrc"), PathBuf::from("../repo/zshrc")] {
        symlink(link, &target).unwrap();
        let plan = planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap());
        assert!(matches!(plan.files[0].outcome, Outcome::NoOp));
        assert!(plan.files[0].effects.is_empty());
        fs::remove_file(&target).unwrap();
    }
    for link in ["/missing", ".zshrc"] {
        symlink(link, &target).unwrap();
        let plan = planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap());
        assert!(plan.has_blockers());
        assert!(plan.files[0].effects.is_empty());
        fs::remove_file(&target).unwrap();
    }
}

#[test]
fn apply_existing_regular_destination_requires_backup_and_shows_permissions() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    manifest(&repo, &[("zshrc", "zshrc", "~/.zshrc")]);
    fs::write(repo.join("zshrc"), "new").unwrap();
    fs::set_permissions(repo.join("zshrc"), fs::Permissions::from_mode(0o640)).unwrap();
    let target = home.host.home.join(".zshrc");
    fs::write(&target, "old").unwrap();
    let plan = planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap());
    assert!(!plan.has_blockers());
    assert!(plan.files[0].destructive);
    assert!(
        matches!(plan.files[0].effects.first(), Some(Effect::Backup { path }) if path == &target)
    );
    assert!(plan.human().contains("0640"));
    assert_eq!(fs::read_to_string(target).unwrap(), "old");
    assert!(!home.host.state_dir.exists());
}

#[test]
fn adoption_plans_one_transaction_without_changing_source_manifest_or_destination() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    let original_manifest = fs::read(repo.join("dort.toml")).unwrap();
    let target = home.host.home.join(".zshrc");
    fs::write(&target, "ADOPTION-CONTENT-SENTINEL").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let snapshot = planner::observe(&home.host, &repo, &[], adopt()).unwrap();
    let plan = planner::plan(snapshot);
    assert!(!plan.has_blockers());
    assert!(
        matches!(plan.files[0].effects.as_slice(), [Effect::Backup { .. }, Effect::EnsureParents { .. }, Effect::CopySource { permissions, .. }, Effect::AddMapping { .. }, Effect::Backup { .. }, Effect::Link { .. }] if permissions == "0600")
    );
    assert!(plan.human().contains("recoverable transaction"));
    assert!(!plan.human().contains("ADOPTION-CONTENT-SENTINEL"));
    assert!(!format!("{plan:?}").contains("ADOPTION-CONTENT-SENTINEL"));
    assert!(!repo.join("files").exists());
    assert_eq!(fs::read(repo.join("dort.toml")).unwrap(), original_manifest);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "ADOPTION-CONTENT-SENTINEL"
    );
    assert!(!home.host.config_dir.exists());
    assert!(!home.host.state_dir.exists());
}

#[test]
fn adoption_refuses_existing_source_duplicate_mapping_and_unsafe_source_paths() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    fs::write(home.host.home.join(".zshrc"), "safe").unwrap();
    fs::create_dir(repo.join("files")).unwrap();
    fs::write(repo.join("files/zshrc"), "existing").unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap()).has_blockers()
    );
    fs::remove_file(repo.join("files/zshrc")).unwrap();
    for source in ["../escape", "/absolute", ".git/config", "dort.toml"] {
        let Request::Adopt { mut mapping } = adopt() else {
            unreachable!()
        };
        mapping.source = source.into();
        assert!(planner::observe(&home.host, &repo, &[], Request::Adopt { mapping }).is_err());
    }
    manifest(&repo, &[("existing", "files/zshrc", "~/.other")]);
    assert!(
        planner::observe(&home.host, &repo, &[], adopt())
            .unwrap_err()
            .to_string()
            .contains("source overlaps")
    );
    manifest(&repo, &[("zshrc", "other", "~/.other")]);
    assert!(planner::observe(&home.host, &repo, &[], adopt()).is_err());
}

#[test]
fn adoption_refuses_links_directories_special_files_hardlinks_and_sensitive_content() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    let target = home.host.home.join(".zshrc");
    symlink("/missing", &target).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap()).has_blockers()
    );
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap()).has_blockers()
    );
    fs::remove_dir(&target).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(&target).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap()).has_blockers()
    );
    fs::remove_file(&target).unwrap();
    fs::write(&target, "safe").unwrap();
    fs::hard_link(&target, home.host.home.join("hard")).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap()).has_blockers()
    );
    fs::remove_file(home.host.home.join("hard")).unwrap();
    fs::write(&target, "password = SENSITIVE-SENTINEL").unwrap();
    let plan = planner::plan(planner::observe(&home.host, &repo, &[], adopt()).unwrap());
    assert!(plan.has_blockers());
    assert!(plan.files[0].effects.is_empty());
    assert!(!format!("{plan:?}").contains("SENSITIVE-SENTINEL"));
}

#[test]
fn source_and_target_symlinked_parents_block_without_reading_outside_roots() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    let outside = home.repository("outside");
    fs::write(outside.join("zshrc"), "SENTINEL").unwrap();
    symlink(&outside, repo.join("files")).unwrap();
    manifest(&repo, &[("zshrc", "files/zshrc", "~/.zshrc")]);
    let plan = planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap());
    assert!(plan.has_blockers());
    assert!(plan.inputs.files[0].source.is_none());
    fs::remove_file(repo.join("files")).unwrap();
    fs::write(repo.join("source"), "safe").unwrap();
    symlink(&outside, home.host.home.join("alias")).unwrap();
    manifest(&repo, &[("zshrc", "source", "~/alias/zshrc")]);
    let plan = planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap());
    assert!(plan.has_blockers());
    assert!(plan.inputs.files[0].destination.is_none());
}

#[test]
fn unmanage_keep_materializes_and_remove_requires_distinct_acknowledgement() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    manifest(&repo, &[("zshrc", "zshrc", "~/.zshrc")]);
    let source = repo.join("zshrc");
    let target = home.host.home.join(".zshrc");
    fs::write(&source, "retained").unwrap();
    symlink(&source, &target).unwrap();
    for mode in [UnmanageMode::Keep, UnmanageMode::Remove] {
        let plan = planner::plan(
            planner::observe(
                &home.host,
                &repo,
                &[],
                Request::Unmanage {
                    id: "zshrc".into(),
                    mode,
                },
            )
            .unwrap(),
        );
        assert!(!plan.has_blockers());
        let file = &plan.files[0];
        assert!(matches!(
            file.effects.as_slice(),
            [
                Effect::Backup { .. },
                Effect::Backup { .. },
                _,
                Effect::RemoveMapping { .. }
            ]
        ));
        assert_eq!(
            file.requires_removal_acknowledgement,
            mode == UnmanageMode::Remove
        );
        match mode {
            UnmanageMode::Keep => assert!(matches!(file.effects[2], Effect::Materialize { .. })),
            UnmanageMode::Remove => {
                assert!(matches!(file.effects[2], Effect::RemoveDestination { .. }))
            }
        }
        assert!(target.is_symlink());
        assert_eq!(fs::read_to_string(&source).unwrap(), "retained");
    }
    fs::remove_file(&target).unwrap();
    fs::write(&target, "user drift").unwrap();
    let plan = planner::plan(
        planner::observe(
            &home.host,
            &repo,
            &[],
            Request::Unmanage {
                id: "zshrc".into(),
                mode: UnmanageMode::Remove,
            },
        )
        .unwrap(),
    );
    assert!(plan.has_blockers());
    assert!(plan.files[0].effects.is_empty());
    assert_eq!(fs::read_to_string(target).unwrap(), "user drift");
}

#[test]
fn unsupported_modes_and_unverified_requirements_block_before_content_reads() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    fs::write(repo.join("source"), "SENTINEL").unwrap();
    for mode in ["template", "secret"] {
        fs::write(
            repo.join("dort.toml"),
            format!(
                "schema=1\n[[files]]\nid='file'\nsource='source'\ntarget='~/.file'\nmode='{mode}'\n"
            ),
        )
        .unwrap();
        let snapshot = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
        assert!(snapshot.files[0].source.as_ref().unwrap().sha256.is_none());
        assert!(planner::plan(snapshot).has_blockers());
    }
    fs::write(repo.join("dort.toml"), "schema=1\n[[tools]]\nid='git'\nbinary='git'\n[tools.install.ubuntu]\nprovider='apt'\npackage='git'\n[[files]]\nid='file'\nsource='source'\ntarget='~/.file'\nmode='link'\nrequires=['git']\n").unwrap();
    let snapshot = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
    assert!(snapshot.files[0].source.as_ref().unwrap().sha256.is_none());
    assert!(planner::plan(snapshot).has_blockers());
}

#[test]
fn active_profiles_overrides_collisions_and_protected_targets_are_enforced() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    fs::write(repo.join("platform"), "safe").unwrap();
    fs::write(repo.join("dort.toml"), "schema=1\n[profiles]\navailable=['dev']\n[[files]]\nid='file'\nsource='missing'\ntarget='~/.file'\nmode='link'\nprofiles=['dev']\n[files.overrides]\nubuntu='platform'\n").unwrap();
    assert!(
        planner::observe(&home.host, &repo, &[], Request::Apply)
            .unwrap()
            .files
            .is_empty()
    );
    let plan = planner::plan(
        planner::observe(&home.host, &repo, &["dev".into()], Request::Apply).unwrap(),
    );
    assert!(!plan.has_blockers());
    assert_eq!(plan.inputs.files[0].source_path, repo.join("platform"));
    for target in [
        home.host.config_dir.join("x"),
        home.host.state_dir.join("x"),
        repo.join("x"),
        PathBuf::from("/etc/file"),
    ] {
        manifest(&repo, &[("file", "platform", target.to_str().unwrap())]);
        assert!(planner::observe(&home.host, &repo, &[], Request::Apply).is_err());
    }
    manifest(
        &repo,
        &[
            ("a", "platform", "~/.same"),
            ("b", "platform", "~/.same/child"),
        ],
    );
    assert!(planner::observe(&home.host, &repo, &[], Request::Apply).is_err());
}

#[test]
fn missing_sensitive_destinations_and_link_or_hardlinked_sources_are_blocked() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    fs::write(repo.join("source"), "safe").unwrap();
    manifest(&repo, &[("file", "source", "~/.ssh/id_ed25519")]);
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap())
            .has_blockers()
    );
    manifest(&repo, &[("file", "alias", "~/.file")]);
    symlink("source", repo.join("alias")).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap())
            .has_blockers()
    );
    fs::remove_file(repo.join("alias")).unwrap();
    fs::hard_link(repo.join("source"), repo.join("alias")).unwrap();
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap())
            .has_blockers()
    );
}

#[test]
fn snapshots_bind_manifest_and_source_content_and_manifest_links_are_refused() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    manifest(&repo, &[("file", "source", "~/.file")]);
    fs::write(repo.join("source"), "before").unwrap();
    let first = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
    fs::write(repo.join("source"), "after").unwrap();
    let mut text = fs::read_to_string(repo.join("dort.toml")).unwrap();
    text.push_str("\n# reviewed comment\n");
    fs::write(repo.join("dort.toml"), text).unwrap();
    let second = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
    assert_ne!(first.manifest.sha256, second.manifest.sha256);
    assert_ne!(
        first.files[0].source.as_ref().unwrap().sha256,
        second.files[0].source.as_ref().unwrap().sha256
    );
    fs::rename(repo.join("dort.toml"), repo.join("saved.toml")).unwrap();
    symlink("saved.toml", repo.join("dort.toml")).unwrap();
    assert!(planner::observe(&home.host, &repo, &[], Request::Apply).is_err());
    fs::remove_file(repo.join("dort.toml")).unwrap();
    fs::hard_link(repo.join("saved.toml"), repo.join("dort.toml")).unwrap();
    assert!(planner::observe(&home.host, &repo, &[], Request::Apply).is_err());
}

#[test]
fn swapped_parent_after_observation_does_not_affect_pure_plan_and_reobservation_blocks() {
    let home = TestHome::new();
    let repo = home.repository("repo");
    fs::create_dir(repo.join("files")).unwrap();
    fs::write(repo.join("files/zshrc"), "safe").unwrap();
    manifest(&repo, &[("zshrc", "files/zshrc", "~/.zshrc")]);
    let snapshot = planner::observe(&home.host, &repo, &[], Request::Apply).unwrap();
    let before = planner::plan(snapshot.clone()).human();
    fs::rename(repo.join("files"), repo.join("moved")).unwrap();
    symlink(repo.join("moved"), repo.join("files")).unwrap();
    assert_eq!(before, planner::plan(snapshot).human());
    assert!(
        planner::plan(planner::observe(&home.host, &repo, &[], Request::Apply).unwrap())
            .has_blockers()
    );
}
