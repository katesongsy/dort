# Tool declaration transactions

Tool declarations are desired metadata, not package installation, installed-binary adoption or provider ownership. CLI `tools declare` and Tools → Add tool declaration share `services/declaration.rs` and the core `tools::declaration` service. Only an already registered repository is eligible. No provider, candidate binary or Git process is dispatched.

## Preview and focused edit

The input is one typed `model::Tool`, bounded to 32 KiB of encoded metadata. Both platforms, profiles, prerequisites and npm runtime bindings use the existing manifest validation rules; at most 4096 tool declarations are supported. Frontends combine persistent profile additions with session/request additions. The core does not discover environment values.

The core securely reads the owned, single-link, no-follow, bounded manifest. Original TOML parser errors are content-free. A pure `toml_edit` edit appends to an absent `tools` key, array of tables or inline array, preserving unrelated formatting/comments. Imported table positions are cleared. The result must parse to exactly the expected complete manifest, with only the new typed Tool added. An identical declaration is a no-op; a conflicting ID is refused. Updates and removals are not supported.

The real in-memory preview contains typed declaration metadata and paths, not manifest bytes. Its private binding covers host context, repository/registration, profile scope, original manifest identity/hash and local configuration identity. `Approval::from_preview` consumes and checks this preview. Neither preview serialization nor an imported plan grants authority. Preview and approval have no Debug implementation.

The TUI retains its editable multi-platform draft across dismissal/navigation. Review opens the shared Confirm action; Enter or a click consumes the real approval once, without typed challenges. Preparation and execution use the existing owned, joined worker. Dry-run and below-floor layouts cannot approve; shrinking dismisses confirmation. A failed write discards approval and can only retry preparation from the retained draft.

## Manifest-only schema 5

The separate `tool_journal` wire format does not reinterpret adoption or lifecycle operations. Its immutable header records repository/transaction IDs, approved-preview digest, Tool metadata, profiles, original manifest metadata/hash, resulting manifest digest, parent anchors and local configuration identity. It stores no raw manifest bytes, provider receipts or binary ownership claims.

Execution checks the writer lock and global transaction diagnosis, independently obtains a fresh preview and compares the complete binding. No-ops allocate no transaction or backups. A changing request proceeds through:

1. Allocate a private transaction directory and persist a checksummed preparing intent.
2. Create and sync the private `manifest.backup`, verifying its original digest.
3. Require an owned manifest parent on the transaction filesystem. Prepare and sync `op-0000` with the original ordinary permissions/group.
4. Persist running/pending, then mutation intent. Revalidate identities, hashes, registration, configuration, parents and backups before descriptor-relative atomic exchange.
5. Verify the result, persist done, then verifying/completed checkpoints.

There is no path-based replacement or cross-device fallback. Checkpoints are immutable checksum-linked envelopes with bounded reads, consecutive sequence numbers, stable headers, validated phase/progress transitions and a fixed artifact set. Missing/truncated/malformed intent, invalid tails and corrupted required backups block subsequent writers. Schemas 1–4 keep their independent parsers/executors; there is no migration.

## Explicit recovery

The existing all-repository diagnosis dispatches schema 5 separately. CLI rollback and TUI Recovery request a fresh core preview that binds the current journal head. Eligibility hints do not grant approval. Under the writer lock, recovery rechecks that binding and scope before proceeding.

A preparing transaction has no live mutation and can be marked rolled back, retaining private artifacts. Otherwise recovery validates both live and staged manifest identities/content, unchanged parents/configuration/registration and the durable backup. A completed result is restored only if it remains unchanged. An intent interrupted around exchange is recognized from exact before/after identities; inverses persist undo-intent before exchange and undone afterward. Recovery is restartable, never automatic, and refuses later edits rather than overwriting them. It does not undo package operations because none occurred.

Raw manifest bytes exist only in short-lived private execution buffers and private durable artifacts. They are excluded from Debug, reports, ordinary logs, journal metadata and progress. Backups are private, not encrypted. Manifest edits invalidate npm inspection provenance; existing session snapshots are not silently rebound or refreshed.

## Validation

Canonical disposable fixtures cover registration, focused edits/comments and permissions, repeated/inline additions, no-ops, conflicts, malformed inputs, tampered/stale previews, unsafe files, later edits, backup/tail corruption, every exposed forward/inverse failure boundary and process exits followed by explicit recovery. CLI and Ratatui state/view fixtures cover the same declaration/recovery services, retained drafts, compact/wide focus/caret/styles, dry-run, shrink, late preparation and fresh retry. Process-exit tests establish restart behavior, not power-loss safety or real-provider compatibility.
