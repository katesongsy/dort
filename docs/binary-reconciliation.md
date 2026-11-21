# Internal package and command comparison

[Check installed](declared-tools.md) gathers approved package-manager observations, associates them with declarations and calls the pure package planner. Comparison is an internal stage of that action, not a separate page, CLI command or user binding workflow. [Installed tool discovery](tools-inspection.md) keeps one independent current result for browsing and declaration seeding.

## Observation scope and provenance

`services::reconciliation::Snapshot` is an internal immutable ID and shared metadata report. It carries no approval and is neither persisted nor imported as trust. Normal checks associate only scopes approved in that check; earlier results are never a fallback. Separate providers, prefixes and Node runtimes remain separate. Internal explicit associations prevent accidental scope merging without exposing IDs or selection forms to users.

Preparation reads the bounded no-follow owned-regular-file manifest, resolves profiles and records its fingerprint/hash. Source-bearing parser/read errors are replaced with static diagnostics. The declared-tools adapter combines registered-repository persistent profile additions with requested additions and checks manifest/configuration provenance before queries, before comparison and before installation preparation.

Managed npm reports privately retain source manifest fingerprint/hash provenance, skipped by serialization. A report from another or changed declaration scope is refused; a matching runtime name cannot rebind globals. Ambiguous runtimes require independent accepted Node/npm settings. Each runtime group gets its own query scope; one group's settings never authorize another. Up to 16 scopes and 65,536 total package records are retained for a check.

These bindings do not establish live package-database freshness, executable readiness or provider ownership. Installers independently reconstruct exact scopes and revalidate fresh facts under the application writer lock. [Command descriptions](install-command-previews.md) remain non-executable planner metadata, not approvals.

## Metadata-only command observation

`dort_core::tools::binaries` observes selected declared bare binary names in separately supplied physical tool-search directories. The application captures and canonicalizes suitable PATH directories at its boundary. The core never discovers PATH, launches `--version`, follows a candidate symlink or reads/hashes candidate contents.

Access is descriptor-relative and no-follow. Directories are checked before probing; each entry is checked twice and directories are revalidated afterward. Leaf fingerprints and literal UTF-8 symlink targets are metadata. Symlinked parents/directories, unreadable or changed scopes and unsafe link targets make the affected observation unavailable, not healthy empty. The core has no path-based traversal fallback.

All occupied names remain unattributed paths, including nonexecutable files, directories, special files, hard links and dangling symlinks. Recording them never grants authority to open, execute, adopt or overwrite them. A complete empty lookup means absence only in the reviewed directories, not global absence, version satisfaction or ownership. Partial/unavailable scopes cannot justify absence.

Requests allow at most 32 distinct directories, 4,096 tools and 32,768 tool/directory probes. Directory strings are absolute UTF-8 without dot/NUL/PATH-separator components, bounded to 4,096 bytes and 64 components. Shared binary names reuse probe metadata. These are count/path bounds, not a hard filesystem deadline or isolation against same-user races.

## Presentation and validation

Users see declaration statuses, per-tool reasons/next steps and exact approval scopes. Enter opens the selected tool. Check, Install and necessary Tool settings remain named actions. Raw JSON is a CLI automation format, not the TUI details model. Installed means a matching package-manager claim; command candidates, version differences and unavailable observations remain distinct facts.

Reports contain no file bodies, raw provider output, parser source lines or configuration contents, and remain outside ordinary write receipts/journals. Dynamic metadata is escaped at terminal boundaries.

Canonical disposable fixtures cover empty/unavailable observations, all occupied file kinds, unsafe identities, no candidate execution, source changes, independent Node runtimes, no historical fallback, explicit approvals and normal brew/npm workflows. CLI/TUI/terminal coverage exercises the public Check/Install/Installed tools flows rather than manual snapshot management. These tests do not establish real-provider compatibility, sandboxing or human usability approval. See [validation methods](validation.md).
