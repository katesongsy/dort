mod support;
use dort_core::{
    Error, Manifest,
    adoption::Event,
    context::HostContext,
    journal,
    local::RepositoryRegistry,
    model::Tool,
    platform::{Architecture, Platform},
    recovery,
    state::WriterLock,
    tools::declaration::{self, Approval, Change},
};
use std::{fs, path::PathBuf, process::Command};
const ORIGINAL: &str = "# PRIVATE_COMMENT\nschema=1\n\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nprovider='brew'\npackage='demo'\n\n# unrelated tool\n[[tools]]\nid='other'\nbinary='other'\n[tools.install]\nprovider='brew'\npackage='other'\n";
fn edited_tool() -> Tool {
    let mut t = Manifest::parse(ORIGINAL).unwrap().tools.remove(0);
    t.binary = "new-demo".into();
    t.install.version = Some("latest".into());
    t
}
struct Fixture {
    home: support::TestHome,
    repo: PathBuf,
    id: String,
    lock: Option<WriterLock>,
}
impl Fixture {
    fn new() -> Self {
        let home = support::TestHome::new();
        let repo = home.repository("repo");
        fs::write(repo.join("dort.toml"), ORIGINAL).unwrap();
        let lock = WriterLock::acquire(&home.host).unwrap();
        let id = RepositoryRegistry::register(&home.host, &lock, &repo).unwrap();
        Self {
            home,
            repo,
            id,
            lock: Some(lock),
        }
    }
    fn preview(&self, change: Change) -> declaration::Preview {
        prepare(&self.home.host, &self.repo, change)
    }
    fn execute(
        &self,
        change: Change,
        cb: impl FnMut(&Event) -> dort_core::Result<()>,
    ) -> dort_core::Result<Option<String>> {
        declaration::execute(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            Approval::from_preview(self.preview(change)).unwrap(),
            cb,
        )
    }
    fn undo(&self, cb: impl FnMut(&Event) -> dort_core::Result<()>) -> dort_core::Result<()> {
        let d = journal::diagnose(&self.home.host)?;
        let p = recovery::preview(&self.home.host, &self.id, &d[0].transaction_id)?;
        recovery::execute_with_observer(
            &self.home.host,
            self.lock.as_ref().unwrap(),
            recovery::Approval::from_preview(p)?,
            cb,
        )
    }
    fn restored(&self) {
        assert_eq!(
            fs::read_to_string(self.repo.join("dort.toml")).unwrap(),
            ORIGINAL
        );
        assert!(!journal::diagnose(&self.home.host).unwrap()[0].blocks_writes());
    }
}
fn prepare(host: &HostContext, repo: &std::path::Path, change: Change) -> declaration::Preview {
    match change {
        Change::Edit => declaration::preview_edit(host, repo, &[], edited_tool()),
        Change::Remove => declaration::preview_remove(host, repo, &[], "demo"),
    }
    .unwrap()
}
fn boundaries(change: Change, inverse: bool) -> usize {
    let f = Fixture::new();
    let mut n = 0;
    if inverse {
        f.execute(change, |_| Ok(())).unwrap();
        f.undo(|_| {
            n += 1;
            Ok(())
        })
        .unwrap();
    } else {
        f.execute(change, |_| {
            n += 1;
            Ok(())
        })
        .unwrap();
    }
    n
}
#[test]
fn focused_edit_remove_and_noop_preserve_unrelated_metadata_and_recover_schema6() {
    for change in [Change::Edit, Change::Remove] {
        for inline in [false, true] {
            let f = Fixture::new();
            let original = if inline {
                "# PRIVATE_COMMENT\nschema=1\ntools=[{id='demo',binary='demo',install={provider='brew',package='demo'}}, # keep\n{id='other',binary='other',install={provider='brew',package='other'}}]\n"
            } else {
                ORIGINAL
            };
            fs::write(f.repo.join("dort.toml"), original).unwrap();
            let p = f.preview(change);
            let report = serde_json::to_string(&p).unwrap();
            assert!(!report.contains("PRIVATE_COMMENT"));
            assert!(p.before.is_some());
            assert_eq!(p.change, Some(change));
            f.execute(change, |_| Ok(())).unwrap();
            let result = fs::read_to_string(f.repo.join("dort.toml")).unwrap();
            let m = Manifest::parse(&result).unwrap();
            assert_eq!(m.tools.len(), if change == Change::Edit { 2 } else { 1 });
            assert!(result.contains("# PRIVATE_COMMENT"));
            assert!(result.contains("id='other'"));
            if change == Change::Edit {
                assert_eq!(m.tools[0].binary, "new-demo");
                assert!(f.preview(change).no_op);
                assert!(f.execute(change, |_| Ok(())).unwrap().is_none());
            }
            let history = journal::diagnose(&f.home.host).unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].journal_schema, Some(6));
            f.undo(|_| Ok(())).unwrap();
            assert_eq!(
                fs::read_to_string(f.repo.join("dort.toml")).unwrap(),
                original
            );
        }
    }
}
#[test]
fn references_unknown_ids_stale_and_modified_previews_refuse_without_mutation() {
    for reference in ["requires=['demo']", "runtime_tool='demo'", "file"] {
        let f = Fixture::new();
        let text = if reference == "file" {
            format!(
                "{ORIGINAL}\n[[files]]\nid='config'\nsource='config'\ntarget='~/.config-fixture'\nmode='link'\nrequires=['demo']\n"
            )
        } else if reference.starts_with("requires") {
            ORIGINAL.replace("id='other'", "id='other'\nrequires=['demo']")
        } else {
            ORIGINAL.replace(
                "provider='brew'\npackage='other'",
                "provider='npm'\npackage='other'\nruntime_tool='demo'",
            )
        };
        fs::write(f.repo.join("dort.toml"), &text).unwrap();
        assert!(declaration::preview_remove(&f.home.host, &f.repo, &[], "demo").is_err());
        assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
        assert_eq!(fs::read_to_string(f.repo.join("dort.toml")).unwrap(), text);
    }
    for change in [Change::Edit, Change::Remove] {
        let f = Fixture::new();
        let mut p = f.preview(change);
        p.change = None;
        assert!(Approval::from_preview(p).is_err());
        let a = Approval::from_preview(f.preview(change)).unwrap();
        fs::write(
            f.repo.join("dort.toml"),
            format!("{ORIGINAL}\n# later edit\n"),
        )
        .unwrap();
        assert!(
            declaration::execute(&f.home.host, f.lock.as_ref().unwrap(), a, |_| Ok(())).is_err()
        );
        assert!(journal::diagnose(&f.home.host).unwrap().is_empty());
        assert!(declaration::preview_remove(&f.home.host, &f.repo, &[], "missing").is_err());
        let mut invalid = edited_tool();
        invalid.requires = vec!["missing".into()];
        assert!(declaration::preview_edit(&f.home.host, &f.repo, &[], invalid).is_err());
        let mut t = edited_tool();
        t.id = "missing".into();
        assert!(declaration::preview_edit(&f.home.host, &f.repo, &[], t).is_err());
    }
}
#[test]
fn every_schema6_forward_inverse_boundary_blocks_writers_and_recovers() {
    for change in [Change::Edit, Change::Remove] {
        for inverse in [false, true] {
            for stop in 1..=boundaries(change, inverse) {
                let f = Fixture::new();
                if inverse {
                    f.execute(change, |_| Ok(())).unwrap();
                }
                let mut n = 0;
                let cb = |_: &Event| {
                    n += 1;
                    if n == stop {
                        Err(Error::Invalid("fixture failure".into()))
                    } else {
                        Ok(())
                    }
                };
                let result = if inverse {
                    f.undo(cb).map(|_| None)
                } else {
                    f.execute(change, cb)
                };
                assert!(result.is_err());
                if journal::diagnose(&f.home.host).unwrap()[0].blocks_writes() {
                    assert!(
                        RepositoryRegistry::register(
                            &f.home.host,
                            f.lock.as_ref().unwrap(),
                            &f.repo
                        )
                        .is_err()
                    );
                }
                f.undo(|_| Ok(()))
                    .unwrap_or_else(|e| panic!("{change:?} inverse={inverse} stop={stop}: {e}"));
                f.restored();
            }
        }
    }
}
#[test]
fn schema6_later_edits_and_corrupt_backup_or_tail_refuse_recovery() {
    for change in [Change::Edit, Change::Remove] {
        for mode in 0..3 {
            let f = Fixture::new();
            let tx = f.execute(change, |_| Ok(())).unwrap().unwrap();
            let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
                .unwrap()
                .join("transactions")
                .join(tx);
            match mode {
                0 => fs::write(f.repo.join("dort.toml"), "schema=1\n# later edit\n").unwrap(),
                1 => fs::write(dir.join("manifest.backup"), "corruption").unwrap(),
                _ => {
                    let n = journal::diagnose(&f.home.host).unwrap()[0].valid_checkpoints;
                    fs::write(dir.join(format!("{n:06}.json")), "{").unwrap();
                }
            }
            let live = fs::read(f.repo.join("dort.toml")).unwrap();
            assert!(f.undo(|_| Ok(())).is_err());
            assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), live);
            if mode != 0 {
                assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
            }
        }
    }
}
#[test]
fn checksum_valid_schema_or_operation_substitution_cannot_authorize_recovery() {
    use sha2::{Digest, Sha256};
    for mode in 0..4 {
        let f = Fixture::new();
        if mode == 3 {
            let mut t = edited_tool();
            t.id = "extra".into();
            let p = declaration::preview(&f.home.host, &f.repo, &[], t).unwrap();
            declaration::execute(
                &f.home.host,
                f.lock.as_ref().unwrap(),
                Approval::from_preview(p).unwrap(),
                |_| Ok(()),
            )
            .unwrap();
        } else {
            f.execute(Change::Remove, |_| Ok(())).unwrap();
        }
        let d = journal::diagnose(&f.home.host).unwrap().remove(0);
        let dir = dort_core::local::repository_state_path(&f.home.host, &f.id)
            .unwrap()
            .join("transactions")
            .join(d.transaction_id);
        let live = fs::read(f.repo.join("dort.toml")).unwrap();
        let mut previous = serde_json::Value::Null;
        for i in 0..d.valid_checkpoints {
            let path = dir.join(format!("{i:06}.json"));
            let mut e: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let mut r: serde_json::Value =
                serde_json::from_str(e["payload"].as_str().unwrap()).unwrap();
            match mode {
                0 => r["schema"] = 5.into(),
                1 => r["header"]["change"] = serde_json::Value::Null,
                2 => r["header"]["change"] = "edit".into(),
                _ => r["header"]["change"] = serde_json::Value::Null,
            }
            r["previous_sha256"] = previous;
            let payload = serde_json::to_string(&r).unwrap();
            let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
            e["payload"] = payload.into();
            e["sha256"] = hash.clone().into();
            fs::write(path, serde_json::to_vec(&e).unwrap()).unwrap();
            previous = hash.into();
        }
        assert!(f.undo(|_| Ok(())).is_err());
        assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), live);
        if mode != 2 {
            assert!(journal::diagnose(&f.home.host).unwrap()[0].blocks_writes());
        }
    }
}
#[test]
fn child_schema6_crash_probe() {
    let Some(home) = std::env::var_os("DORT_CHANGE_HOME") else {
        return;
    };
    let host = HostContext::new(
        home.into(),
        None,
        None,
        Platform::Linux,
        Architecture::X86_64,
    )
    .unwrap();
    let repo = PathBuf::from(std::env::var_os("DORT_CHANGE_REPO").unwrap());
    let lock = WriterLock::acquire(&host).unwrap();
    let change = if std::env::var("DORT_CHANGE_KIND").unwrap() == "edit" {
        Change::Edit
    } else {
        Change::Remove
    };
    let stop: usize = std::env::var("DORT_CHANGE_STOP").unwrap().parse().unwrap();
    let mut n = 0;
    let cb = |_: &Event| {
        n += 1;
        if n == stop {
            std::process::exit(77);
        }
        Ok(())
    };
    if let Ok(tx) = std::env::var("DORT_CHANGE_TX") {
        let p = recovery::preview(&host, &std::env::var("DORT_CHANGE_ID").unwrap(), &tx).unwrap();
        recovery::execute_with_observer(
            &host,
            &lock,
            recovery::Approval::from_preview(p).unwrap(),
            cb,
        )
        .unwrap();
    } else {
        declaration::execute(
            &host,
            &lock,
            Approval::from_preview(prepare(&host, &repo, change)).unwrap(),
            cb,
        )
        .unwrap();
    }
    panic!("boundary not reached");
}
#[test]
fn process_exit_at_every_schema6_forward_and_inverse_boundary_recovers() {
    for change in [Change::Edit, Change::Remove] {
        for inverse in [false, true] {
            for stop in 1..=boundaries(change, inverse) {
                let mut f = Fixture::new();
                let tx = if inverse {
                    f.execute(change, |_| Ok(())).unwrap().unwrap()
                } else {
                    String::new()
                };
                drop(f.lock.take());
                let mut cmd = Command::new(std::env::current_exe().unwrap());
                cmd.args(["--exact", "child_schema6_crash_probe", "--nocapture"])
                    .env_clear()
                    .env("PATH", "/no-external-programs")
                    .env("HOME", &f.home.host.home)
                    .env("DORT_CHANGE_HOME", &f.home.host.home)
                    .env("DORT_CHANGE_REPO", &f.repo)
                    .env("DORT_CHANGE_STOP", stop.to_string())
                    .env(
                        "DORT_CHANGE_KIND",
                        if change == Change::Edit {
                            "edit"
                        } else {
                            "remove"
                        },
                    );
                if inverse {
                    cmd.env("DORT_CHANGE_TX", tx).env("DORT_CHANGE_ID", &f.id);
                }
                let out = cmd.output().unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(77),
                    "{change:?} inverse={inverse} stop={stop}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                f.lock = Some(WriterLock::acquire(&f.home.host).unwrap());
                f.undo(|_| Ok(())).unwrap();
                f.restored();
            }
        }
    }
}
