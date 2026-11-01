# Dort

**Your development environment, everywhere.**

Dort is a Git-native developer environment manager. It is designed to reconcile tools, dotfiles, profiles, templates, and encrypted configuration through a shared CLI/TUI engine—without replacing Git or existing package managers.

## Current status

**Read-only observe/plan slice (A0–A5), not a deploying dotfile manager yet.** Dort provides strict manifest validation, profile/dependency resolution, platform detection, local registry/locking APIs, bounded discovery, descriptor-safe file observation, and pure link/adoption/unmanage previews.

Actual adoption/deployment, backups/journals/recovery, package-manager execution, remote cloning/sync, templates, secrets, and the Ratatui interface are **not implemented**. Every current CLI command is read-only. A successful preview is not permission or an executable artifact for a later write.

## Build and run

Requires a supported host: macOS (Apple Silicon or Intel), or Ubuntu (ARM64 or x86_64). Rust **1.99.1** is the pinned toolchain and current minimum supported Rust version; `rustup` will use `rust-toolchain.toml`.

```bash
cargo build --workspace --locked
cargo run -p dort -- --help

# Read-only host/platform diagnostics; does not create local state
cargo run -p dort -- doctor
cargo run -p dort -- --json doctor

# Validate the example without installing tools or touching live dotfiles
cargo run -p dort -- manifest check --manifest examples/dort.toml
cargo run -p dort -- --json manifest check --manifest examples/dort.toml --profile dev

# Check an existing environment repository explicitly
cargo run -p dort -- --repo /path/to/dotfiles manifest check
```

Without `--manifest` or `--repo`, repository commands use a locally registered active repository. Registration currently exists only as a core API; there is no `init` command yet. Discovery also works without a repository. `--profile` affects only the current check/list/preview.

JSON reports go to stdout; JSON errors go to stderr. Exit codes are 0 for success, 1 for I/O failure, 2 for invalid input/configuration or unsupported hosts, 3 for a held writer lock, and **4 for a blocked plan**. A blocked plan is still emitted in full on stdout (not as a separate stderr error). No current CLI command acquires a write lock or changes repository registration.

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

Initial adoption supports only owned, single-link regular files into a new repository source and a new base-profile mapping. It refuses existing sources, mapping/source collisions, directories, special files, foreign/broken links, detected sensitive content/paths, and special permission bits. Applying links can preview replacement of an ordinary regular destination only with an explicit backup effect. Unmanage requires the currently correct managed link; drift is blocked. Destructive removal is marked as requiring a separate acknowledgement in a future executor. Template/secret mappings and files with unverified tool prerequisites are always blocked, never silently treated as links.

Filename/content sensitivity checks are deliberately conservative heuristics, not secret detection guarantees; false positives are possible. Review content privately before any eventual adoption. No current command can execute a plan, collect approval, or create a backup.

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

Write-capable registry APIs require a single advisory application lock. They use private directories/files, no-follow descriptor-relative state access, and synced atomic configuration replacement. Existing state paths must not traverse symlinks; existing Dort directories must be private (0700) and local state files private (0600), owned by the current user, and not hard-linked. Paths such as macOS `/var` may need their real, non-symlink spelling when used as explicit state overrides.

Live-file observation also walks absolute paths descriptor-relatively with no-follow directory opens and no-follow terminal inspection. Symlinked parents (including aliases such as macOS `/var`), source symlinks/escapes, hard links, directories, and special files cannot enter regular-file deployment plans. Terminal links can be classified as broken, looping, or unsafe without reading their referents. Regular-file reads are bounded to 16 MiB, check descriptor/named-file identity and metadata before/after reading, and hash only owned, single-link files. Non-UTF-8 report paths/link targets fail explicitly rather than being lossily serialized.

These are **observation and local metadata protections**, not a finished file transaction engine. Snapshots are not globally atomic and do not authorize later writes. A future executor must lock and revalidate the manifest, relevant configuration, parent identities, sources and destinations; persist transaction intent and durable backups before destructive effects; and refuse rollback over subsequent user/source edits. Adoption's source copy, manifest edit and destination replacement must recover together. No automatic Git merge may edit live-linked sources. No destructive real-home testing is permitted.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Local validation on macOS ARM64: **67 tests pass**, formatting passes, and Clippy passes with warnings denied. Cross-platform CI is configured but has not been run locally. Linux additionally exercises FIFO observation and real non-UTF-8 directory/link entries (APFS rejects such filenames at creation).

Integration tests use disposable homes/repositories and isolated child-process environments. Coverage includes symlink parents/loops, racing terminal substitution, hard links/special files, sensitive exclusions, observation limits, profile overrides, deterministic/no-op/blocked plans, both unmanage modes, and CLI no-write/content-redaction assertions. They never install packages or mutate the developer's configuration. CI is configured for Ubuntu x86_64/ARM64 and macOS Intel/Apple Silicon; local validation does not establish that all CI platforms have passed.

## Roadmap

Implementation specs and planning notes are transient local artifacts in ignored `specs/`; they are not required to build or test Dort. Implemented behaviour and usage are documented in this README.

The next slice is durable journals/backup storage and recoverable single-file adoption (A6–A7), followed by completing link/unmanage execution. The broader roadmap adds tools, unified setup, generated/secret configuration, safe Git synchronisation, and the full TUI.
