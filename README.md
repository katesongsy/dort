# Dort

**Your development environment, everywhere.**

Dort describes a development environment in Git: dotfiles, profiles and the tools you expect. It reconciles that description without replacing Git or your package manager.

## Status

Experimental, pre-1.0. Supported hosts: **macOS, Linux and WSL** on x86_64 or ARM64. Prebuilt Linux archives need glibc 2.39+ (Ubuntu 24.04 or newer).

Working today:

- Local repository create/attach, manifest checks and profiles.
- Discover, adopt, link, keep and remove regular files with guarded history and recovery.
- Read-only Git status and diff, with explicit patch disclosure.
- Homebrew and npm tool checks, missing-tool installs and declaration editing.
- CLI and TUI workflows for the supported tasks.

Not implemented: templates, secrets, Git clone/sync, staging/commit/push, and general package upgrades.

## Install

[GitHub Releases](https://github.com/katesongsy/dort/releases) provide native archives with SHA-256 checksums. Rust is not required.

Homebrew:

```sh
brew install katesongsy/tap/dort
```

Mise:

```sh
mise use --global github:katesongsy/dort@0.1.0
```

Cargo:

```sh
cargo install dort --locked
```

Cargo builds need Rust **1.99.1**. See [getting started](docs/user/getting-started.md#installation) for archive selection, checksum verification and a disposable-HOME walkthrough.

## Documentation

- [User guide](docs/user/README.md)
- [Getting started](docs/user/getting-started.md)
- [Manifest guide](docs/user/manifest.md)
- [Command reference](docs/user/commands.md)
- [Safety and troubleshooting](docs/user/safety.md)
- [Roadmap](docs/roadmap.md)
- [Changelog](CHANGLOG.md)

Contributor notes:

- [Architecture](ARCHITECTURE.md): boundaries, data flow and safety limits.
- [Design guide](docs/design-system.md): shared TUI principles.
- [Validation](docs/validation.md): commands, fixtures and evidence limits.
- [Release mechanics](docs/releases.md): authorization, tooling and artifacts.
- [Agent rules](AGENTS.md): working agreements and checks.

## Development

From the repository root, with Rust 1.99.1:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/test.py full
cargo build --workspace --locked
python3 scripts/check-docs.py
python3 scripts/check-tui-style.py
python3 scripts/test-terminal.py
git diff --check
```

`python3 scripts/test.py` runs focused unit checks. Choose `npm`, `tools` or `ui`; use `--list` to see commands. `full` builds once and runs every test in its own process with a private temporary directory.

## License

[MIT](LICENSE), copyright (c) 2026 Kate Song.
