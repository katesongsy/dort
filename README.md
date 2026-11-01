# Dort

**Your development environment, everywhere.**

Dort is a Git-native developer environment manager. It is designed to reconcile tools, dotfiles, profiles, templates, and encrypted configuration through a shared CLI/TUI engine—without replacing Git or existing package managers.

## Current status

**Foundation slice (A0–A3), not a usable dotfile manager yet.** The implementation provides strict manifest validation, profile/dependency resolution, platform detection, explicit host contexts, and local repository-registry/locking APIs. The CLI is read-only.

Dotfile discovery/adoption/deployment, package-manager execution, remote cloning/sync, templates, secrets, and the Ratatui interface are **not implemented**. Successfully checking a manifest does not mean its tools are installed or its files are safe to deploy.

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

Without `--manifest` or `--repo`, manifest checking uses a locally registered active repository. Registration currently exists only as a core API; there is no `init` command yet. `--profile` affects only the current check.

JSON success output goes to stdout; JSON errors go to stderr. Exit codes are 0 for success, 1 for I/O failure, 2 for invalid input/configuration or unsupported hosts, and 3 for a held writer lock. No current CLI command acquires a write lock or changes repository registration.

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

See [examples/dort.toml](examples/dort.toml) for tool and future file-mode declarations. The checker validates these declarations but explicitly reports unsupported runtime capabilities. Provider availability, source contents, live symlinks, and installed versions are not inspected.

## Local state and safety

Both macOS and Ubuntu use the same XDG-compatible layout:

- Configuration: `${XDG_CONFIG_HOME:-$HOME/.config}/dort/local.toml`
- State: `${XDG_STATE_HOME:-$HOME/.local/state}/dort/`
- Repository state: `repositories/<local-repository-id>/` below the state directory

Empty XDG overrides are ignored; relative overrides are rejected. Core APIs receive an explicit host context, so tests do not change the process's HOME. Missing local state is not created by read-only commands.

Write-capable registry APIs require a single advisory application lock. They use private directories/files, no-follow descriptor-relative state access, and synced atomic configuration replacement. Existing state paths must not traverse symlinks; existing Dort directories must be private (0700) and local state files private (0600), owned by the current user, and not hard-linked. Paths such as macOS `/var` may need their real, non-symlink spelling when used as explicit state overrides.

These are **local metadata protections**, not a finished file transaction engine. Live deployment will require reviewed plans, backups, durable journals, revalidation, and guarded recovery. No destructive real-home testing is permitted.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Local validation on macOS ARM64: **38 tests pass**, formatting passes, and Clippy passes with warnings denied. Cross-platform CI is configured but has not been run locally.

Integration tests use disposable homes/repositories and isolated child-process environments. They never install packages or mutate the developer's configuration. CI is configured for Ubuntu x86_64/ARM64 and macOS Intel/Apple Silicon; local validation does not establish that all CI platforms have passed.

## Roadmap

Implementation specs and planning notes are transient local artifacts in ignored `specs/`; they are not required to build or test Dort. Implemented behaviour and usage are documented in this README.

The next slice is read-only file observation/planning (A4–A5), followed by recoverable single-file adoption (A6–A7). The broader roadmap adds tools, unified setup, generated/secret configuration, safe Git synchronisation, and the full TUI.
