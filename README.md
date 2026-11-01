# Dort

**Your development environment, everywhere.**

Dort is a Git-native developer environment manager. It is designed to reconcile tools, dotfiles, profiles, templates, and encrypted configuration through a shared CLI/TUI engine—without replacing Git or existing package managers.

## Current status

**Core filesystem lifecycle slice (A0–A8), with an entirely read-only CLI.** Dort provides strict manifest validation, profile/dependency resolution, platform detection, local registry/locking APIs, bounded discovery, descriptor-safe file observation, pure lifecycle previews, and experimental core-only adoption, link apply, unmanage and guarded rollback/history with immutable journals.

Public adoption/deployment/recovery write commands, package-manager execution, remote cloning/sync, templates, secrets, and the Ratatui interface are **not implemented**. A successful preview is not permission or an executable artifact for a later write. The executor is tested only against disposable homes; this is not yet a production-ready dotfile manager.

## Documentation

- [User guide](docs/README.md): [getting started](docs/getting-started.md), [manifest guide](docs/manifest.md), [commands](docs/commands.md), and [safety/troubleshooting](docs/safety.md).
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

Without `--manifest` or `--repo`, repository commands use a locally registered active repository. Registration currently exists only as a core API; there is no `init` command yet. Discovery also works without a repository. `--profile` affects only the current check/list/preview.

JSON reports go to stdout; JSON errors go to stderr. Exit codes are 0 for success, 1 for I/O failure, 2 for invalid input/configuration or unsupported hosts, 3 for a held writer lock, and **4 for a blocked plan or recovery diagnosis reporting write blockers**. Those reports are still emitted in full on stdout (not as a separate stderr error). No current CLI command acquires a write lock or changes repository registration.

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

Initial adoption supports only owned, single-link regular files into a new repository source and a new active mapping. The CLI adoption preview creates a base-profile mapping; core callers can use other declared active profiles. It refuses existing sources, mapping/source collisions, directories, special files, foreign/broken links, detected sensitive content/paths, and special permission bits. Applying links can preview replacement of an ordinary regular destination only with an explicit backup effect. Unmanage requires the currently correct managed link; drift is blocked. Destructive removal requires a separate core acknowledgement; CLI previews never collect it. Template/secret mappings and files with unverified tool prerequisites are always blocked, never silently treated as links.

Filename/content sensitivity checks are deliberately conservative heuristics, not secret detection guarantees; false positives are possible. Review content privately before adoption. No current CLI command can execute a plan, collect approval, or create a backup.

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

Snapshots are not globally atomic and do not authorize later writes. The experimental core executors below revalidate approval and journal changes, but public deployment remains unavailable. No automatic Git merge may edit live-linked sources. No destructive real-home testing is permitted.

## Core filesystem execution and recovery (experimental)

There are deliberately **no CLI write commands**. Core callers must register the repository, present and privately review an unblocked adoption preview, explicitly construct `adoption::Approval::from_plan`, then call `adoption::execute` while holding `WriterLock`. Approval is opaque and cannot be deserialized from preview JSON. Execution independently re-observes and compares the approved manifest, relevant configuration, mapping/effects, source/destination fingerprints and parent identities. Unrelated directory timestamp churn does not invalidate approval; identity, ownership and permissions still must match.

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

### Shared durability and recovery

The durable phases are `preparing → running → verifying → completed`, with explicit `rolling_back → rolled_back`. Preparing may create only private artifacts, never live effects. Intent is checkpointed and synced **before each live mutation and each inverse**; postconditions get a separate immutable checkpoint. A torn final write cannot overwrite earlier durable intent. Missing records, truncated/malformed records, unsupported schemas, checksum/backup mismatches and invalid transitions fail closed. Diagnosis reports the valid prefix but never silently ignores a damaged tail. Checksums detect corruption; they do not authenticate against a malicious owner/root.

`recovery list` inspects all local repositories without creating state, acquiring a lock or replaying anything. It reports incomplete/malformed transactions as write blockers (exit 4). Recovery is core-only: use `adoption::rollback` for schema 1 or `lifecycle::rollback` for schema 2, with an explicit transaction ID and matching lock. `lifecycle::history` returns content-free retained history for both formats; `recovery list` includes the last validated `journal_schema` (or null when none is valid). History diagnoses journal integrity, not live drift; a completed record does not guarantee rollback guards still match. There is **no automatic startup rollback**. Malformed journals require manual diagnosis/repair rather than speculative execution.

Rollback preflights all source, manifest, destination, staged-object and parent guards before changing anything, then rechecks around each inverse. Edits through the live source link, replacement files, changed parent identities/permissions, extra files in newly created directories or damaged backups stop recovery instead of overwriting user work. An interrupted rename can be recognized from the persisted intent and retained object identity/content; uncertain states are refused. Interrupted rollback can itself be resumed explicitly. Failed calls leave durable state for diagnosis, never silently auto-undo.

The lock coordinates Dort writers, not arbitrary editors or a malicious same-user process. Descriptor-relative no-follow operations avoid traversing substituted symlink parents, but these checks are not filesystem-wide isolation. The tests exercise process exits at durable boundaries and simulated torn records, **not real power-loss/filesystem fault injection**. Retention/scan bounds and lack of garbage collection are further prototype limits.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Local validation on macOS ARM64: **108 tests pass**, formatting passes, and Clippy passes with warnings denied. Cross-platform CI is configured but has not been run locally. Linux additionally exercises FIFO observation, real non-UTF-8 directory/link entries (APFS rejects such filenames at creation), and cross-device refusal when a separate `/dev/shm` filesystem is available.

Integration tests use disposable homes/repositories and isolated child-process environments. Coverage includes symlink parents/loops, racing terminal substitution, hard links/special files, sensitive exclusions, observation limits, profile overrides, deterministic/no-op/blocked plans, both unmanage preview modes, every adoption/apply/unmanage/recovery checkpoint and post-mutation error boundary, child-process crash/recovery and lock release for forward and inverse lifecycle work, schema-1 compatibility, no-journal repeated apply, separate removal acknowledgement, focused manifest edits, shared destination parents, source preservation, CLI history diagnosis without replay, malformed/checksum/schema/backup failures, stale approvals, source/user-edit rollback guards, and CLI no-write/content-redaction assertions. They never install packages or mutate the developer's configuration. CI is configured for Ubuntu x86_64/ARM64 and macOS Intel/Apple Silicon; local validation does not establish that all CI platforms have passed.

### Maintaining documentation and history

Keep this README, `ARCHITECTURE.md`, end-user `docs/`, and `AGENTS.md` aligned with implementation changes. The changelog filename is intentionally `CHANGLOG.md`.

Generate history with pinned git-cliff **2.14.2** (local installation stays ignored):

```sh
cargo install git-cliff --version 2.14.2 --locked --root temp/tooling/git-cliff
scripts/changelog.sh
scripts/changelog.sh --check
```

The script uses `cliff.toml`, local committed history and no remote metadata or command hooks. It never installs tools, commits, pushes or creates tags. Regenerate after authorized implementation commits; generated-only commits should use `chore(changelog): regenerate history` to avoid self-referential entries. Full history/tags are required; see [AGENTS.md](AGENTS.md#changelog-maintenance).

## Roadmap

Implementation specs and planning notes are transient local artifacts in ignored `specs/`; they are not required to build or test Dort. Implemented behaviour is documented in this README, tracked end-user `docs/`, and `ARCHITECTURE.md`. The new `docs/` is public documentation, not the ignored implementation notes.

The next slice adds read-only Git status/diff (A9); public write/approval commands remain deferred to A10. Core filesystem lifecycle execution (A8) is sandbox-tested, not a production deployment guarantee. The broader roadmap adds tools, unified setup, generated/secret configuration, safe Git synchronisation, and the full TUI.

## License

[MIT](LICENSE), copyright (c) 2026 Kate Song.
