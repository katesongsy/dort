# Exact mise installation and attempt metadata

`tools::install` is a separate opaque core capability, not an executor for `tools::install_preview` descriptions. The shared `services::installation` adapter reconstructs requests from explicit reconciliation bindings and one selected session snapshot. The CLI opts in through `tools reconcile --install-tool`; the TUI offers a dedicated review on eligible rows. Other providers and version policies remain non-executable. See [user workflows](user/commands.md#exact-mise-installation).

## Scope and approval

Only one observed missing mise package with an exact semver version, an independently missing declared binary and clear prerequisites can be reviewed. The repository must be registered. Profiles include persistent selections and explicit additions; the adapter refuses a changed reconciliation profile scope instead of silently expanding it. Only tools explicitly bound to the selected snapshot participate in that provider scope; dependencies are never automatically bound or merged across snapshots. Existing, conflicting, ambiguous, unobserved, unmanaged or blocked states do not authorize installation.

Preparation is process-free. It binds the bounded stable manifest, registration/profile configuration, host context, explicit regular executable and separate no-follow declared-name lookup directories. No candidate is executed. Public review metadata includes the selected package/version, repository/profiles, executable, provider child PATH, independent binary lookup scope, fixed arguments and deadline. Only consuming the real in-memory preview creates approval. Neither a stored attempt nor JSON metadata can recreate it.

One frontend confirmation covers fresh inspection, conditional installation and post-inspection. Execution holds the application writer lock, checks existing file-journal write guards and unresolved Tools attempts before querying, independently revalidates all bindings, and rebuilds eligibility from a fresh fixed mise query and binary metadata. A newly satisfied exact package is a no-op with no installer or attempt; other conflicts or unavailable observations refuse. Intent/head/source guards are checked again immediately before spawn. Historical session snapshots are never overwritten; installation invalidates cached reconciliation and marks retained snapshot IDs stale for later TUI install reviews. Metadata comparisons remain available, but recomputing with an old snapshot cannot re-enable installation: the view offers Collect fresh inventory, and the new snapshot must be explicitly bound. CLI install requests always collect new approved inventories. No automatic replacement or rebinding occurs.

## Fixed process and worker contract

The private runner constructs only:

```text
mise --no-config --no-env --no-hooks install --jobs=1 PACKAGE@EXACT
```

It runs at `/` with null stdin, cleared environment, explicit HOME/XDG/PATH, fixed locale and mise controls. No credentials, terminal, arbitrary argv, `force`, activation, upgrade or uninstall operation is inherited or dispatched. Unsupported flags fail without a weaker fallback. Providers/backends/package code and downloads can still have effects; this is hardening, not an OS sandbox, atomic transaction, ownership proof or rollback mechanism. An inactive installed mise version is normal and is not a PATH/readiness claim.

The installer has a displayed ten-minute deadline; inventory queries retain their independent ten-second deadline and bounded parser inputs. Concurrent nonblocking fair stdout/stderr drains discard installer bytes, including successful output; only typed phases and outcomes leave the runner. Timeout/pipe failure terminates the owned process group and joins/reaps it, possibly after partial effects. The existing single owned joined worker and single input thread remain unchanged. Quit/Ctrl-C waits for approved work; no cooperative cancellation, detached workers or automatic rollback is provided. Progress uses separate typed install events, not fabricated file-journal checkpoints.

Post-inspection is attempted after process failure as well as success. A changed source/executable prevents re-executing the changed provider. Process outcome, exact package claim and binary metadata availability remain separate. Output persistence failure is an unresolved attempt, never implied success. Reports do not claim binary readiness.

## Separate durable attempt store

`STATE/tools-attempts/<64-hex-id>/` is independent of file transactions and Recovery. Descriptor-relative no-follow private helpers create and sync immutable checksummed records:

- `intent.json`: schema 1, attempt ID, requested scope and private source fingerprints/hashes; durable before installer spawn.
- `outcome.json`: observed typed process/package/binary outcome bound to the exact intent artifact.
- `close-NNNN.json`: explicit fresh-inspection closure bound to the unchanged census/head of all earlier artifacts; append-only, never repair/overwrite an earlier record.

Records contain metadata, not raw output, manifest/configuration bodies, variable values, credentials, backups or persisted trust. There is no file-journal format change. Bounds are 256 attempts, 32 files per attempt and 64 KiB per record; exhausted or unsafe storage is refused, never silently evicted or followed through symlinks. File and directory sync establish the ordinary durability protocol; process-exit tests do not prove power-loss safety.

Missing/malformed intent, incomplete outcome, changed artifacts or invalid closure leave an unresolved attempt. Only subsequent Tools installations are blocked; unrelated file workflows are not blocked by this store. Startup never replays, cleans up, resumes or rolls back an installation.

## Explicit inspection and closure

Both frontends list all-repository attempt metadata, distinguish unavailable storage from an empty list, expose full IDs/scope/outcomes, and offer a process-free closure preview. A valid original scope requires its original executable/home/PATH; damaged or missing scope requires an explicit trusted mise executable and child PATH instead. Closure is historical metadata handling, not a current manifest operation.

One deliberate activation acquires the writer lock, checks unchanged head/host/executable bindings, runs a fresh fixed mise query, requires a complete observation and appends a head-bound closure. Unavailable queries do not close anything. A closure never retries an installer, repairs packages, verifies readiness or asserts that a damaged record was successful. Editing an earlier artifact invalidates the closure and makes the attempt unresolved again. Retry always prepares a fresh preview; no approval is replayed. File-journal problems do not prevent this separate metadata-only closure workflow.

## Validation boundaries

Disposable fake-provider fixtures cover fixed arguments/environment, output discard, source/profile/registration/executable drift, scope and prerequisite refusal, intent/outcome failure boundaries, independent file guards, unresolved-only Tools blocking, malformed/changed attempt closure, process exits, deadlines and descendant-held pipes. CLI, typed TUI/cell/caret and PTY checks exercise one-activation review, progress, joined exit, compact layouts and explicit closure. No workstation provider install, network compatibility, unfamiliar-user usability or power-loss guarantee follows from these checks. See [validation methods](validation.md).
