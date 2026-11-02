# Working on Dort

These repository instructions apply to all files unless a more specific instruction overrides them.

## Scope and current capabilities

- Read [README.md](README.md) and [ARCHITECTURE.md](ARCHITECTURE.md) before changing behaviour.
- Dort is a Rust workspace: `crates/dort-core` owns UI-independent services; `crates/dort` owns the Clap CLI and presentation.
- The CLI is currently entirely read-only. Core-only regular-file adoption, link apply, unmanage and guarded rollback/history are experimental. Do not expose writes, install tools for users, or imply unsupported capabilities work as part of an unrelated change.
- Do not introduce a TUI/provider execution layer or change journal/schema/security contracts without explicit agreement.
- Keep existing uncommitted work intact. Do not commit, push, tag or release unless requested.

## Required documentation maintenance

Update documentation in the same change as the behaviour it describes:

| File | Maintain when |
| --- | --- |
| `README.md` | Status, build requirements, navigation, commands, limitations or roadmap change. |
| `ARCHITECTURE.md` | Module boundaries, data flow, persistence/schema contracts, safety guarantees or architectural limitations change. |
| `docs/` | User-visible commands, flags, manifests, workflows, output/exit codes or troubleshooting change. |
| `AGENTS.md` | Contributor workflow, tooling, validation or standing project constraints change. |
| `CHANGLOG.md` | Authorized commits add history; regenerate with git-cliff, never hand-edit generated entries. |

- `docs/` is tracked, end-user documentation. Keep examples usable and distinguish previews from execution.
- `specs/` contains ignored, transient implementation notes. Never commit it or require it in builds, tests, runtime code or tracked documentation links.
- Do not describe planned features as shipped. State local validation separately from unobserved remote CI.
- Keep relative links valid. Keep the requested spelling `CHANGLOG.md`; do not create a competing `CHANGELOG.md`.

## Rust workflow

- Use the pinned Rust 1.99.1 toolchain, edition 2024 and committed `Cargo.lock`.
- Prefer existing dependencies. Make lockfile changes deliberately, then validate with `--locked`.
- Keep domain policy out of CLI handlers. The pure planner must stay free of filesystem I/O, clocks and randomness.
- Use explicit `HostContext` inputs; only the application boundary discovers environment/platform values.
- Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
git diff --check
```

## Filesystem and recovery invariants

- Never test destructive operations against the real HOME, real dotfiles or package managers.
- Use disposable canonicalized homes/repositories (`tests/support::TestHome`). macOS `/var` aliases are symlinks; use physical temporary paths.
- Do not mutate process-global HOME/XDG variables in tests. Configure isolated child processes instead. CLI tests use deterministic fake Git/missing-Git fixtures and must not require installed external tools. Optional core real-Git integration tests use only disposable repositories and skip explicitly when Git is absent.
- Keep live/state access descriptor-relative and no-follow. Do not substitute ordinary path-based writes for secure helpers.
- Reject unsafe parents, special files, hard links, sensitive ordinary adoption and unsupported template/secret/prerequisite execution. Filename/content heuristics are not proof of secrecy.
- Preserve bounded reads, stable identities and explicit rejection of non-UTF-8 report paths; never silently use lossy path identity.
- Writers require the application lock, explicit approval and fresh independent revalidation. Preview JSON is not execution authorization.
- Persist immutable checksummed intent before live mutations/inverses; verify private durable backups and postconditions. Incomplete/malformed transactions block subsequent writes.
- Keep schema-1 adoption diagnosis/rollback compatible. Schema-2 apply/unmanage uses one recoverable transaction per approved request, immutable source watches and a separate matching removal acknowledgement. No silent format migration.
- Recovery is explicit and guarded. Never automatically replay/rollback on startup or overwrite later user/source edits.
- Preserve the current same-filesystem atomic-rename constraint; do not add an unsafe cross-device fallback.
- Do not put file contents, local variable values or secrets in ordinary reports, logs or checkpoint metadata. The agreed narrow exception is explicit `git diff --patch`: bounded patch content is disclosed only in that requested report, with unsafe bytes escaped, never in logs/errors/journals. Summary/status reports stay metadata-only; patches are not secret-proof. Backups are private but ordinary backups are not encrypted.
- Extend failure-boundary/process-crash tests when changing the executor. Do not claim process-exit tests establish power-loss safety.
- Do not automatically merge Git changes into live-linked sources.
- Git inspection uses a caller-supplied trusted absolute executable, cleared child environment, fixed read-only arguments, no optional locks/index refresh, no hooks/external diff/textconv/fsmonitor/submodule recursion, and bounded concurrent pipe drains/deadlines. Reject configured content filters and partial clones/promisor settings before status/diff; no unsafe fallback, arbitrary arguments, network or writes. This is hardening, not isolation against a malicious same-user config race. Keep Git executable/environment resolution at the CLI boundary.

## Changelog maintenance

git-cliff **2.14.2** is pinned by `scripts/changelog.sh`. Install locally when authorized:

```sh
cargo install git-cliff --version 2.14.2 --locked --root temp/tooling/git-cliff
scripts/changelog.sh
scripts/changelog.sh --check
```

The script uses `cliff.toml`, disables network metadata/command hooks, and changes only the generated file (plus ignored temporary tooling). It does not install tools, commit, create tags or change releases.

Prefer descriptive Conventional Commit subjects (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`). Legacy non-conventional commits remain included. Release tags follow `vMAJOR.MINOR.PATCH` with optional prereleases.

Generation uses committed history, not the working tree. Regenerate after authorized feature/fix/documentation commits or before release preparation. If committing only the regenerated file, use `chore(changelog): regenerate history`; this maintenance subject is excluded to avoid self-referential churn. `chore(release):` commits are also excluded. Never fabricate commits or tag a release merely to populate the file. A full-history checkout (including tags) is required for reproducible generation.

## License

Dort is MIT-licensed; keep [LICENSE](LICENSE) and Cargo license metadata consistent. Preserve copyright notices.
