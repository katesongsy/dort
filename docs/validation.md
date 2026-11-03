# Validation methods and test isolation

[Documentation](README.md) · [Architecture](../ARCHITECTURE.md) · [Contributor instructions](../AGENTS.md)

This is a maintained technical reference for the test infrastructure, not a release checklist, implementation plan or record of a particular candidate's results. Test counts, run IDs, temporary evidence and unfinished milestone gates belong in ignored planning notes and local artifacts. A passing earlier revision is not evidence for a changed working tree.

## Ordinary checks

From the repository root, with the pinned Rust toolchain and Python 3:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -- --nocapture
cargo build --workspace --locked
python3 scripts/check-docs.py --self-test
python3 scripts/check-docs.py
python3 scripts/test-terminal.py
git diff --check
```

The Python scripts use only the standard library. Ordinary checks do not install tools, mount volumes, fill disks or operate on real dotfiles. Cargo may fetch locked dependencies/toolchain components as needed. [CI](../.github/workflows/ci.yml) runs the ordinary gates on Ubuntu x86_64/ARM64 and macOS Intel/Apple Silicon, records platform/tool versions and requires a non-root user plus distinct Linux cross-device fixture filesystems. A configured matrix is not proof that a particular run passed.

The [documentation checker](../scripts/check-docs.py) scans maintained root Markdown and `docs/` recursively, including `docs/user/`. It validates inline local links/fragments, ignores fenced examples and rejects links to ignored specifications. It is not a general Markdown parser or external-URL checker. Keep its self-tests aligned with the documentation layout.

## Fixture and external-tool boundaries

- Use disposable canonical HOME/config/state/repository roots and explicit core `HostContext` inputs. macOS `/var` aliases need their physical path spelling.
- Never change process-global HOME/XDG in Rust tests; set isolated child environments instead. Never automate destructive real-home smoke tests or provider/package-manager execution on a workstation.
- CLI tests use deterministic fake/missing Git fixtures. Optional core real-Git tests use disposable repositories and announce missing-Git skips; examine those messages rather than treating skips as exercised coverage.
- Permission-denial tests cannot establish refusal when run as root. Linux cross-device tests need a genuinely distinct device, not a second directory on the same filesystem; unavailable fixtures must be reported explicitly.
- Ordinary reports/events/logs must remain metadata-only. Do not retain file bodies, local variable values, credentials or raw terminal buffers in evidence. Explicit escaped Git patch reports are the narrow separately requested content exception.

## Coverage and interpretation

| Area | Suites / boundary |
| --- | --- |
| Manifest/profiles/planning | Core unit/integration tests verify strict parsing, dependency/selection rules, deterministic pure plans, unsupported effects and no-op behavior. |
| Filesystem observation | [Observation](../crates/dort-core/tests/observation.rs) tests no-follow parents/terminal entries, unsafe paths, unreadable files, special files, hard links, bounds and sensitivity heuristics. Linux additionally permits real non-UTF-8/FIFO fixtures. |
| File lifecycle | [Adoption](../crates/dort-core/tests/adoption.rs) and [lifecycle](../crates/dort-core/tests/lifecycle.rs) exercise fresh approvals, backup failures/corruption, immutable intent, forward/inverse interruption, stale/edit/parent guards, source preservation, separate removal acknowledgement and repeated apply. |
| Bootstrap/recovery | [Bootstrap](../crates/dort-core/tests/bootstrap.rs) and [recovery](../crates/dort-core/tests/recovery.rs) cover schema dispatch, staged publication/registration, explicit head-bound recovery, malformed tails and refusal to overwrite edits or delete published repositories. |
| Concurrency | [Separate-process lock tests](../crates/dort-core/tests/state_process.rs), crash suites and CLI fixtures verify one writer and OS lock release. Other editors are not coordinated by the app lock. |
| Git | [Git integration](../crates/dort-core/tests/git.rs) and runner/parser tests cover unusual/unborn/conflict/worktree cases, helper/filter refusal, output/deadline bounds, explicit disclosure/escaping and unchanged input bytes/identities. |
| CLI/TUI | Command tests, pure application/view snapshots, [TUI workflows](../crates/dort/src/tui/workflow_tests.rs) and the PTY gate cover approval, dry-run, cancellation, small layouts, guarded services and terminal/worker ownership. |

Application parity tests compare shared CLI/TUI requests, reports and plans. Sequential filesystem observations are not globally atomic; plan comparisons normalize only volatile parent-directory size/timestamps/link counts, matching the core approval digest, while retaining parent identity/mode/ownership and all file observations. Fixture siblings can otherwise cause nondeterministic comparison failures. Pure state/view tests also cover profile contributions/reset, independent scopes, retained drafts, compact focused controls, raw selection identity, full-scope apply despite filters and exact-ID lifecycle forms. TUI parser errors must not disclose manifest source excerpts.

Process exits at persisted boundaries demonstrate process-crash handling, not power-loss durability. Callback errors, permission failures and exclusive-create collisions do not substitute for real ENOSPC. Checksummed intent and guards detect corruption/edits; they do not isolate a malicious same-user/root process or make multi-path operations globally atomic.

## Real terminal checks

[The PTY gate](../scripts/test-terminal.py) discovers fresh executable paths from Cargo JSON and runs isolated fixtures without an installed-Git requirement. It checks terminal/default entry, q/Ctrl-C, compact layouts, preapproval cancellation, dry-run refusal, retained text drafts/missing Git, nonpersistent repository selection, guided create/attach, repository-independent discovery/adoption, retained manual adoption, independent standalone manifest/profile checks, display-only file search/full-scope previews, exact-ID link deletion, approved apply and normal/error/panic restoration. Bootstrap fixtures inject a deterministic fake Git rather than requiring an installed executable. A supervisor retains the controlling terminal long enough to compare attributes; only the kernel PENDIN retype flag is ignored. Wait/output capture is bounded and owned fixture children are cleaned up on failure; raw screen buffers are not printed. A small bounded VT view interprets cursor-addressed differential draws for text assertions (including split escapes/UTF-8); it is not a general terminal emulator. Raw bytes are retained only within each disposable session for terminal-restoration assertions.

The terminal-session failure/panic probe exists only in isolated test children, not as a production fault switch. Preserve the approved finish-writes-before-exit policy: preapproval cancel does nothing, an approved write is never detached/cooperatively stopped/automatically undone, and errors restore the terminal and join the owned worker. Forced termination cannot promise cleanup.

## Opt-in bounded storage checks

[The storage launcher](../scripts/test-storage-vm.py) is **not an ordinary CI or workstation check**. Every mounted-volume/fault environment needs separate explicit approval and independent isolation review. Never fill a host disk, mount host data into a fault guest or reuse unrelated VMs/containers.

The current launcher is restricted to a non-root Linux user in the reviewed guest named `lima-a12`, an administrator-owned `/run/dort-approved-storage-vm` marker containing `dort-isolated-lima-a12-v1` plus LF (mode 0644), and no detected virtiofs/9p/SSHFS shares. The marker is an administrative guard, **not proof of isolation**; never create it on a workstation. It does not provision or create a VM.

Only inside a separately approved, verified disposable guest:

```sh
python3 scripts/test-storage-vm.py --approved-disposable-vm
```

The launcher builds the normally ignored Linux-only [Rust storage probe](../crates/dort-core/tests/storage_faults.rs) before mounting. It allocates a bounded 128 MiB ext4 image with 4 KiB blocks, no reserved-space percentage and delayed allocation disabled. It verifies the exact loop backing file, mount and distinct device; all fixture HOME/config/state/repository/destination paths live on that filesystem. No production fault-injection API or cross-device fallback is added.

The probe tests allocation, preparation, first durable intent, first mutation and nonempty partial-checkpoint exhaustion for adoption/apply/keep/remove and schema-3 attachment. It requires actual kernel errno 28, checks earlier checkpoint hashes and watched sources, and verifies unchanged live input identities before publication. The partial-record case reclaims only two blocks of its own filler after verifying a successful baseline checkpoint exceeds that space. Controlled geometry and reclamation are important: metadata/allocation headroom makes a single freed block unreliable for producing a nonempty partial write.

After freeing only harness-owned filler, valid preparation must recover explicitly; malformed immutable tails must refuse recovery and continue blocking new writers. Never delete/truncate journals to enable recovery. The launcher owns/reaps its process group before cleanup, unmounts without lazy/force fallback and removes only its temporary scope; failed unmount retains the scope for diagnosis. Stop the dedicated guest after use and retain reviewed metadata-only evidence separately.

This establishes only the failures actually observed on the tested revision/filesystem. It does not establish every operation boundary, new-repository Git preparation/publication, inverse ENOSPC, failing fsync, other-platform cross-device handling or real power-loss safety. Preserve those distinctions in any candidate-specific evidence record.
