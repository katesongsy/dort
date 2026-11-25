//! Exhaustive model check of the preview planner's safety invariants.
//!
//! `planner::plan` promises, in `ARCHITECTURE.md`, that a plan which replaces a
//! file accounts for the backup first, that a blocked plan carries no work, and
//! that `destructive` describes the effects it sits next to. Tests pin those for
//! a handful of fixtures. This file checks them for **every** combination of the
//! facts the planner reads.
//!
//! Why this is a complete check and not a sample: `plan` receives a `Snapshot`
//! of facts that `observe` already extracted from the filesystem. Every such
//! fact it branches on is finite — a `Kind`, a `FileMode`, two `Option`s, the
//! three ownership predicates inside a `Fingerprint`, two presence flags, three
//! `Path` comparisons, and four `Request` shapes. The domain below is the cross
//! product of those, so no input that can change the output is left out. The
//! comment on each domain value says which line of `plan` reads it.
//!
//! Values `plan` never reads are fixed, and named as such in `NEVER_READ`.
//!
//! A Kani harness over the same function was tried first and abandoned: CBMC
//! does not converge on `plan` in minutes because of the std string and
//! collection machinery it reaches through `observation::sensitive_path` and
//! `format!`. See `.pi/skills/verify-planner-invariants/SKILL.md`.

use dort_core::{
    context::HostContext,
    model::{FileMode, ManagedFile},
    observation::{Entry, Fingerprint, Kind},
    planner::{self, Effect, FilePlan, Outcome, Request, Snapshot, UnmanageMode},
    platform::{Architecture, Platform},
};
use std::{collections::BTreeSet, path::PathBuf};

/// `plan` compares file ownership against `Snapshot::user_id`.
const USER: u32 = 501;
const OTHER_USER: u32 = 999;
/// `observation::sensitive_path` returns false for this path.
const PLAIN_SOURCE: &str = "/repo/files/demorc";
/// `observation::sensitive_path` returns true for this path, because of `.ssh`.
const SENSITIVE_SOURCE: &str = "/repo/files/.ssh/id_rsa";
const TARGET: &str = "/home/user/.demorc";
const REPOSITORY: &str = "/repo";

/// Fields present on the structs above that `plan` never reads, so holding them
/// constant loses no case:
/// - `Entry::path`, `Entry::parents`
/// - `Entry::link_target` **on the source** (only the destination's is compared)
/// - `Snapshot::host`, `Snapshot::repository`, `Snapshot::active_profiles`
/// - `Snapshot::manifest` except `.path`, `Snapshot::local_config`
/// - `ManagedFile::{id, source, target, profiles, overrides, permissions}`
///   (they are copied into effects, never branched on)
/// - `FileSnapshot::target_path` (only copied into effects)
/// - `Snapshot::user_id`: only compared against `Fingerprint::uid`, which the
///   ownership domain below already varies both ways
///
/// One file per snapshot: `plan` handles each `FileSnapshot` with the same code,
/// so one file covers the per-file invariants for any count.
const NEVER_READ: &str = "see the comment above";

fn host() -> HostContext {
    HostContext {
        home: PathBuf::from("/home/user"),
        config_dir: PathBuf::from("/home/user/.config/dort"),
        state_dir: PathBuf::from("/home/user/.local/state/dort"),
        platform: Platform::Linux,
        architecture: Architecture::Aarch64,
    }
}

/// The four shapes `plan` distinguishes. `Directory` and `Special` fall into the
/// same `_` arm in `plan` and the same `kind != Regular` test in `regular`, so
/// they are one class — not a dropped case.
#[derive(Clone, Copy, Debug)]
enum KindClass {
    Missing,
    Regular,
    Symlink,
    Other,
}

const KIND_CLASSES: [KindClass; 4] = [
    KindClass::Missing,
    KindClass::Regular,
    KindClass::Symlink,
    KindClass::Other,
];

impl KindClass {
    fn of(self) -> Kind {
        match self {
            KindClass::Missing => Kind::Missing,
            KindClass::Regular => Kind::Regular,
            KindClass::Symlink => Kind::Symlink,
            KindClass::Other => Kind::Special,
        }
    }
}

/// The distinct combinations `regular` and the destination checks can see:
/// presence, then single-link, own-user, and no special mode bits.
#[derive(Clone, Copy, Debug)]
struct FingerprintClass {
    present: bool,
    single_link: bool,
    own_user: bool,
    plain_mode: bool,
}

fn fingerprint_classes() -> Vec<FingerprintClass> {
    let mut out = vec![FingerprintClass {
        present: false,
        single_link: false,
        own_user: false,
        plain_mode: false,
    }];
    for single_link in [true, false] {
        for own_user in [true, false] {
            for plain_mode in [true, false] {
                out.push(FingerprintClass {
                    present: true,
                    single_link,
                    own_user,
                    plain_mode,
                });
            }
        }
    }
    out
}

/// How the destination's link target compares with the source path. `plan` only
/// ever asks for equality, so "a different existing target" and "no target" are
/// the two shapes of not-equal.
#[derive(Clone, Copy, Debug)]
enum Pointer {
    Matches,
    Differs,
    Absent,
}

const POINTERS: [Pointer; 3] = [Pointer::Matches, Pointer::Differs, Pointer::Absent];

#[derive(Clone, Copy, Debug)]
struct EntryClass {
    kind: KindClass,
    sensitive: bool,
    fingerprint: FingerprintClass,
    has_sha256: bool,
    pointer: Pointer,
}

/// An entry with nothing but its kind set, for branches that read no more.
fn bare(kind: KindClass) -> EntryClass {
    EntryClass {
        kind,
        sensitive: false,
        fingerprint: FingerprintClass {
            present: false,
            single_link: false,
            own_user: false,
            plain_mode: false,
        },
        has_sha256: false,
        pointer: Pointer::Absent,
    }
}

fn fingerprinted(kinds: &[KindClass], pointers: &[Pointer]) -> Vec<EntryClass> {
    let mut out = vec![];
    for kind in kinds.iter().copied() {
        for sensitive in [false, true] {
            for fingerprint in fingerprint_classes() {
                for has_sha256 in [false, true] {
                    for pointer in pointers.iter().copied() {
                        out.push(EntryClass {
                            kind,
                            sensitive,
                            fingerprint,
                            has_sha256,
                            pointer,
                        });
                    }
                }
            }
        }
    }
    out
}

fn with_none(classes: Vec<EntryClass>) -> Vec<Option<EntryClass>> {
    std::iter::once(None)
        .chain(classes.into_iter().map(Some))
        .collect()
}

fn materialize(class: EntryClass, path: &str, source_path: &str) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind: class.kind.of(),
        fingerprint: if class.fingerprint.present {
            Some(Fingerprint {
                device: 1,
                inode: 2,
                links: if class.fingerprint.single_link { 1 } else { 2 },
                mode: if class.fingerprint.plain_mode {
                    0o100_644
                } else {
                    0o104_755
                },
                uid: if class.fingerprint.own_user {
                    USER
                } else {
                    OTHER_USER
                },
                gid: 20,
                size: 3,
                modified: (0, 0),
                changed: (0, 0),
            })
        } else {
            None
        },
        parents: Vec::new(),
        link_target: match class.pointer {
            Pointer::Matches => Some(PathBuf::from(source_path)),
            Pointer::Differs => Some(PathBuf::from("/home/user/elsewhere")),
            Pointer::Absent => None,
        },
        sha256: if class.has_sha256 {
            Some("deadbeef".to_string())
        } else {
            None
        },
        sensitive: class.sensitive,
    }
}

/// The four shapes of `Request`. `Adopt`'s mapping mode is enumerated by
/// `Case::mode`, so the request itself only needs the branch.
#[derive(Clone, Copy, Debug)]
enum RequestClass {
    Apply,
    Adopt,
    Unmanage(UnmanageMode),
}

const REQUESTS: [RequestClass; 4] = [
    RequestClass::Apply,
    RequestClass::Adopt,
    RequestClass::Unmanage(UnmanageMode::Keep),
    RequestClass::Unmanage(UnmanageMode::Remove),
];

fn request_of(class: RequestClass, mapping: ManagedFile) -> Request {
    match class {
        RequestClass::Apply => Request::Apply,
        RequestClass::Adopt => Request::Adopt { mapping },
        RequestClass::Unmanage(mode) => Request::Unmanage {
            id: mapping.id,
            mode,
        },
    }
}

#[derive(Clone, Copy, Debug)]
struct Case {
    request: RequestClass,
    mode: FileMode,
    requires: bool,
    sensitive_source_path: bool,
    initial_blocker: bool,
    source: Option<EntryClass>,
    destination: Option<EntryClass>,
}

impl Case {
    /// A counterexample a reviewer can act on, in the planner's own words.
    fn describe(&self) -> String {
        let source = match &self.source {
            None => "no source observation".to_string(),
            Some(class) => format!(
                "source kind={:?} sensitive={} fingerprint={:?} sha256={} link_target={:?}",
                class.kind, class.sensitive, class.fingerprint, class.has_sha256, class.pointer
            ),
        };
        let destination = match &self.destination {
            None => "no destination observation".to_string(),
            Some(class) => format!(
                "destination kind={:?} sensitive={} fingerprint={:?} sha256={} link_target={:?}",
                class.kind, class.sensitive, class.fingerprint, class.has_sha256, class.pointer
            ),
        };
        format!(
            "request={:?} mode={:?} requires_tool={} source_path={} initial_blocker={} | {} | {}",
            self.request,
            self.mode,
            self.requires,
            if self.sensitive_source_path {
                SENSITIVE_SOURCE
            } else {
                PLAIN_SOURCE
            },
            self.initial_blocker,
            source,
            destination
        )
    }

    fn snapshot(&self) -> Snapshot {
        let source_path = if self.sensitive_source_path {
            SENSITIVE_SOURCE
        } else {
            PLAIN_SOURCE
        };
        let mapping = ManagedFile {
            id: "demorc".into(),
            source: PathBuf::from("files/demorc"),
            target: PathBuf::from(TARGET),
            mode: self.mode,
            profiles: vec!["base".into()],
            requires: if self.requires {
                vec!["some-tool".into()]
            } else {
                vec![]
            },
            overrides: Default::default(),
            permissions: None,
        };
        Snapshot {
            user_id: USER,
            host: host(),
            repository: PathBuf::from(REPOSITORY),
            manifest: Entry {
                path: PathBuf::from("/repo/dort.toml"),
                kind: Kind::Regular,
                fingerprint: None,
                parents: Vec::new(),
                link_target: None,
                sha256: None,
                sensitive: false,
            },
            local_config: Entry {
                path: PathBuf::from("/home/user/.config/dort/local.toml"),
                kind: Kind::Regular,
                fingerprint: None,
                parents: Vec::new(),
                link_target: None,
                sha256: None,
                sensitive: false,
            },
            // Never read by `plan`; empty keeps the check cheap.
            temporary_profiles: vec![],
            profiles: vec![],
            inactive_links: vec![],
            active_profiles: BTreeSet::new(),
            request: request_of(self.request, mapping.clone()),
            files: vec![planner::FileSnapshot {
                mapping,
                source_path: PathBuf::from(source_path),
                target_path: PathBuf::from(TARGET),
                source: self
                    .source
                    .map(|class| materialize(class, source_path, source_path)),
                destination: self
                    .destination
                    .map(|class| materialize(class, TARGET, source_path)),
                blockers: if self.initial_blocker {
                    vec!["already blocked".into()]
                } else {
                    vec![]
                },
            }],
        }
    }
}

fn every_case() -> impl Iterator<Item = Case> {
    // Each request branch reads a different subset of the two observations, so
    // each gets its own domain. Enumerating a field the branch never reads adds
    // no case; the comment on each domain names the code that reads it.
    let mut cases = vec![];
    for request in REQUESTS {
        let (sources, destinations, modes): (
            Vec<Option<EntryClass>>,
            Vec<Option<EntryClass>>,
            &[FileMode],
        ) = match request {
            // `Adopt` reads `source.kind` only ("adoption source must not
            // exist"), and calls `regular` on the destination alone.
            RequestClass::Adopt => (
                std::iter::once(None)
                    .chain(KIND_CLASSES.map(|kind| Some(bare(kind))))
                    .collect(),
                with_none(fingerprinted(KIND_CLASSES.as_slice(), &[Pointer::Absent])),
                &[FileMode::Link, FileMode::Template, FileMode::Secret],
            ),
            // `Unmanage` calls `regular` on the source, and reads the
            // destination only as "is it the correct symlink".
            RequestClass::Unmanage(_) => (
                with_none(fingerprinted(KIND_CLASSES.as_slice(), &[Pointer::Absent])),
                with_none(fingerprinted(KIND_CLASSES.as_slice(), &POINTERS)),
                &[FileMode::Link, FileMode::Template],
            ),
            // `Apply` reads every field of both observations.
            RequestClass::Apply => (
                with_none(fingerprinted(KIND_CLASSES.as_slice(), &[Pointer::Absent])),
                with_none(fingerprinted(KIND_CLASSES.as_slice(), &POINTERS)),
                &[FileMode::Link, FileMode::Template],
            ),
        };
        for mode in modes.iter().copied() {
            for requires in [false, true] {
                for sensitive_source_path in [false, true] {
                    for initial_blocker in [false, true] {
                        for source in sources.iter().copied() {
                            for destination in destinations.iter().copied() {
                                cases.push(Case {
                                    request,
                                    mode,
                                    requires,
                                    sensitive_source_path,
                                    initial_blocker,
                                    source,
                                    destination,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    cases.into_iter()
}

/// The path an effect replaces or deletes: the only effects that can lose a
/// user's file.
fn destroyed_path(effect: &Effect) -> Option<&PathBuf> {
    match effect {
        Effect::Link { target, .. }
        | Effect::Materialize { target, .. }
        | Effect::RemoveDestination { target } => Some(target),
        _ => None,
    }
}

fn backed_up_before(effects: &[Effect], index: usize, path: &PathBuf) -> bool {
    effects[..index]
        .iter()
        .any(|effect| matches!(effect, Effect::Backup { path: earlier } if earlier == path))
}

/// Did anything exist at this path, according to the facts the plan was built
/// from? A `Link` onto a missing destination replaces nothing and needs no
/// backup, so this premise is what makes the backup rule precise.
fn existed(plan: &planner::Plan, path: &PathBuf) -> bool {
    plan.inputs
        .files
        .iter()
        .find(|file| &file.target_path == path)
        .and_then(|file| file.destination.as_ref())
        .is_some_and(|entry| entry.kind != Kind::Missing)
}

fn is_destination_path(plan: &planner::Plan, path: &PathBuf) -> bool {
    plan.inputs
        .files
        .iter()
        .any(|file| &file.target_path == path)
}

/// Does any effect replace or remove something that existed?
fn replaces_or_removes(plan: &planner::Plan, effects: &[Effect]) -> bool {
    effects.iter().any(|effect| match destroyed_path(effect) {
        Some(path) => existed(plan, path),
        None => false,
    })
}

fn assert_invariants(case: &Case, plan: &planner::Plan) {
    assert_eq!(
        plan.files.len(),
        1,
        "the planner dropped the only file it was given: {}",
        case.describe()
    );
    let file: &FilePlan = &plan.files[0];
    let context = format!("\ncase: {}\neffects: {:?}", case.describe(), file.effects);

    // 1. Nothing that existed is replaced or removed before a backup of the same
    //    path. A `Link` onto a missing destination replaces nothing, so the
    //    premise is "the destination existed", read from the plan's own inputs.
    for (index, effect) in file.effects.iter().enumerate() {
        if let Some(path) = destroyed_path(effect)
            && existed(plan, path)
        {
            assert!(
                backed_up_before(&file.effects, index, path),
                "destructive effect at {index} replaced or removed {} before a backup of it{context}",
                path.display()
            );
        }
    }

    // 2. A destination backup is only taken when there was a destination to save.
    for effect in &file.effects {
        if let Effect::Backup { path } = effect
            && is_destination_path(plan, path)
        {
            assert!(
                existed(plan, path),
                "a backup was taken of {}, where nothing existed{context}",
                path.display()
            );
        }
    }

    // 3. Exactly the files with blockers are blocked, so no outcome is silent.
    assert_eq!(
        matches!(file.outcome, Outcome::Blocked),
        !file.blockers.is_empty(),
        "a blocked plan must name a blocker and a named blocker must block{context}"
    );

    // 4. A blocked plan carries no work.
    if matches!(file.outcome, Outcome::Blocked) {
        assert!(
            file.effects.is_empty(),
            "blocked plan carried effects{context}"
        );
        assert!(
            !file.destructive,
            "blocked plan claimed a destructive write{context}"
        );
        assert!(
            !file.requires_removal_acknowledgement,
            "blocked plan demanded a removal acknowledgement{context}"
        );
    }

    // 5. A no-op plan carries no work.
    if matches!(file.outcome, Outcome::NoOp) {
        assert!(
            file.effects.is_empty(),
            "no-op plan carried effects{context}"
        );
        assert!(
            !file.destructive,
            "no-op plan claimed a destructive write{context}"
        );
    }

    // 6. A change plan has work to do and nothing blocking it.
    if matches!(file.outcome, Outcome::Change) {
        assert!(
            !file.effects.is_empty(),
            "change plan carried no effects{context}"
        );
        assert!(
            file.blockers.is_empty(),
            "change plan carried a blocker{context}"
        );
    }

    // 7. `destructive` describes the effects beside it: true exactly when some
    //    effect replaces or removes something that existed.
    assert_eq!(
        file.destructive,
        replaces_or_removes(plan, &file.effects),
        "the destructive flag must match the effects it describes{context}"
    );

    // 8. A removal acknowledgement is only ever asked for alongside a removal.
    if file.requires_removal_acknowledgement {
        assert!(
            file.effects
                .iter()
                .any(|effect| matches!(effect, Effect::RemoveDestination { .. })),
            "a removal acknowledgement must accompany a removal{context}"
        );
    }
}

#[test]
fn planner_invariants_hold_for_every_case() {
    let _ = NEVER_READ;
    let mut checked = 0usize;
    let mut changes = 0usize;
    let mut blocked = 0usize;
    let mut noops = 0usize;
    let mut destructive: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut removal_acks = 0usize;
    for case in every_case() {
        let plan = planner::plan(case.snapshot());
        assert_invariants(&case, &plan);
        let file = &plan.files[0];
        match file.outcome {
            Outcome::Change => changes += 1,
            Outcome::Blocked => blocked += 1,
            Outcome::NoOp => noops += 1,
        }
        if file.requires_removal_acknowledgement {
            removal_acks += 1;
        }
        if file.destructive {
            let branch = match case.request {
                RequestClass::Apply => "apply",
                RequestClass::Adopt => "adopt",
                RequestClass::Unmanage(_) => "unmanage",
            };
            *destructive.entry(branch).or_default() += 1;
        }
        checked += 1;
    }

    // An exhaustive check over a domain that never reaches a branch proves
    // nothing about it. Every branch must produce the interesting outcome.
    assert!(changes > 0, "no case produced a change plan");
    assert!(blocked > 0, "no case produced a blocked plan");
    assert!(noops > 0, "no case produced a no-op plan");
    assert!(
        removal_acks > 0,
        "no case asked for a removal acknowledgement"
    );
    for branch in ["apply", "adopt", "unmanage"] {
        assert!(
            destructive.get(branch).copied().unwrap_or(0) > 0,
            "no {branch} case produced a destructive plan, so the backup rule \
             was never checked on that branch"
        );
    }

    println!(
        "PASS planner invariants over {checked} cases \
         ({changes} change, {blocked} blocked, {noops} no-op; \
         destructive: apply={}, adopt={}, unmanage={}; {removal_acks} removal acks)",
        destructive["apply"], destructive["adopt"], destructive["unmanage"]
    );
}
