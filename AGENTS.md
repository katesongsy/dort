# Working on Dort

These instructions apply repository-wide unless a more specific file overrides them.

## Start here

- Read [README.md](README.md) and [ARCHITECTURE.md](ARCHITECTURE.md) before changing behavior. Use the [documentation index](docs/README.md) for feature-specific contracts.
- `crates/dort-core` owns UI-independent services; `crates/dort` owns the CLI and TUI. Keep domain policy in core and the pure planner free of I/O, clocks and randomness. Discover environment/platform values at the application boundary through explicit `HostContext` inputs.
- Preserve existing uncommitted work. Commit, push, tag, release and publish only when requested; registry publication needs separate approval.
- Keep changes within the approved feature scope. Provider execution, installation, persistence/schema changes and cancellation contracts require explicit agreement.

## Writing and UX

- Be concise in all UI text, documentation and code comments. State what something is or does directly. Avoid defensive phrasing and unnecessary contrasts such as “this doesn't do X” or “this isn't Y.” Mention limitations, expectations or rationale only when needed for correct use or an informed decision.
- Prefer straightforward defaults, clear actions and short explanations. Keep routine workflows simple while preserving necessary safeguards.
- Confirm actions with a clear button, key activation or simple CLI confirmation. Bind approval internally to the exact preview/selection; never require typing a literal word, phrase or ID.
- Every supported CLI workflow needs a complete TUI workflow in the same milestone. Keep JSON and automatic-approval flags as CLI automation concerns.

## TUI changes

- Read the [design system](docs/design-system.md) and [interaction contracts](docs/tui-interactions.md) before editing. Identify the shared components and semantic roles you will reuse.
- Shared visual components, colors, emphasis, glyphs and motion belong in `crates/dort/src/tui/components/`; workflow controllers and approval ownership stay outside. New semantics, exceptions and standard changes need operator review.
- Preserve the documented shell keys, one foreground action strip, distinct input/fact/selection/feedback styles, honest progress and keyboard-only compact/wide navigation. Forms exclude footer buttons; explicit readers own arrows and paging. Enter invokes the contextual primary action.
- Keep drafts and opaque previews when menus open or close; discard approvals on actual navigation. Explain restricted navigation during dialogs or busy states. Keep escaped metadata separate from trusted decoration, with foreground-only carets and hits.
- Preserve dry-run, layout-floor, shrink-cleared approval, privacy and joined-worker guards. Use actual-component/backend and terminal tests alongside style checks. Inspect visual differences and obtain review for changed baselines; automated passes do not establish human visual or usability approval.
- Report reused components, validation run and remaining deviations.

## Safety essentials

- Use disposable canonicalized homes/repositories and fake providers for tests. Isolate child environments rather than changing process-global HOME/XDG. Real-home, package-manager, privileged, mounted-volume and storage-fault tests need separately approved disposable scopes; follow [validation methods](docs/validation.md).
- Keep filesystem access descriptor-relative, bounded and no-follow, with stable identities and exact UTF-8 report paths. Preserve private backups, same-filesystem atomic rename and rejection of unsafe parents, special files and hard links.
- Writers require the application lock, an opaque single-use approval, fresh independent revalidation and durable checksummed intent before mutation. Preserve existing journal schemas/raw payloads and failure-boundary/process-crash coverage. Recovery stays explicit and refuses later edits; incomplete or malformed transactions block writers.
- Discovery, checks, plans, status, history and Git inspection stay read-only and provider-free. Provider queries and installs use their separate reviewed approval paths. Preserve explicit executable/runtime/prefix/snapshot bindings, metadata-only defaults, unavailable-versus-empty facts and fresh review on retry. Follow the [inspection](docs/tools-inspection.md), [installation](docs/tool-installation.md), [reconciliation](docs/binary-reconciliation.md) and [declaration](docs/tool-declarations.md) contracts.
- Keep file contents and secrets out of ordinary reports, logs, diagnostics and persistence. Preserve the bounded selected-file Discover reader and separately confirmed Git patch disclosure, including payload lifetime and layout guards.
- Dry runs remain side-effect-free. CLI writes require terminal confirmation or explicit `--yes`; TUI writes consume the real in-memory preview. Quit joins active workers, and terminal state is restored on return, error or panic.

## Documentation

- Update documentation with behavior: `README.md` for status/build/navigation/commands, `ARCHITECTURE.md` for boundaries/data flow/persistence/safety, `docs/user/` for end-user guidance, and other `docs/` pages for durable technical facts.
- Keep contributor rules in `AGENTS.md` and temporary plans, worklists and candidate evidence in ignored `specs/`. Tracked documents, code and builds must be independent of `specs/`. Document implemented behavior and distinguish local validation from observed CI.
- Check maintained Markdown recursively, including `docs/user/`, and repair relative links when moving pages.
- Preserve MIT notices and keep package-local licenses/example manifests byte-identical to root `LICENSE` and `examples/dort.toml`.

## Changelog maintenance

Prefer descriptive Conventional Commits. Regenerate `CHANGLOG.md` from committed history with `scripts/changelog.sh` after authorized commits; use `chore(changelog): regenerate history` for generated-only commits. Follow [release mechanics](docs/releases.md) for pinned tooling, packaging and publication.

## Validation

Use pinned Rust **1.99.1**, edition **2024** and committed `Cargo.lock`. Prefer existing dependencies and validate deliberate lockfile changes with `--locked`.

Run from the repository root. Independent Rust tests run in separate processes with private `TMPDIR` (`python3 scripts/test.py full`), which isolates fixture paths and avoids lock-descriptor inheritance between tests. Direct `cargo test` runs must keep `--test-threads=1` because same-process tests can briefly retain a sibling's fork-inherited lock descriptor until exec. Follow [validation methods](docs/validation.md) for coverage and opt-in environments. Keep checks and visual expectations meaningful; investigate failures rather than weakening gates or regenerating baselines to pass.

```sh
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
