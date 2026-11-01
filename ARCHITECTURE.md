# Dort architecture

This document describes implemented behaviour, not a promise that the roadmap is complete. Maintain it alongside code changes; [AGENTS.md](AGENTS.md) defines the contribution workflow. End-user documentation starts at [docs/README.md](docs/README.md).

## Status and boundaries

Dort currently supports macOS and Ubuntu on x86_64/ARM64. It uses a Rust 2024 workspace with the pinned Rust 1.99.1 toolchain.

| Component | Responsibility | Current boundary |
| --- | --- | --- |
| `crates/dort-core` | Models, validation, profiles/graphs, local state, observation, discovery, planning, journals, experimental filesystem lifecycle/recovery | UI-independent; explicit host inputs; no package-manager execution. |
| `crates/dort` | Clap commands, environment/platform boundary, repository selection, human/JSON presentation, exit codes | All CLI commands are read-only; no write approval/replay interface. |
| `examples/` | Tracked manifest examples | Validation examples, not an installed environment or a deployable source tree. |
| `docs/` | Tracked end-user guides | Document shipped commands, not internal implementation plans. |

Public write commands, tool installation/inventory, templates, secrets, cloning/sync and the TUI remain unimplemented. Core adoption/apply/unmanage executors are sandbox-tested prototypes, not production-ready general deployment.

## Module map

All module paths below are under `crates/dort-core/src/`:

| Modules | Responsibilities |
| --- | --- |
| `model.rs`, `manifest.rs` | Strict schema-1 manifest models and structural/host validation; resource/dependency conflicts. |
| `profiles.rs`, `platform.rs` | Profile union/selection, supported host/platform detection, stable dependency inputs. |
| `context.rs` | Explicit HOME/XDG/platform/architecture and lexical destination protection. Environment reads are confined to `from_env`. |
| `local.rs`, `state.rs` | Private local TOML, repository IDs/registration/relocation, descriptor-relative state primitives, retained-inode advisory writer lock. |
| `observation.rs` | No-follow parent walks, terminal entry/identity snapshots, bounded checked reads, safe hashes, terminal-link classification. |
| `discovery.rs` | Bounded metadata discovery, path exclusions and candidate reporting. No candidate-content reads. |
| `planner.rs` | Explicit observation into snapshots, then pure deterministic lifecycle planning. Unsupported effects become blockers. |
| `journal.rs` | Private per-repository transaction storage, immutable checksum chains, integrity/state validation, read-only diagnosis and write blocking. |
| `adoption.rs` | Schema-1 opaque approval, fresh revalidation, private staging, one-file adoption and explicit guarded/restartable rollback; shared identity/approval helpers. |
| `lifecycle.rs`, `lifecycle_journal.rs` | Schema-2 apply/unmanage approval/execution/rollback, immutable transaction store/validation and retained history. Sources are watches, never operation targets. |
| `error.rs` | Shared errors; callers decide presentation and exit status. |

CLI routing and reports live in [crates/dort/src/cli.rs](crates/dort/src/cli.rs). Domain services do not depend on Clap, terminal rendering or CLI output.

## Read and plan flow

```text
CLI arguments + environment
    → HostContext + private LocalConfig (read-only)
    → explicit --repo or locally selected registered root
    → manifest validation + effective profiles
    → descriptor-safe observation → Snapshot
    → pure planner::plan(Snapshot) → Plan
    → human/JSON output; no execution
```

Manifest-only checks validate declarations and graphs without observing live sources or installed tools. File inventory and discovery are separate from the planner's safety decision.

Profiles are a deterministic union of built-in `base`, defaults, platform selection and requested/local profiles. A resource is selected when any of its profiles is active. Unknown names, unresolved/disabled dependencies, cycles and conflicting destinations are rejected.

A snapshot records the manifest, relevant local configuration metadata, source/destination entries, every existing parent identity, active profiles and the request. Regular-file hashes bind safe observed content; snapshots never include file bodies. The snapshot is not globally atomic and is never trusted as later write authorization.

`plan` performs no I/O, clock reads or random ID generation. Plans have stable file-ID ordering and explicit backup, permission, source/manifest/destination, destructive and acknowledgement effects. Correct managed links are no-ops. Templates, secrets and unverified tool prerequisites are blocked rather than treated as links.

## Filesystem boundary and threat model

Lexical validation alone grants no access. Observers walk absolute paths using retained directory descriptors and no-follow opens, then inspect terminal entries without following symlinks. Checked reads verify open/named identity and metadata before/after reading, with a 16 MiB limit. Terminal link classification never grants permission to read a foreign referent's content.

Regular-file adoption requires an owned, single-link, non-sensitive file without special permission bits. Unsafe parents, special files, source escapes and protected destination overlaps fail closed. Non-UTF-8 report paths/link targets fail explicitly rather than being lossily identified.

Discovery inspects hidden HOME entries and up to three levels below `.config`, within a 4,096-name budget. It avoids known credentials, keys, caches, browser data and generated content; candidates are metadata, not an assurance of adoptability. Sensitivity heuristics can miss secrets or reject harmless files.

The advisory lock coordinates cooperating Dort writers only. No-follow descriptors and revalidation are not filesystem-wide isolation or protection against a malicious same-user/root process. Directory churn is tolerated only when stable identity/ownership/permissions remain unchanged. No real-home destructive tests or automatic Git merges into live-linked sources are permitted.

## State and repository identity

Both supported operating systems use:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/dort/local.toml
${XDG_STATE_HOME:-$HOME/.local/state}/dort/
    lock
    repositories/<repository-id>/
        transactions/<transaction-id>/
```

`LocalConfig` holds active profiles, variables, the selected repository ID and registered canonical roots. Variable values deliberately have no Debug representation and are not embedded in snapshots/journals. Configuration is machine-local, not part of the shared manifest.

Repository IDs are initially SHA-256 of the canonical root path; explicit relocation preserves the ID. They identify local state, not a Git revision or portable repository identity. Registration requires an existing Git marker and valid manifest; the CLI does not expose registration yet.

Private directory/file access is descriptor-relative. Configuration/checkpoints/backups use private owned single-link regular files (0600), and transaction directories use 0700. Prepared objects/retained originals can retain live modes inside private directories. Configuration replacement stages, syncs, renames and syncs its directory. The application lock retains its inode and is never unlinked to release ownership.

Incomplete or malformed transaction history blocks registration/relocation and all new filesystem transactions. Read-only commands do not create missing state or acquire writer locks.

## Core adoption flow

```text
privately reviewed Plan → explicit Approval::from_plan
    → application lock + registered repository
    → independent fresh observation / approved-plan digest comparison
    → preparing checkpoint + verified backups / private prepared objects
    → running: intent → atomic mutation → observed postcondition checkpoint
    → verifying → completed
failure/interruption
    → retained durable intent → read-only diagnosis
    → explicit guarded rollback only (never automatic replay)
```

Approval cannot be deserialized from preview JSON. It binds host context, mapping/effects, manifest/configuration and file inputs. Parent fingerprints are normalized only for unrelated directory timestamps/link counts; stable identity/ownership/permissions remain bound.

One transaction publishes missing source-parent directories (0700), copies the regular file, appends/revalidates the mapping without rewriting existing manifest text/comments, and replaces the destination with an absolute link. Source/manifest modes and groups are preserved; ACLs and extended attributes are not copied.

**State, repository and destination must share one filesystem supporting atomic no-replace/exchange renames.** There is no cross-device fallback. Each publication is atomic; the multi-path adoption is not globally atomic and readers can see intermediate states. The schema-2 lifecycle executor below uses the same constraint, including watched sources.

## Core link/unmanage flow

`lifecycle::Approval::from_plan` accepts only unmodified, unblocked apply/keep plans. Remove requires the distinct `from_removal_plan` API and a `RemovalAcknowledgement::confirm` matching its mapping ID. These opaque types cannot be deserialized. The caller must present effects and obtain approval; the core does not prompt. Execution requires the matching lock, registered repository, clean journal history and a fresh independent plan-digest match.

- **Apply:** one approved request becomes one recoverable transaction for all changed mappings, ordered by ID after deduplicated missing destination-parent operations (0700, parents before children). Correct links are untouched. Entirely correct/empty apply returns `None` without transaction storage or callbacks.
- **Keep:** exchange the managed link for a regular copy preserving source bytes/mode/group, then exchange the manifest with a focused mapping removal. `toml_edit` preserves unrelated declarations/comments; full parse, host validation and semantic comparison verify the edit. Array-of-tables and inline file arrays are supported.
- **Remove:** withdraw the correct managed link into private storage with a no-replace rename, then remove its manifest mapping. No source is deleted.
- **Guards:** all selected source entries remain immutable watches, including no-op mappings and resolved platform overrides. Manifest/configuration, destination, parent, stage and backup guards are rechecked around mutations/inverses. Any affected guard mismatch blocks the whole rollback preflight; unrelated edits outside created directories are retained.

The prototype caps a request at 128 selected mappings and 256 operations. Checked reads/checkpoints are at most 16 MiB; bounded history scans and no garbage collector remain limitations. Individual renames are atomic, not the request as a whole; failures retain durable intent and never auto-undo.

## Journal schema 1 and recovery contracts

Each transaction uses a unique 64-hex ID under its stable local repository ID. Durable metadata includes:

- Exclusively created immutable `000000.json`, `000001.json`, … checkpoints.
- An envelope containing the exact JSON `payload` string and its SHA-256 `sha256`.
- Payload schema/sequence/previous-checksum binding; repository/root/transaction IDs; approved-plan digest; mapping, profiles and input/parent fingerprints; backup hashes; ordered operation IDs, progress and observed postconditions.
- Private, synced, hash-verified `manifest.backup` and `destination.backup` byte copies.
- `op-0000`, … prepared objects or retained originals. Source-parent operations precede source publication, manifest exchange and destination exchange.

## Journal schema 2

Schema 2 is a separate wire format, not a reinterpretation/migration of schema-1 adoption records. Both live under the same repository/transaction layout and use the same immutable checksummed envelope/chain convention. Its header binds repository/root/transaction IDs, approved-plan digest, action (`apply`, `keep`, `remove`), separate removal acknowledgement, profiles, original manifest/configuration, ordered mapping/source/destination watches, parent anchors and verified backup specifications.

Operations bind contiguous IDs, roles (`directory`, `destination`, `manifest`), mutation (`publish`, `exchange`, `withdraw`), preconditions, prepared entries, progress and observed postconditions. Scope/order is reconstructed from the header and independently bound to the original validated manifest and host protections before recovery. Source paths cannot become mutation targets.

`backup-0000`, … stores byte copies of replaced regular files/manifests or exact raw symlink target text, with bound hashes and 0600 private access. `op-0000`, … stores prepared objects/retained originals. Publish uses no-replace; exchange retains the original; withdraw moves the link into an initially absent private slot with no-replace (its inverse republishes it). Both directories are synced. Directory operations precede apply links; unmanage changes the destination before removing the mapping.

The reader dispatches by the first checkpoint's schema hint, then independently verifies the entire chain, immutable operation identities, format-specific scope/order/progress, backup bindings and known artifacts. The hint is never authorization. Unsupported versions and corrupted/torn tails fail closed, even with a valid earlier prefix. Schema-1 reading and `adoption::rollback` stay supported; `lifecycle::rollback` handles schema 2. No format is rewritten on read.

## Shared durability and recovery

Transaction phases:

```text
preparing → running → verifying → completed
                 ↘ explicit rolling_back → rolled_back
completed ────────────────↗
```

Preparing can only change private transaction storage. Running operations progress through `pending → intent → done`; explicit inverses use `undo_intent → undone`. Intent must be synced before mutation/inverse. Atomic no-replace renames publish new objects; exchanges retain original manifest/destination in the private transaction directory. Both affected directories are synced before recording postconditions.

A torn final checkpoint never overwrites an earlier record. Gaps, malformed/truncated records, unsupported schemas, bad checksum chains, invalid transitions/bindings and damaged backups fail closed. Diagnosis may report the valid prefix but cannot silently discard an invalid tail. Checksums detect corruption, not malicious-owner forgery. Checkpoints contain metadata/hashes, never file contents or local variable values. Backups are not encrypted; secret deployment is unsupported.

Rollback requires an explicit transaction ID and matching application lock. It preflights all affected paths, parents, prepared objects and linked-source content, then rechecks around inverses. Recognizable interrupted atomic renames can be resolved from retained identity/content; uncertain states are refused. Source edits, substituted files/parents, extra entries in created directories and corrupt backups stop recovery without overwriting user work. Rollback itself is checkpointed and restartable. Completed transactions can also be explicitly rolled back if their guards still match. Schema-2 inverses handle publication/exchange/withdrawal without writing repository sources. A later transaction or editor can invalidate an older transaction's guards; history does not promise arbitrary undo order.

`lifecycle::history`/`journal::diagnose` expose content-free retained history across both schemas. `Diagnosis.journal_schema` reports the last fully validated record's schema (or null), not an unverified dispatch hint. `recovery list` presents this metadata without acquiring a writer lock or reading live source content. Diagnosis checks journal integrity, not current live drift; completed history does not establish that rollback is still safe.

No automatic recovery, garbage collection or transaction-history migration is implemented. Retained originals, backups and copied sources are not deleted after rollback. Malformed histories require manual diagnosis/repair rather than speculative execution.

## Presentation, validation and known limits

The CLI emits reports on stdout and errors on stderr, with optional JSON. Exit codes are 0 success, 1 I/O, 2 invalid input/configuration/host, 3 lock contention, and 4 a blocked plan or blocking recovery diagnosis. No file contents should enter logs or reports.

Integration tests use disposable canonicalized homes and isolated child environments. They cover pure planning, no-write CLI behaviour, unsafe paths/races, malformed state, approvals, guarded rollback, every exposed checkpoint/post-mutation error boundary, forward/inverse lifecycle process exits/lock release, no-journal repeated apply, batch/shared-parent execution, source preservation, separate removal acknowledgement and schema-1 compatibility. These tests do **not** establish real power-loss or filesystem fault-injection guarantees.

CI is configured for Ubuntu and macOS on both architectures; local test success is not evidence every remote runner passed. See [README.md](README.md#development) for current validation and [AGENTS.md](AGENTS.md) for required checks. Maintain this document when these boundaries, flows, persistence formats or guarantees change.
