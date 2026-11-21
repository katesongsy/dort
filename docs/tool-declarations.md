# Tool declaration transactions

Tools opens declarations across all profiles; Enter opens the selected tool's command, package, version and dependency details. Check/Install remain named actions. Add declaration is an initially unfocused [navigable form](tui-interactions.md): Tab begins editing and Esc stops editing before going back. The form has ID, command, profiles, prerequisites, provider, package, version and npm-runtime fields. Only relevant provider controls participate in traversal; Space chooses a provider and F4 reveals dependencies. Provider drafts, including empty values, are retained independently. Enter prepares the process-free preview; its single Confirm action consumes the exact changes once. Human before/desired metadata replaces raw JSON and alternate F3 modes. Done returns to the invoking declaration list or Installed tools parent. Menu dismissal retains drafts/reviews; actual navigation drops approval.

A package in the current Installed tools result can seed a separate retained draft. The reported provider and package seed a single platform-independent install specification. npm may seed an observed version and a known desired runtime, derived from the reviewed scope or the selected manifest's unambiguous declaration hint. A standalone inventory has no managed runtime binding. The user supplies the desired tool ID and bare command name; neither command identity nor ownership is inferred. Known-metadata draft keys keep these alternatives separate from manual input, bounded to 128. Seeding and saving never rerun inspection or install packages. A separate real manifest preview/approval remains required; no snapshot ID or history selection is exposed.

Tool declarations are desired metadata. CLI `tools declare`, `tools edit` and `tools remove`, and the TUI Add / Edit / Remove actions share `services/declaration.rs` and the core `tools::declaration` service. On the declaration list or selected-tool details, `e` opens a retained prefilled edit form and `d` prepares removal. Tool IDs stay fixed during editing. Each change has one exact preview approval; installed packages stay untouched. Only an already registered repository is eligible. No provider, candidate binary or Git process is dispatched.

## Preview and focused edit

The input is one typed `model::Tool`, bounded to 32 KiB of encoded metadata. The install specification, profiles, prerequisites and npm runtime bindings use the existing manifest validation rules; at most 4096 tool declarations are supported. Frontends combine persistent profile additions with session/request additions. The core does not discover environment values.

The core securely reads the owned, single-link, no-follow, bounded manifest. Original TOML parser errors are content-free. A pure `toml_edit` transformation adds, replaces or removes one tool in an array of tables or inline array, preserving unrelated formatting/comments. Additions also support an absent `tools` key. Imported table positions are cleared. The result must parse to exactly the expected complete manifest with only the selected change. Identical additions/edits are no-ops; conflicting additions and unknown edit/remove IDs are refused. Full manifest validation rejects changes that invalidate tool prerequisites, file prerequisites or npm runtime references. Removal never cascades.

The real in-memory preview contains the operation, typed declaration metadata and paths, not manifest bytes. Edit/remove previews also show the original typed declaration. Its private binding covers host context, repository/registration, profile scope, original manifest identity/hash and local configuration identity. `Approval::from_preview` consumes and checks this preview. Neither preview serialization nor an imported plan grants authority. Preview and approval have no Debug implementation.

The TUI retains its editable platform-independent draft across dismissal/navigation. The preview owns the shared Confirm changes action; Enter or a click consumes the real approval once, without typed challenges. Preparation and execution use the existing owned, joined worker. Dry-run and below-floor layouts cannot approve; shrinking clears approval and requires fresh review input on a sufficiently large layout. A failed write discards approval and can only retry preparation from the retained draft.

## Manifest-only schemas 5 and 6

The separate `tool_journal` wire format does not reinterpret adoption or lifecycle operations. Its immutable header records repository/transaction IDs, approved-preview digest, Tool metadata, profiles, original manifest metadata/hash, resulting manifest digest, parent anchors and local configuration identity. Additions retain the schema-5 wire format. Schema 6 adds an explicit edit/remove operation; schema/operation pairing and schema stability are validated throughout the checkpoint chain. Existing schema-5 raw payloads remain unchanged. It stores no raw manifest bytes, provider receipts or binary ownership claims.

Execution checks the writer lock and global transaction diagnosis, independently obtains a fresh preview and compares the complete binding. No-ops allocate no transaction or backups. A changing request proceeds through:

1. Allocate a private transaction directory and persist a checksummed preparing intent.
2. Create and sync the private `manifest.backup`, verifying its original digest.
3. Require an owned manifest parent on the transaction filesystem. Prepare and sync `op-0000` with the original ordinary permissions/group.
4. Persist running/pending, then mutation intent. Revalidate identities, hashes, registration, configuration, parents and backups before descriptor-relative atomic exchange.
5. Verify the result, persist done, then verifying/completed checkpoints.

There is no path-based replacement or cross-device fallback. Checkpoints are immutable checksum-linked envelopes with bounded reads, consecutive sequence numbers, stable headers, validated phase/progress transitions and a fixed artifact set. Missing/truncated/malformed intent, invalid tails and corrupted required backups block subsequent writers. Schemas 1–4 keep their independent parsers/executors; there is no migration.

## Explicit recovery

The existing all-repository diagnosis dispatches declaration schemas 5 and 6 separately from other transaction families. CLI rollback and TUI Recovery request a fresh core preview that binds the current journal head. Eligibility hints do not grant approval. Under the writer lock, recovery rechecks that binding and scope before proceeding.

A preparing transaction has no live mutation and can be marked rolled back, retaining private artifacts. Otherwise recovery validates both live and staged manifest identities/content, unchanged parents/configuration/registration and the durable backup. A completed result is restored only if it remains unchanged. An intent interrupted around exchange is recognized from exact before/after identities; inverses persist undo-intent before exchange and undone afterward. Recovery is restartable, never automatic, and refuses later edits rather than overwriting them. It does not undo package operations because none occurred.

Raw manifest bytes exist only in short-lived private execution buffers and private durable artifacts. They are excluded from Debug, reports, ordinary logs, journal metadata and progress. Backups are private, not encrypted. Manifest edits invalidate npm inspection provenance; existing session snapshots are not silently rebound or refreshed.

## Validation

Canonical disposable fixtures cover registration, focused additions/edits/removals, unrelated comments and permissions, table/inline arrays, no-ops, conflicts, invalid references, malformed inputs, tampered/stale previews, unsafe files, later edits, backup/tail corruption, every exposed forward/inverse failure boundary and process exits followed by explicit recovery for both declaration schemas. CLI and Ratatui state/view fixtures cover the same declaration/recovery services, retained drafts, compact/wide focus/caret/styles, dry-run, shrink, late preparation and fresh retry. Process-exit tests establish restart behavior, not power-loss safety or real-provider compatibility.
