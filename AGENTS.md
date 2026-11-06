# Working on Dort

These repository instructions apply to all files unless a more specific instruction overrides them.

## Scope and current capabilities

- Read [README.md](README.md) and [ARCHITECTURE.md](ARCHITECTURE.md) before changing behaviour.
- Dort is a Rust workspace: `crates/dort-core` owns UI-independent services; `crates/dort` owns the Clap CLI, Ratatui workbench and presentation.
- The experimental CLI exposes approved local init/attach, regular-file adoption, link apply, unmanage and guarded recovery. Keep discovery/checks/plans/status/history/Git inspection read-only. Do not add provider execution, cloning/sync, Git staging/commit/push or unrelated writes.
- The current TUI exposes Overview, searchable Actions/help, session repository/profile selection, guided create/attach, Files discovery/adoption/apply/unmanage, structural Checks with host Diagnostics/About, searchable all-repository Recovery with contextual action hints, and read-only Git status/diff with separately confirmed patch viewing; it remains experimental. The accepted v0.1 boundary is **TUI-first, polished and complete for every supported CLI workflow**. The interim parity/UX roadmap belongs in ignored `specs/`, not tracked documentation. Before release, complete usability review and exact-candidate validation. Preserve keyboard-only compact/wide navigation, textual (not color-only) state, retained drafts and inspectable escaped metadata. Keep typed Files/review presentation separate from opaque approval ownership; cell-style/caret/focus assertions supplement text and PTY checks. Enter on a discovered row inspects it; candidate adoption is a separate action. Automated state/view/PTY checks do not establish unfamiliar-user intuitiveness. Keep doctor metadata shared across frontends, diagnostics content-free, and unavailable observations distinct from healthy empty results. Recovery eligibility hints never replace core previews; failed-write retries must not retain/replay approvals. No supported task may be completed by a CLI handoff; JSON/automatic-approval flags remain CLI automation concerns. Do not add provider execution or change journal/schema/security/cancellation contracts without explicit agreement. The separately approved TUI patch flow requires a deliberate content warning and literal `view patch` once per view; do not broaden it into automatic content viewing.
- Keep existing uncommitted work intact. Do not commit, push, tag or release unless requested.

## Required documentation maintenance

Update documentation in the same change as the behaviour it describes:

| File | Maintain when |
| --- | --- |
| `README.md` | Status, build requirements, navigation, commands, limitations or roadmap change. |
| `ARCHITECTURE.md` | Module boundaries, data flow, persistence/schema contracts, safety guarantees or architectural limitations change. |
| `docs/user/` | End-user commands, flags, manifests, workflows, output/exit codes, safety or troubleshooting change. |
| `docs/` | Durable technical notes about the current architecture, contracts, internals, validation methods or other maintained system facts change. |
| `AGENTS.md` | Contributor workflow, tooling, validation or standing project constraints change. |
| `CHANGLOG.md` | Authorized commits add history; regenerate with git-cliff, never hand-edit generated entries. |

- `docs/user/` is tracked **end-user-only** documentation: installation/build-for-use, current CLI/TUI usage, configuration, workflows, safety and troubleshooting. Do not include contributor instructions, test infrastructure, development status, slice IDs, feature plans, roadmaps or release checklists there. Keep examples usable and distinguish previews from execution.
- `docs/` (outside `docs/user/`) holds the documentation index and tracked **durable technical references**. Describe the current system and keep notes maintained; do not store temporary feature designs, implementation schedules, unfinished task lists, candidate-specific test counts/run IDs or release worklists there. Root `README.md`/`ARCHITECTURE.md`/`AGENTS.md` remain maintained project entry points.
- `specs/` holds **ignored, transient feature specifications, interim decisions/designs, roadmaps, implementation plans and candidate-specific release/evidence checklists**. These can become stale or be discarded after the feature is done; do not keep them in source control. Never require/link them from tracked docs, code, tests or builds.
- After implementation, promote only enduring, implemented technical facts into `docs/`/`ARCHITECTURE.md` and user-facing behavior into `docs/user/`; do not copy completed task lists into maintained documentation. Move obsolete planning notes out of public docs rather than retaining them as a public archive.
- Do not describe planned features as shipped. Every later CLI capability needs a usable TUI workflow in the same milestone. State local validation separately from unobserved remote CI, recording transient evidence in ignored planning/artifact locations.
- Scan maintained Markdown recursively, including `docs/user/`, with the documentation checker. Moving a page requires updating relative links in root documents, nested guides and local planning notes; tracked pages must never link into ignored `specs/`.
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
python3 scripts/check-docs.py --self-test
python3 scripts/check-docs.py
python3 scripts/test-terminal.py
git diff --check
```

The Python gates use the standard library; terminal tests discover fresh Cargo artifacts and operate only on disposable fixtures. CI runs all four runtime platforms, exposes optional skip messages, requires non-root permission tests and distinct Linux cross-device fixtures, and rejects tracked-file changes. See [validation methods](docs/validation.md) for maintained coverage, isolation and harness guidance; candidate-specific evidence and remaining release blockers belong in ignored planning notes. Do not equate an earlier commit's CI result with validation of new work. Real ENOSPC/power-loss or mounted-volume fault injection needs a separately approved disposable environment; never fill a workstation disk or automate real-home smoke tests. `scripts/test-storage-vm.py` is an opt-in launcher restricted to the reviewed `lima-a12` guest with an administrative approval marker, no host shares and a verified 128 MiB ext4 loop image. Its Linux Rust probe is ignored by ordinary tests. Do not create the marker on a workstation, add privileged fault injection to normal CI, enlarge the volume or infer failing-sync/power-loss safety from ENOSPC results.

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
- Recovery is explicit and guarded. Never automatically replay/rollback on startup or overwrite later user/source edits. Schema-3 bootstrap is separate from schema-1/2 lifecycle journals: private staged Git initialization, durable no-replace publication/registration intents, explicit guarded resume/abandon only. Abandon retains published repositories, registration and private artifacts; never delete a published repository automatically.
- Mutation CLI commands must present previews and collect human-terminal confirmation or explicit --yes. JSON/nonterminal writes never prompt. Link deletion additionally needs a separate exact-ID acknowledgement. --dry-run must not create directories/state/locks/backups/journals or prompt. Keep approvals opaque and bind fresh previews/journal heads under the writer lock.
- TUI approvals must consume the real in-memory core preview, never serialized plans. Adoption alone uses one explicit Adopt all activation (key/click) against the real combined preview, without typed yes. Other writes retain typed confirmation. Keep separate exact-ID then literal-yes confirmation for removal; blocked/dry-run/small layouts cannot approve. Entry --dry-run cannot be toggled off; shrinking clears both typed yes and removal acknowledgement. Session root selection must remain read-only; only approved bootstrap persists registration/selection. Keep form drafts in memory, accept printable shortcuts as text there, and resolve relative bootstrap paths from a caller-supplied launch directory. q/Ctrl-C during a worker waits for completion; no cooperative write cancellation, detached worker or automatic rollback. Restore the terminal on returns/errors/panics and join active workers before exit. Keep Crossterm input on one thread and progress nonblocking/metadata-only. Test with Ratatui's backend and disposable fixtures; terminal failure/panic injection belongs only in isolated test children.
- Preserve the current same-filesystem atomic-rename constraint; do not add an unsafe cross-device fallback.
- Do not put file contents, local variable values or secrets in ordinary reports, logs or checkpoint metadata. Discover may automatically disclose only its selected eligible regular file through the bounded no-follow core read service (64 KiB; sensitive/binary refusal; no unselected prefetch). Keep this ephemeral payload outside Debug/Serialize, generic metadata models, logs/errors/receipts/history/export; discard on selection/scope/view/shrink/exit and withhold in preview-only/below-floor layouts. Heuristics are not secret-proof; discarding a reference is not secure erasure. Passive reads share the one owned joined worker without blocking navigation or opening activity panels. The separate Git exception remains explicit `git diff --patch`: bounded patch content is disclosed only in that requested report, with unsafe bytes escaped, never in logs/errors/journals. Summary/status reports stay metadata-only; patches are not secret-proof. TUI patch payloads must remain private, outside Debug/Serialize/generic diagnostics/progress/view-model dumps. No prefetch, automatic refresh, clipboard/export or retained patch history; dismissal/scope change/shrink/exit drops the reference, not a guarantee of secure memory erasure. Disclosure confirmation is disabled below 48×14 and is separate from mutation approval. Backups are private but ordinary backups are not encrypted.
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
