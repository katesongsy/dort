# Dort

**Your development environment, everywhere.**

Dort is a Git-native developer environment manager. It is designed to reconcile tools, dotfiles, profiles, templates, and encrypted configuration through a shared CLI/TUI engine—without replacing Git or existing package managers.

## Current status

**Experimental CLI and TUI — development after v0.1.0.** Dort provides strict manifest validation, profile/dependency resolution, bounded discovery, descriptor-safe observation, pure previews, hardened read-only Git status/diff, local repository init/attach, and approved adoption, link apply, unmanage and guarded recovery with immutable journals.

## Documentation

- [User guide](docs/user/README.md): [getting started](docs/user/getting-started.md), [manifest guide](docs/user/manifest.md), [commands](docs/user/commands.md), and [safety/troubleshooting](docs/user/safety.md).
- [Documentation index](docs/README.md): end-user guides and maintained technical references.
- [Validation methods](docs/validation.md): test infrastructure, isolation and interpretation of results.
- [Release mechanics](docs/releases.md): native packaging, provenance and publication guards.
- [Architecture](ARCHITECTURE.md): module boundaries, data flow, journal/recovery contracts and limitations.
- [Contributor/agent instructions](AGENTS.md): development checks and documentation-maintenance rules.
- [Changelog](CHANGLOG.md): generated with git-cliff from committed Git history (uncommitted work is excluded).
- [MIT license](LICENSE).

## Install, build and run

[GitHub Releases](https://github.com/katesongsy/dort/releases) provide native archives and SHA-256 checksums for macOS 15+ (Apple Silicon or Intel) and Ubuntu 24.04+ (ARM64 or x86_64; glibc 2.39+). See [installation instructions](docs/user/getting-started.md#installation) to select and verify an archive. The binaries do not require Rust. Source packages are also published on crates.io as [`dort`](https://crates.io/crates/dort) and [`dort-core`](https://crates.io/crates/dort-core); Cargo installation requires Rust 1.99.1.

### Homebrew

The third-party [Homebrew tap](https://github.com/katesongsy/homebrew-tap) provides prebuilt binaries:

```sh
brew install katesongsy/tap/dort
```

After `brew tap katesongsy/tap`, the short command `brew install dort` also works. This is not Homebrew Core inclusion. The tap owns only Homebrew packaging and validation.

### Mise

Mise's built-in GitHub backend installs the native release directly:

```sh
mise use --global github:katesongsy/dort@0.1.0
```

No third-party plugin or short-name registry entry is required. Mise validation is maintained separately in this repository. Homebrew and mise have the native archive's supported-host requirements and do not require Rust.

### Source builds

For source builds, Rust **1.99.1** is the pinned toolchain and current minimum supported Rust version; `rustup` will use `rust-toolchain.toml`.

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

JSON reports go to stdout; JSON errors go to stderr. Exit codes are 0 for success, 1 for I/O failure or unavailable provider/binary observation, 2 for invalid input/configuration or unsupported hosts, 3 for a held writer lock, and **4 for a blocked plan or recovery diagnosis reporting write blockers**. Those reports are still emitted in full on stdout (not as a separate stderr error). Mutations require confirmation or `--yes`; JSON/nonterminal writes require `--yes` and never prompt. `--dry-run` never creates state or locks.


## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/test.py full
cargo build --workspace --locked
python3 scripts/check-docs.py --self-test
python3 scripts/check-docs.py
python3 scripts/check-tui-style.py --self-test
python3 scripts/check-tui-style.py
python3 scripts/test-terminal.py
python3 scripts/test-cross-device-macos.py --self-test
python3 scripts/release.py --self-test
python3 scripts/check-crates.py --self-test
python3 scripts/check-crates.py
cargo package --workspace --locked
python3 scripts/check-crates.py --archives
git diff --check
```

For faster development checks, run `python3 scripts/test.py` (units), or choose `npm`, `tools` or `ui`. Use `--list` to inspect a suite. `python3 scripts/test.py full` builds once, then runs every test in its own process with private `TMPDIR`, up to 8 concurrently; use `--jobs` and `--timeout` to tune it. Focused runs do not replace milestone durability validation. Each test keeps its own assertions, deadlines and filesystem durability; the runner only changes process isolation and scheduling.

The Python gates require Python 3 (standard library only). PTY tests use disposable child environments and do not require installed Git. CI requires non-root permission fixtures and distinct Linux cross-device fixture filesystems; optional skips are visible in test output.

Follow the mandatory [TUI design system](docs/design-system.md) and [interaction contracts](docs/tui-interactions.md) before changing presentation. Shared components live in `crates/dort/src/tui/components/`; the strict style checker and actual-component gallery supplement—not replace—human visual review.

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
