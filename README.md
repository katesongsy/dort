# Dort

**Your development environment, everywhere.**

Dort is a Git-native developer environment manager. It is designed to reconcile tools, dotfiles, profiles, templates, and encrypted configuration through a shared CLI/TUI engine—without replacing Git or existing package managers.

## Current status

**Experimental CLI and TUI.** Dort provides strict manifest validation, profile/dependency resolution, bounded discovery, descriptor-safe observation, pure previews, hardened read-only Git status/diff, local repository init/attach, and approved adoption, link apply, unmanage and guarded recovery with immutable journals.

A Ratatui interface provides Overview, session repository selection, guided local create/attach, temporary profiles, Files discovery/adoption, manifest checks, host Diagnostics/About, searchable all-repository Recovery, read-only Git inspection, previews, explicit approval and responsive progress over the same core services. Package-manager execution, remote cloning/sync, Git staging/commit/push, templates and secrets are **not implemented**. A successful preview is not permission or an executable artifact for a later write. The executor is tested only against disposable homes; this is not yet a production-ready dotfile manager.

## Documentation

- [User guide](docs/user/README.md): [getting started](docs/user/getting-started.md), [manifest guide](docs/user/manifest.md), [commands](docs/user/commands.md), and [safety/troubleshooting](docs/user/safety.md).
- [Documentation index](docs/README.md): end-user guides and maintained technical references.
- [Validation methods](docs/validation.md): test infrastructure, isolation and interpretation of results.
- [Architecture](ARCHITECTURE.md): module boundaries, data flow, journal/recovery contracts and limitations.
- [Contributor/agent instructions](AGENTS.md): development checks and documentation-maintenance rules.
- [Changelog](CHANGLOG.md): generated with git-cliff from committed Git history (uncommitted work is excluded).
- [MIT license](LICENSE).

## Build and run

Requires a supported host: macOS (Apple Silicon or Intel), or Ubuntu (ARM64 or x86_64). Rust **1.99.1** is the pinned toolchain and current minimum supported Rust version; `rustup` will use `rust-toolchain.toml`.

```bash
cargo build --workspace --locked
cargo run -p dort -- --help

# Read-only host/platform diagnostics; does not create local state
cargo run -p dort -- doctor
cargo run -p dort -- --json doctor

# Read-only transaction diagnosis; never replays or repairs a journal
cargo run -p dort -- recovery list
cargo run -p dort -- --json recovery list

# Validate the example without installing tools or touching live dotfiles
cargo run -p dort -- manifest check --manifest examples/dort.toml
cargo run -p dort -- --json manifest check --manifest examples/dort.toml --profile dev

# Check an existing environment repository explicitly
cargo run -p dort -- --repo /path/to/dotfiles manifest check
```

Without `--manifest` or `--repo`, repository commands use the active repository selected by `dort init PATH`. Discovery also works without a repository. Repeatable `--profile` affects only the current check/list/plan/file operation.

JSON reports go to stdout; JSON errors go to stderr. Exit codes are 0 for success, 1 for I/O failure, 2 for invalid input/configuration or unsupported hosts, 3 for a held writer lock, and **4 for a blocked plan or recovery diagnosis reporting write blockers**. Those reports are still emitted in full on stdout (not as a separate stderr error). Mutations require confirmation or `--yes`; JSON/nonterminal writes require `--yes` and never prompt. `--dry-run` never creates state or locks.

## Interactive interface

Start with the [TUI-only disposable-HOME walkthrough](docs/user/getting-started.md#start-in-a-disposable-home): create, discover/adopt, inspect, keep and review recovery without a CLI handoff.

Run `dort` (no arguments) or `dort tui` in a terminal. `--repo PATH` overrides the active repository; `--dry-run` disables approval without creating state. Nonterminal invocation, `--json`, `--yes` and `--confirm-remove` are rejected for TUI entry, never interpreted as automatic approval. Use explicit CLI commands for automation.

Overview shows repository registration, active file counts and global journal write blockers in summary cards, with full scope/profile/drift metadata in scrollable sections. Unavailable observations are unknown, not zero. Files supports searchable metadata inventories, adoption, full-scope apply and both unmanage modes; Recovery has a typed transaction table with schema, phase and integrity/write-block state, unfiltered global blockers, and full-ID details. Its page buttons offer eligible schema-1/2 rollback or schema-3 resume/abandon previews; Preparing offers Abandon, not Resume. Checks shows structural validity and selected file/tool counts separately from capability limits; read-only host/registry Diagnostics (`j`) has independent registry/history summaries. Actions includes About/version. The workbench uses highlighted tabs, state columns, focus borders, action buttons and bordered inputs with a caret. Press Space for searchable Actions, `?` for help, `1`/`2`/`3`/`4`/`5` to change views, and Tab/Shift-Tab to change focus. Left/Right selects within focused destination tabs or action buttons; Enter activates the selected button. `p` chooses a repository for the session, `n` opens create/attach, `a` previews apply, and `q` exits outside text entry. All five destinations remain labeled at 48 columns. PgUp/PgDn scroll reports; Home returns to the start (both axes in previews), and preview scrolling stays within escaped metadata bounds. File reviews show typed change counts and per-mapping effects; Up/Down selects a mapping and `i` toggles its complete report, including permissions and warnings. Review the effects, press Enter, then type literal `yes`; link deletion first requires its exact file ID in a separate acknowledgement. Esc dismisses previews/confirmations without writing. Approved writes finish before q/Ctrl-C exits; terminal errors wait for the worker, never detach it or auto-undo. Errors and panics restore the terminal where the OS still permits cleanup.

The interface requires 48×14 for approval. `f` discovers metadata without a repository; directories and other ineligible entries are labeled and cannot open a candidate adoption draft. Enter inspects a candidate. After inspection, Tab focuses the action buttons and Right selects Adopt selected; Enter opens editable fields for eligible regular files. `o` opens a manual draft. Eligibility is only a hint, not approval. `t` selects temporary repository profile additions; `c` checks its manifest and `v` checks a standalone manifest with independent additions. In Files, `/` filters display only; `i` inspects full details and Actions accepts exact mapping IDs outside the filter. Apply always covers the full active scope. Git (`5`) has an independent session root, typed Status/Unstaged/Staged metadata tables, search and refresh. Status separates path/kind/XY; diffs separate path/change/modes. Full root, branch and rename-path details remain scrollable, with Home returning to the start. `w` opens a content warning; only literal `view patch` requests one bounded escaped patch view. Esc, scope change, resize below the floor or exit drops its reference, without promising secure memory erasure. No patch prefetch, automatic refresh, history or export is provided. Choose a known root or enter an exact path without persisting selection. Create/attach retains its path draft; Enter advances from the path to a Review button, whose activation requests a preview. It resolves relative paths from the launch directory and uses the same guarded bootstrap as `init`; only explicit approval publishes registration. Without a selected repository, Welcome offers these actions and Recovery remains accessible. Shrinking below the approval floor clears typed confirmation and deletion acknowledgement. Error `r` retries a read/fresh preview, never an approval; after a failed write it only refreshes. Error `h` opens independent Recovery. New execution always needs a new preview and confirmation. No startup replay occurs. See [keys and behavior](docs/user/commands.md#interactive-interface) and [terminal safety](docs/user/safety.md#terminal-and-interruption).

## Read-only Git inspection

An installed, trusted Git executable must be available through an absolute PATH entry; Dort never installs Git. Initialization/attachment also needs Git; other commands do not.

```sh
dort --repo /path/to/dotfiles git status
dort --repo /path/to/dotfiles --json git status
dort --repo /path/to/dotfiles git diff
dort --repo /path/to/dotfiles git diff --staged
# Explicit CONTENT disclosure: may expose secrets; review privately.
dort --repo /path/to/dotfiles git diff --patch
```

Status and default diff reports contain metadata only. Diff compares tracked worktree changes to the index, or the index to HEAD with `--staged` (including unborn repositories). Untracked names appear in status; their contents are never included in diff. Diff summaries use change codes, modes and paths, without rename detection; status can report renames. Only explicit CLI `--patch` or the TUI's separately confirmed View patch content action discloses content, escaping control/non-ASCII bytes and backslashes; binary files get a change marker, not binary payloads. Patches are display reports, not directly applicable patch files, and are **not secret-proof**.

Git runs with a cleared environment, fixed safe arguments, no optional locks/index refresh, hooks, external diff/textconv, fsmonitor or submodule recursion. Configured content filters and partial-clone/promisor settings are conservatively refused, even when unused/false. Each child has a 10-second deadline and a combined 4 MiB stdout/stderr limit; errors never echo Git stderr or partial output. These inspection commands have no fallback, network, merge or write operation. This is hardening, **not OS sandboxing or protection from a malicious same-user configuration race**. Git reads repository-controlled configuration/includes/attributes/object paths using Git's own filesystem access, not Dort's no-follow observer.

`--repo` must identify the exact non-bare working-tree root (linked worktrees supported), with no manifest/registration requirement. Reports are not globally atomic; summary and explicit patch are separate reads. See [command details](docs/user/commands.md#git-inspection) and [Git safety/troubleshooting](docs/user/safety.md#git-inspection-boundary).

## File discovery and previews

```bash
# Metadata only: hidden files in HOME and .config, without following directory symlinks
cargo run -p dort -- files discover
cargo run -p dort -- --json files discover

# Inspect active mappings and preview link effects; --profile is optional
cargo run -p dort -- --repo /path/to/dotfiles files list
cargo run -p dort -- --repo /path/to/dotfiles files plan apply --profile dev
cargo run -p dort -- --repo /path/to/dotfiles --json files plan apply

# Preview ONLY: no source copy, manifest edit, link replacement or backup occurs
cargo run -p dort -- --repo /path/to/dotfiles files plan adopt '~/.zshrc' \
  --id zshrc --source files/common/zshrc

# Keep would materialize an ordinary file; remove would delete the managed link.
# Both would remove the mapping, retaining the repository source.
cargo run -p dort -- --repo /path/to/dotfiles files plan unmanage zshrc --mode keep
cargo run -p dort -- --repo /path/to/dotfiles files plan unmanage zshrc --mode remove
```

Discovery is intentionally limited to top-level hidden files and three levels below the `.config` entry, with a 4,096-entry budget. Other top-level directories are not traversed. Known key/credential locations, browser stores, caches, history, and generated data are excluded. Protected Dort/repository containers are not candidates. Reports may contain unsupported directories, links, and special files; a metadata candidate is **not** an assurance that content is safe. Unreadable/raced subtrees produce warnings; an oversized root directory fails with a limit diagnostic.

`files list` observes active mappings; `files plan` makes the safety/conflict decision. Plans are schema-versioned, deterministically ordered by file ID, and contain parent/file metadata fingerprints and safe regular-file SHA-256 hashes, never file contents. Human output shows source/destination paths, permissions, proposed backups and effects, conflicts, and destructiveness. Identical saved snapshots yield identical plans without further filesystem reads. Correct managed links are no-ops.

Initial adoption supports only owned, single-link regular files into a new repository source and a new active mapping. CLI/TUI adoption previews create a base-profile mapping; core callers can use other declared active profiles. It refuses existing sources, mapping/source collisions, directories, special files, foreign/broken links, detected sensitive content/paths, and special permission bits. Applying links can preview replacement of an ordinary regular destination only with an explicit backup effect. Unmanage requires the currently correct managed link; drift is blocked. Destructive removal requires a separate matching acknowledgement; CLI previews never collect it. Template/secret mappings and files with unverified tool prerequisites are always blocked, never silently treated as links.

Filename/content sensitivity checks are deliberately conservative heuristics, not secret detection guarantees; false positives are possible. Review content privately before adoption. Only the explicit mutation commands below can execute after approval; previews themselves never write.

## Approved local mutations (experimental)

Try these only in a [disposable HOME](docs/user/getting-started.md) first:

```sh
# Explicit path: absent destination under an existing safe parent, or valid Git+dort.toml root.
dort init /absolute/path/to/dotfiles --dry-run
dort init /absolute/path/to/dotfiles --yes
# Preview first; nonterminal/JSON writes always need --yes.
dort files add '~/.editorrc' --id editor --source files/editorrc --dry-run
dort files add '~/.editorrc' --id editor --source files/editorrc --yes
dort status
dort plan
dort files apply --dry-run
dort files apply --yes
dort files remove editor --mode keep --yes
# Alternative destructive mode (requires the correct managed link):
# dort files remove editor --mode remove --yes --confirm-remove editor
dort history
# dort rollback REPOSITORY_ID TRANSACTION_ID --dry-run
```

`files adopt`/`files unmanage` alias `add`/`remove`. Both unmanage modes retain sources. Interactive confirmation is available only with terminal stdin/stdout/stderr and without `--json`; `--yes` bypasses it, not validation or separate link-removal acknowledgement. Dry runs never prompt or create state, locks or artifacts. See [commands](docs/user/commands.md) for recovery and JSON details. On any failed write, inspect `history`: effects may be partially published, never automatically undone.

## Manifest basics

`dort.toml` uses schema 1 and rejects unknown fields. Tool/file IDs are unique within their respective namespaces. Tool dependencies must reference declared tools and cannot form cycles.

```toml
schema = 1

[profiles]
available = ["dev", "macos", "ubuntu"]
default = ["base"]

[profiles.platform]
macos = ["macos"]
ubuntu = ["ubuntu"]

[[files]]
id = "zshrc"
source = "files/common/zshrc"
target = "~/.zshrc"
mode = "link"
```

`base` is built in and always active. Other profiles must appear in `profiles.available`. Effective profiles are the union of base, defaults, platform selections, and explicit local/requested profiles. Multiple profiles on a resource use OR semantics; an explicit empty resource profile list is invalid.

See [examples/dort.toml](examples/dort.toml) for tool and future file-mode declarations. The checker validates these declarations but explicitly reports unsupported runtime capabilities. `manifest check` does not inspect provider availability, source contents, live symlinks, or installed versions. Use the separate file observation/preview commands for live-file diagnostics; tools remain unverified.

## Local state and safety

Both macOS and Ubuntu use the same XDG-compatible layout:

- Configuration: `${XDG_CONFIG_HOME:-$HOME/.config}/dort/local.toml`
- State: `${XDG_STATE_HOME:-$HOME/.local/state}/dort/`
- Repository state: `repositories/<local-repository-id>/` below the state directory

Empty XDG overrides are ignored; relative overrides are rejected. Core APIs receive an explicit host context, so tests do not change the process's HOME. Missing local state is not created by read-only commands.

Write-capable registry and filesystem lifecycle APIs require a single advisory application lock. Registry updates use no-follow descriptor-relative state access and synced atomic configuration replacement. Incomplete or malformed transactions block registry updates and all new filesystem transactions. Existing state paths must not traverse symlinks; private state directories use 0700 and configuration/checkpoints/backups use 0600, owned by the current user without hard links. Prepared files and retained originals preserve their live modes inside private transaction directories. Paths such as macOS `/var` may need their real, non-symlink spelling when used as explicit state overrides.

Live-file observation also walks absolute paths descriptor-relatively with no-follow directory opens and no-follow terminal inspection. Symlinked parents (including aliases such as macOS `/var`), source symlinks/escapes, hard links, directories, and special files cannot enter regular-file deployment plans. Terminal links can be classified as broken, looping, or unsafe without reading their referents. Regular-file reads are bounded to 16 MiB, check descriptor/named-file identity and metadata before/after reading, and hash only owned, single-link files. Non-UTF-8 report paths/link targets fail explicitly rather than being lossily serialized.

Snapshots are not globally atomic and do not authorize later writes. The experimental executors below independently revalidate approval and journal changes. No automatic Git merge may edit live-linked sources. No destructive real-home testing is permitted.

## Core filesystem execution and recovery (experimental)

The CLI exposes the shared core services through explicit approval boundaries. Core callers must register the repository, present and privately review an unblocked adoption preview, explicitly construct `adoption::Approval::from_plan`, then call `adoption::execute` while holding `WriterLock`. Approval is opaque and cannot be deserialized from preview JSON. Execution independently re-observes and compares the approved manifest, relevant configuration, mapping/effects, source/destination fingerprints and parent identities. Unrelated directory timestamp churn does not invalidate approval; identity, ownership and permissions still must match.

One transaction includes new source-parent directories (0700), source copy, manifest addition and destination link. The source and manifest preserve permission bits and group; the manifest addition preserves existing comments/text and is fully revalidated. Adoption copies file bytes, not ACLs or extended attributes. It supports only the regular-file link subset described above. **Repository, destination and transaction state must be on one filesystem supporting atomic exchange/no-replace renames**; there is no cross-device copy fallback. Individual publications are atomic, not the entire multi-path transaction. Other readers may see intermediate states.

`lifecycle::Approval::from_plan` and `lifecycle::execute` support link apply and unmanage-and-keep under the same lock/revalidation constraints. Removal instead requires `Approval::from_removal_plan` plus a separate `RemovalAcknowledgement::confirm` for the matching mapping ID. These are explicit caller approval boundaries, not interactive prompts or deserializable preview authorizations.

Apply groups all changed active mappings into **one recoverable transaction per approved request**, sharing newly needed destination parents (0700). Correct mappings stay untouched; an entirely correct or empty apply returns `None` with no journal/backups. Unmanage first materializes source bytes/mode/group at the destination (`keep`) or moves the managed link into private storage (`remove`), then removes only its manifest mapping using a focused, fully revalidated TOML edit. Both modes retain repository sources unchanged. All selected sources—including no-op apply mappings—are watched for edits during execution/rollback. These prototype APIs are bounded to 128 selected mappings and 256 operations per transaction; file reads remain bounded to 16 MiB.

### Journal schema 1

Transaction state lives under `repositories/<repository-id>/transactions/<transaction-id>/` (both IDs are 64 lowercase hexadecimal characters). It retains:

- Immutable, exclusively created `000000.json`, `000001.json`, … checkpoint envelopes. Each contains the exact JSON payload string and its SHA-256 checksum; the payload binds schema, sequence and previous checksum, repository/root/transaction IDs, approved-plan digest, input fingerprints, backup hashes, ordered operation IDs and observed postconditions.
- Private, synced `manifest.backup` and `destination.backup` files with verified SHA-256 hashes. No file contents or local variable values appear in checkpoints or diagnosis output. Ordinary backups are **not encrypted**; sensitive adoption is refused heuristically, not proven secret-free.
- `op-0000`, … prepared objects/retained originals. New files/directories are published with no-replace renames; manifest/destination replacements exchange the prepared object with the original. Both directories are synced. Originals and copied sources remain available in private transaction storage after rollback; there is no garbage collector yet.

### Journal schema 2

Apply/unmanage use a separate schema-2 payload in the same private transaction layout; schema-1 adoption records and recovery stay unchanged, with no migration. Schema 2 binds the action (`apply`, `keep`, `remove`), separate removal acknowledgement, approved-plan digest, profiles, original manifest/configuration, mapping/source/destination watches, parent anchors and backup hashes.

Ordered operations explicitly distinguish directory/destination/manifest roles and `publish` (no-replace), `exchange` (retain original) or `withdraw` (move link into private storage) mutations. `op-0000`, … holds prepared objects or originals; a withdrawal starts with an absent slot. `backup-0000`, … contains verified private byte copies of replaced regular files/manifests, or the exact raw link target text for replaced/removed links. No file contents enter checkpoint metadata. Apply never edits the manifest or sources; unmanage publishes the destination change before exchanging the edited manifest. Diagnosis independently validates each format's full chain and fails closed on unsupported versions or malformed tails.

### Journal schema 3: local bootstrap

`init PATH` creates a minimal `schema = 1` manifest and local Git repository in a private 0700 stage, or attaches an already-valid repository. Git initialization uses no templates, fixed `main`/SHA-1, no global/system configuration or inherited environment, and the existing bounded runner. It never clones, contacts a remote, stages or commits files. A missing destination requires an existing owned, non-group/world-writable physical parent; arbitrary existing directories are refused.

Schema 3 uses the same immutable checkpoint envelope/layout, but a dedicated bootstrap payload: `preparing → publish_intent → published → register_intent → completed`, or explicit `abandoned`. Intent precedes each no-replace repository publication and atomic registration publication/exchange. A synced bounded tree snapshot, parent/configuration identities and hashes bind recovery; configuration values/bodies stay only in private `config.new`/`config.backup`, never checkpoints. Original configuration inodes are retained after exchange. Repository/configuration/state must share a filesystem. Private configuration directories may be created during preparation.

Incomplete bootstrap blocks other writes. Inspect `recovery resume REPOSITORY_ID TRANSACTION_ID --dry-run` or `recovery abandon … --dry-run`, then approve explicitly. Resume recognizes only the exact prepared/published repository and before/after registration, refusing edits/substitutions. Incomplete private preparation can only be abandoned and retried. Abandon marks terminal without deleting a published repository, undoing registration or removing private artifacts. Malformed chains require manual diagnosis, not speculative recovery.

### Shared durability and recovery

For file lifecycle (schemas 1/2), durable phases are `preparing → running → verifying → completed`, with explicit `rolling_back → rolled_back`. Preparing may create only private artifacts, never live effects. Intent is checkpointed and synced **before each live mutation and each inverse**; postconditions get a separate immutable checkpoint. A torn final write cannot overwrite earlier durable intent. Missing records, truncated/malformed records, unsupported schemas, checksum/backup mismatches and invalid transitions fail closed. Diagnosis reports the valid prefix but never silently ignores a damaged tail. Checksums detect corruption; they do not authenticate against a malicious owner/root.

`recovery list` inspects all local repositories without creating state, acquiring a lock or replaying anything. It reports incomplete/malformed transactions as write blockers (exit 4). Use `rollback REPOSITORY_ID TRANSACTION_ID --dry-run` to inspect guarded schema-1/2 inverse paths; omit `--dry-run` and confirm to execute. Core callers can use `recovery::preview`/opaque approval/`execute`, dispatching to the existing schema-specific rollback with a fresh head/layout check under the matching lock. `history` and `lifecycle::history` return content-free retained history across all three formats; `recovery list` includes the last validated `journal_schema` (or null when none is valid). History diagnoses journal integrity, not live drift; a completed record does not guarantee rollback guards still match. There is **no automatic startup rollback**. Malformed journals require manual diagnosis/repair rather than speculative execution.

Rollback preflights all source, manifest, destination, staged-object and parent guards before changing anything, then rechecks around each inverse. Edits through the live source link, replacement files, changed parent identities/permissions, extra files in newly created directories or damaged backups stop recovery instead of overwriting user work. An interrupted rename can be recognized from the persisted intent and retained object identity/content; uncertain states are refused. Interrupted rollback can itself be resumed explicitly. Failed calls leave durable state for diagnosis, never silently auto-undo.

The lock coordinates Dort writers, not arbitrary editors or a malicious same-user process. Descriptor-relative no-follow operations avoid traversing substituted symlink parents, but these checks are not filesystem-wide isolation. The ordinary tests exercise process exits at durable boundaries and simulated torn records. A separately approved, opt-in Linux VM harness additionally exercises real ENOSPC and partial checkpoint writes, **not failing fsync or real power loss**. Retention/scan bounds and lack of garbage collection are further prototype limits.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/check-docs.py --self-test
python3 scripts/check-docs.py
python3 scripts/test-terminal.py
git diff --check
```

The Python gates require Python 3 (standard library only). PTY tests use disposable child environments and do not require installed Git. CI requires non-root permission fixtures and distinct Linux cross-device fixture filesystems; optional skips are visible in test output.

See [validation methods](docs/validation.md) for test coverage, fixture prerequisites, terminal/storage harnesses and evidence limits. CI is configured for Ubuntu x86_64/ARM64 and macOS Intel/Apple Silicon; local success and an earlier CI run do not establish that a changed candidate passed all platforms.

Integration tests use disposable canonical homes/repositories and isolated child environments. They cover observation/planning guards, approved journalled lifecycle/bootstrap/recovery, concurrency, source preservation, no-op apply, Git hardening and CLI/TUI approval/terminal behavior. CLI Git fixtures do not require installed Git; optional core real-Git fixtures announce skips. Mounted-volume fault injection is opt-in, requires separately approved disposable infrastructure and never belongs on a workstation or in ordinary CI.

### Maintaining documentation and history

Keep this README, `ARCHITECTURE.md`, maintained technical notes in `docs/`, end-user guides in `docs/user/`, and `AGENTS.md` aligned with implementation changes. The changelog filename is intentionally `CHANGLOG.md`.

Generate history with pinned git-cliff **2.14.2** (local installation stays ignored):

```sh
cargo install git-cliff --version 2.14.2 --locked --root temp/tooling/git-cliff
scripts/changelog.sh
scripts/changelog.sh --check
```

The script uses `cliff.toml`, local committed history and no remote metadata or command hooks. It never installs tools, commits, pushes or creates tags. Regenerate after authorized implementation commits; generated-only commits should use `chore(changelog): regenerate history` to avoid self-referential entries. Full history/tags are required; see [AGENTS.md](AGENTS.md#changelog-maintenance).

## Documentation layout

- `docs/user/`: tracked end-user documentation for available commands, manifests, workflows, safety and troubleshooting; no development content.
- `docs/`: maintained technical references about the current system and a documentation index, not temporary feature plans or release worklists.
- `specs/`: ignored feature specifications, interim decisions, roadmaps, implementation plans and candidate-specific release/evidence notes. These may become obsolete after implementation; do not commit them or require/link them from tracked docs, code, tests or builds.

Keep current behavior documented as it changes. Promote enduring technical facts from completed work into maintained references, rather than retaining completed task lists in source control.

## License

[MIT](LICENSE), copyright (c) 2026 Kate Song.
