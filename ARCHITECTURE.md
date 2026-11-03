# Dort architecture

This document describes implemented behaviour and durable technical contracts, not future feature specifications or implementation plans. Maintain it alongside code changes; [AGENTS.md](AGENTS.md) defines the contribution workflow. End-user documentation starts at [docs/user/README.md](docs/user/README.md); [docs/README.md](docs/README.md) indexes maintained references.

## Status and boundaries

Dort currently supports macOS and Ubuntu on x86_64/ARM64. It uses a Rust 2024 workspace with the pinned Rust 1.99.1 toolchain.

| Component | Responsibility | Current boundary |
| --- | --- | --- |
| `crates/dort-core` | Models, validation, profiles/graphs, local state, observation, discovery, planning, journals, experimental filesystem lifecycle/recovery | UI-independent; explicit host inputs; no package-manager execution. |
| `crates/dort` | Clap commands, Ratatui interface, environment/platform boundary, repository selection, presentation, exit codes | Shared core services and opaque approvals; terminal/flag approval for local bootstrap, file lifecycle and guarded recovery. |
| `examples/` | Tracked manifest examples | Validation examples, not an installed environment or a deployable source tree. |
| `docs/user/` | Tracked end-user guides | Current usage, commands, manifests, safety and troubleshooting only; no development content. |
| `docs/` | Maintained technical references and documentation index | Current system/contracts/methods, not transient feature specifications or implementation plans; no dependencies/links to ignored notes. |

Tool installation/inventory, templates, secrets, Git staging/commit/push and cloning/sync remain unimplemented. The TUI exposes session-only repository selection, guided local create/attach, active Files/apply/unmanage and Recovery; discovery/adoption and Git inspection remain CLI-only. Core adoption/apply/unmanage executors are sandbox-tested prototypes, not production-ready general deployment.

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
| `git.rs` | Fixed read-only Git executable adapter, conservative configuration preflight, bounded nonblocking subprocess runner, NUL status/raw-diff parsers and explicit escaped patch disclosure. No state/journal interaction. |
| `bootstrap.rs` | Opaque init/attach previews, schema-3 staged bootstrap chains, guarded resume/abandon, verified private configuration backups and registration publication. |
| `recovery.rs` | Read-only schema-1/2 inverse-path previews, opaque tamper-checked approvals bound to the journal head, fresh guard checks and rollback dispatch. |
| `error.rs` | Shared errors; callers decide presentation and exit status. |

CLI routing and reports live in [crates/dort/src/cli.rs](crates/dort/src/cli.rs). [services.rs](crates/dort/src/services.rs) shares relative-root resolution, actual bootstrap previews bound to their Git executable, opaque approvals and locked bootstrap/file dispatch between CLI/TUI. [environment.rs](crates/dort/src/environment.rs) discovers the launch directory and trusted Git executable at the application boundary. [tui/](crates/dort/src/tui/) contains pure application/key state (`app.rs`), session-local focus/actions/retained form drafts (`shell.rs`), service actions (`actions.rs`), worker ownership/progress (`events.rs`) and Ratatui rendering (`views.rs`). Domain services do not depend on Clap, Ratatui, the TUI worker or CLI output.

## Read and plan flow

```text
CLI arguments / TUI action + environment
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

## Read-only Git boundary

Application-boundary environment discovery resolves a trusted absolute Git executable from absolute PATH entries (empty/relative entries are ignored). Core `git::Git` receives that path explicitly and exposes only `status` and `diff(staged, patch)`, never arbitrary arguments/revisions or mutations. An explicit repository needs no manifest/registration but must equal Git's canonical non-bare toplevel; linked worktrees and unborn repositories are supported. Subdirectories are not silently searched upward.

Every child clears its environment and sets fixed locale, disabled global/system configuration, system attributes, optional locks, prompts, lazy fetch and replace objects. Fixed Git options disable pagers, hooks, fsmonitor, untracked-cache updates, index auto-refresh, external diff/textconv and submodule recursion/inspection. A safe builtin configuration query honors repository includes and refuses **any configured filter driver or partial-clone/promisor declaration**, including unused/false settings, before status/diff. No shell, install, fallback, network or public arbitrary Git mutation API exists. Bootstrap alone can call a narrow crate-private initializer inside its new private stage, reusing the same bounded/environment-cleared runner.

The private runner concurrently drains nonblocking stdout/stderr, caps their combined bytes at 4 MiB per child and enforces a 10-second deadline. On errors it kills the isolated process group and reaps the child; reader threads cannot hang on descendant-held pipes. Stderr is discarded, never echoed. Failures emit no partial report/patch. Limits bound captured data/time, not the subprocess's OS resource usage; this is not an OS sandbox.

Status parses porcelain-v2 NUL records, preserving rename originals and embedded whitespace. Diff parses raw NUL records with rename detection disabled; summaries contain change codes/modes/paths only. Non-UTF-8 metadata/paths fail explicitly, not lossily. Human metadata is quoted; Git JSON is ASCII-escaped while preserving decoded Unicode identity. The only file-content-report exception is **explicit `--patch`**: a separate bounded read returns a display-only patch with all control/non-ASCII bytes and backslashes escaped (LF preserved), never in errors/logs/journals. Binary payloads are not requested; untracked contents are excluded. Patches can expose secrets; no heuristic promises otherwise.

Git reads configuration/includes/attributes/object paths through its own filesystem code, not Dort's no-follow observer. Preflight and reads are not globally atomic; the configuration is rechecked before a separate patch read, but a malicious same-user process racing configuration can defeat preflight. Trust the Git installation/repository and do not treat hardening as isolation. No live-linked source, index, configuration, registration or transaction state is intentionally modified, and no app writer lock/state creation is needed.

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

Repository IDs are initially SHA-256 of the canonical root path; explicit relocation preserves the ID. They identify local state, not a Git revision or portable repository identity. The legacy core registry requires an existing Git marker and valid manifest. Public `init PATH` uses the separate bootstrap protocol and validates real non-bare Git roots before attachment; arbitrary existing/empty directories are not initialization targets.

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

## Journal schema 3: staged bootstrap

`bootstrap::preview` receives explicit host/root/Git inputs and creates no state. It accepts only a missing root under an existing owned, non-group/world-writable no-follow parent, or an existing valid non-bare Git root with a host-valid manifest. HOME/root and Dort metadata overlaps are forbidden. Preview approval is opaque/tamper-checked and independently re-observed under the lock.

A separate schema-3 payload uses the existing immutable checksummed envelope and repository/transaction layout, without migrating schemas 1/2. It binds the initial destination/manifest/Git marker/configuration metadata, parent identities, configuration hashes, intended registration hash, local repository ID and state path. Configuration values never enter checkpoints. Preparing creates only private artifacts/configuration directories. Git initializes only the newly allocated private 0700 `repository` stage with `--template=`, fixed `main`/SHA-1, no global/system/inherited configuration, hooks or network, using the bounded runner. The minimal manifest is `schema = 1`. Git's own private-stage filesystem writes are not Dort descriptor-relative operations.

The staged tree is no-follow inspected, synced and bounded to 256 regular/directory entries. Entries bind identities/content; symlinks, hard links, special files and unsafe ownership/modes are refused. `config.backup` holds a verified private copy of an existing configuration; `config.new` holds prepared registration bytes and retains the original inode after an exchange. Configuration values remain private, unencrypted backup data. The prepared checkpoint binds tree/configuration/target-parent guards before publication.

Bootstrap steps are `preparing → publish_intent → published → register_intent → completed`, with explicit `abandoned` from any nonterminal step. Diagnosis maps intermediate steps to running and abandoned to rolled_back, reporting journal schema 3. No-replace repository publication precedes registration publication (no-replace if missing, exchange otherwise); both affected directories are synced. All three locations must share a filesystem; no cross-device fallback exists. Attachment follows the same registration protocol without repository mutation.

Explicit bootstrap recovery previews inspect guarded live/private states without creating state or taking a lock. Opaque approvals bind action, effects and journal head; execution revalidates under the lock. Resume recognizes interrupted renames only by exact retained identities/hashes and parent guards. Preparing can only be abandoned/retried. Abandon appends a terminal record and **never deletes a published repository, undoes registration or removes artifacts**. Edits, substitutions, malformed chains and uncertain layouts refuse recovery. There is no automatic replay/deletion. Checksums/guards do not isolate a malicious same-user process.

## Shared durability and recovery

File lifecycle phases (schemas 1/2; bootstrap steps are separate):

```text
preparing → running → verifying → completed
                 ↘ explicit rolling_back → rolled_back
completed ────────────────↗
```

Preparing can only change private transaction storage. Running operations progress through `pending → intent → done`; explicit inverses use `undo_intent → undone`. Intent must be synced before mutation/inverse. Atomic no-replace renames publish new objects; exchanges retain original manifest/destination in the private transaction directory. Both affected directories are synced before recording postconditions.

A torn final checkpoint never overwrites an earlier record. Gaps, malformed/truncated records, unsupported schemas, bad checksum chains, invalid transitions/bindings and damaged backups fail closed. Diagnosis may report the valid prefix but cannot silently discard an invalid tail. Checksums detect corruption, not malicious-owner forgery. Checkpoints contain metadata/hashes, never file contents or local variable values. Backups are not encrypted; secret deployment is unsupported.

Rollback requires an explicit transaction ID and matching application lock. It preflights all affected paths, parents, prepared objects and linked-source content, then rechecks around inverses. Recognizable interrupted atomic renames can be resolved from retained identity/content; uncertain states are refused. Source edits, substituted files/parents, extra entries in created directories and corrupt backups stop recovery without overwriting user work. Rollback itself is checkpointed and restartable. Completed transactions can also be explicitly rolled back if their guards still match. Schema-2 inverses handle publication/exchange/withdrawal without writing repository sources. A later transaction or editor can invalidate an older transaction's guards; history does not promise arbitrary undo order.

`lifecycle::history`/`journal::diagnose` expose content-free retained history across all three schemas. `Diagnosis.journal_schema` reports the last fully validated record's schema (or null), not an unverified dispatch hint. `recovery list` presents this metadata without acquiring a writer lock or reading live source content. Diagnosis checks journal integrity, not current live drift; completed history does not establish that rollback is still safe.

The public rollback service first validates the schema-specific full scope/layout and reports conservative inverse paths without writes or a lock. Its opaque approval binds the exact report and immutable journal head; execution reopens/revalidates under the application lock before calling the existing guarded rollback. JSON preview documents cannot authorize recovery.

No automatic recovery, garbage collection or transaction-history migration is implemented. Retained originals, backups and copied sources are not deleted after rollback. Malformed histories require manual diagnosis/repair rather than speculative execution.

## Minimal TUI execution and terminal lifecycle

Terminal no-argument invocation and `tui` share one entry point, requiring terminal stdin/stdout/stderr and refusing JSON/automatic-approval flags. Host discovery remains at the application boundary; the TUI receives explicit host/repository/dry-run, launch-directory and resolved Git-or-unavailable inputs. Workers and pure input/render state do not discover environment values. Missing selection is a read-only Overview state, not a bootstrap trigger. All-repository recovery diagnosis is independent of the selected file repository. TUI startup never replays history, creates state or acquires a lock. Configuration failure does not suppress independent history diagnosis. Session selection reads known roots or canonicalizes an explicitly entered exact directory without saving configuration or inferring registration. Header/Overview distinguish selection origin, active profiles and registration. Only approved bootstrap writes registration; success selects that root in memory and returns to Overview. Relative create/attach inputs resolve from the supplied launch directory, without tilde/environment expansion; core no-follow checks remain authoritative.

The render/input loop polls Crossterm on one thread. At most one owned standard-library worker performs observations, preview creation or approved execution; no asynchronous runtime or detached job exists. A bounded, nonblocking metadata-only progress channel may coalesce/drop display updates, but never stops the executor if the display goes away. The UI keeps processing input and resizing during work. Approvals consume the actual in-memory core preview, never serialized JSON. Source/configuration/parent/journal-head revalidation and locking remain inside the existing core service path. `recovery::execute_with_observer` forwards callbacks through the same schema-1/2 guarded dispatch; the original `execute` remains compatible.

Searchable labeled Actions and Help restore the invoking destination/focus. Tab/Shift-Tab cycles focus; destination navigation uses Left/Right or numeric shortcuts. Path drafts are bounded, retained in process memory only, and accept printable shortcut letters as text. Esc returns from confirmation to preview, then to the retained form, then to its destination. Results preserve optional transaction IDs/no-op outcomes through refresh, and read errors are distinguished from failed writes. Refresh preserves selected file/transaction identity when present. Previews have vertical/horizontal scrolling. Enter opens a literal-yes confirmation; remove first collects a separate exact-ID acknowledgement. Esc cancels only before execution; dry-run/blocked previews and layouts below 48×14 cannot approve. A resize below that floor clears the entire confirmation, including removal acknowledgement; growing requires fresh acknowledgement. Session preview-only mode is switchable only outside previews/jobs; an entry `--dry-run` cannot be toggled off. q/Ctrl-C during a worker requests exit after it finishes. Once approved, writes are not cooperatively stopped or automatically rolled back. Terminal input/render errors restore the terminal and join the worker before returning; a main-thread unwind also joins it. Worker panics exit through cleanup, rather than continuing to render after Ratatui's panic hook restored the terminal. Forced process termination cannot promise cleanup and may leave an incomplete journal requiring explicit recovery.

A guard covers partial initialization, ordinary returns/errors and unwinding. Ratatui installs its panic-restoration hook. No external editor/shell suspension or background jobs are offered. Dynamic terminal metadata escapes control/non-ASCII characters, including bidi controls; no file bodies/local values/patches are rendered. Scrollable errors retain detailed diagnostics, while progress contains only transaction ID, boundary and sequence.

## Presentation, validation and known limits

The CLI emits reports on stdout and errors on stderr, with optional JSON. Explicit mutation commands present previews in human mode, then require terminal confirmation or --yes; JSON/nonterminal writes require --yes and never prompt. Remove mode additionally requires an exact-ID flag or separate human-terminal typed acknowledgement. --dry-run returns only a preview, never creates state/locks/artifacts and never prompts. Status, plan, history, checks, discovery and Git inspection remain read-only. Successful mutation JSON reports schema/operation/completed/nullable transaction_id; errors may leave durable partial effects, so inspect history after failures. Exit codes are 0 success, 1 I/O, 2 invalid input/configuration/host, 3 lock contention, and 4 a blocked plan or blocking recovery diagnosis. No file contents should enter logs or ordinary reports; only an explicitly requested escaped Git patch report may disclose bounded content.

Integration tests use disposable canonicalized homes and isolated child environments. They cover pure planning, no-write CLI behaviour, unsafe paths/races, malformed state, approvals, guarded rollback, every exposed checkpoint/post-mutation error boundary, forward/inverse lifecycle process exits/lock release, no-journal repeated apply, batch/shared-parent execution, source preservation, separate removal acknowledgement and schema-1 compatibility. CLI Git tests use deterministic fake executables and missing-Git cases, with no installed-tool requirement. Optional core real-Git fixtures skip explicitly when unavailable and cover no-write index/config/source identity, unusual paths, unborn/conflict/detached/worktree states, helper non-execution, filters/promisor refusal and explicit patch disclosure. Runner tests exercise stdout/stderr bounds, deadlines and descendant-held pipes. TUI tests cover small-layout and confirmation snapshots, cancellation/dry-run without state creation, exact-ID then yes approval, shared guarded create/attach/apply/keep/remove/rollback/bootstrap-abandon, retained drafts, session-only selection, focus/search, no-op apply, stale approval, lock contention, worker joining and responsive input. Local PTY checks additionally exercise no-argument/explicit entry, q/Ctrl-C, small-screen exit, retained text input/missing-Git refusal, session-only selection, guided create/attach, cancellation, approved apply and terminal attributes/alternate-screen restoration after normal return, synthetic terminal error and panic; the child probe is test-only, not a production environment switch. These tests do **not** establish real power-loss or filesystem fault-injection guarantees.

Permission-denied/exclusive-create backup failures, unreadable-file observation and cross-device refusal have isolated fixtures. The standard-library Python PTY gate discovers fresh Cargo artifacts without requiring installed Git. A separately approved, opt-in Linux storage launcher/probe uses a bounded 128 MiB ext4 volume to exercise real ENOSPC and nonempty partial checkpoint writes, preserving earlier checkpoint hashes and verifying explicit preparation recovery or malformed-tail refusal. This is test infrastructure, not a production fault switch or ordinary CI step. See [validation methods](docs/validation.md) for fixture requirements, harness operation and limits; it does not establish every boundary, inverse/new-repository ENOSPC, failing fsync or power loss.

CI is configured for Ubuntu and macOS on both architectures; local success or an earlier revision's run is not evidence that a changed candidate passed every remote runner. See [README.md](README.md#development) and [AGENTS.md](AGENTS.md) for required checks. Keep transient candidate/run evidence and feature/release worklists out of this maintained architecture reference.
