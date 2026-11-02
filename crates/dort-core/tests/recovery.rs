mod support;
use dort_core::{
    Error, adoption, lifecycle,
    local::RepositoryRegistry,
    planner::{self, Request},
    recovery,
    state::WriterLock,
};
use std::{fs, os::unix::fs::MetadataExt};

#[test]
fn rollback_previews_are_readonly_tamper_checked_and_head_bound() {
    let t = support::TestHome::new();
    let root = t.repository("repo");
    fs::write(root.join("source"), "ordinary settings\n").unwrap();
    fs::write(
        root.join("dort.toml"),
        "schema=1\n[[files]]\nid='a'\nsource='source'\ntarget='~/.a'\nmode='link'\n",
    )
    .unwrap();
    fs::write(t.host.home.join(".a"), "original\n").unwrap();
    let original = fs::metadata(t.host.home.join(".a")).unwrap().ino();
    let lock = WriterLock::acquire(&t.host).unwrap();
    let id = RepositoryRegistry::register(&t.host, &lock, &root).unwrap();
    let plan = planner::plan(planner::observe(&t.host, &root, &[], Request::Apply).unwrap());
    let tid = lifecycle::execute(
        &t.host,
        &lock,
        lifecycle::Approval::from_plan(plan).unwrap(),
        |_| Ok(()),
    )
    .unwrap()
    .unwrap();
    let tx = t
        .host
        .state_dir
        .join("repositories")
        .join(&id)
        .join("transactions")
        .join(&tid);
    let count = fs::read_dir(&tx).unwrap().count();
    let mut p = recovery::preview(&t.host, &id, &tid).unwrap();
    assert!(!p.effects.is_empty());
    assert_eq!(p.journal_schema, 2);
    assert_eq!(count, fs::read_dir(&tx).unwrap().count());
    p.effects.clear();
    assert!(recovery::Approval::from_preview(p).is_err());
    let a =
        recovery::Approval::from_preview(recovery::preview(&t.host, &id, &tid).unwrap()).unwrap();
    // Change only the journal head by interrupting an otherwise safe rollback.
    let interrupted =
        recovery::Approval::from_preview(recovery::preview(&t.host, &id, &tid).unwrap()).unwrap();
    let mut events = 0;
    recovery::execute_with_observer(&t.host, &lock, interrupted, |_| {
        events += 1;
        Err(Error::Invalid("stop".into()))
    })
    .unwrap_err();
    assert_eq!(events, 1);
    assert!(recovery::execute(&t.host, &lock, a).is_err());
    let a =
        recovery::Approval::from_preview(recovery::preview(&t.host, &id, &tid).unwrap()).unwrap();
    recovery::execute(&t.host, &lock, a).unwrap();
    assert_eq!(
        original,
        fs::metadata(t.host.home.join(".a")).unwrap().ino()
    );
    assert!(
        recovery::preview(&t.host, &id, &tid)
            .unwrap()
            .effects
            .is_empty()
    );
}
#[test]
fn schema_one_preview_revalidates_source_edits_without_mutations() {
    let t = support::TestHome::new();
    let root = t.repository("repo");
    let dest = t.host.home.join(".a");
    fs::write(&dest, "ordinary settings\n").unwrap();
    let lock = WriterLock::acquire(&t.host).unwrap();
    let id = RepositoryRegistry::register(&t.host, &lock, &root).unwrap();
    let mapping = dort_core::model::ManagedFile {
        id: "a".into(),
        source: "source".into(),
        target: "~/.a".into(),
        mode: dort_core::model::FileMode::Link,
        profiles: vec!["base".into()],
        requires: vec![],
        overrides: Default::default(),
        permissions: None,
    };
    let plan =
        planner::plan(planner::observe(&t.host, &root, &[], Request::Adopt { mapping }).unwrap());
    let tid = adoption::execute(
        &t.host,
        &lock,
        adoption::Approval::from_plan(plan).unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    let p = recovery::preview(&t.host, &id, &tid).unwrap();
    assert_eq!(p.journal_schema, 1);
    let a = recovery::Approval::from_preview(p).unwrap();
    fs::write(&dest, "edited via live link\n").unwrap();
    assert!(recovery::execute(&t.host, &lock, a).is_err());
    assert!(recovery::preview(&t.host, &id, &tid).is_err());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "edited via live link\n");
}
