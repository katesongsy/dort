# Documentation

## End-user guides

Start with the [user guide](user/README.md):

- [Getting started](user/getting-started.md): install and try Dort with a disposable environment.
- [Commands](user/commands.md): current CLI/TUI behavior, options and exit codes.
- [Manifest](user/manifest.md): configuration syntax and profile selection.
- [Safety and troubleshooting](user/safety.md): limits, approvals and recovery.

`docs/user/` is for end users only. It describes available behavior and usable workflows, not development tasks, implementation plans, test infrastructure or release checklists.

## Durable technical references

- [Architecture](../ARCHITECTURE.md): implemented module boundaries, execution, persistence and recovery contracts.
- [Provider inspection](tools-inspection.md): shared CLI/TUI approval, process, metadata and runtime-binding boundaries.
- [Snapshot reconciliation](binary-reconciliation.md): explicit multi-snapshot binding, no-follow candidate metadata and frontend scope/freshness boundaries.
- [Non-executable command previews](install-command-previews.md): fixed argv/control descriptions, native side-effect caveats and no execution authority.
- [Pure tool package planning](tools-planning.md): explicit-scope claims, binary uncertainty, version/no-op semantics and dependency blockers.
- [Validation methods](validation.md): test isolation, repeatable checks, terminal/storage harnesses and what their results mean.
- [Release mechanics](releases.md): native artifacts, provenance and tag-triggered publication guards.
- [Contributor instructions](../AGENTS.md): workflow, safety constraints and documentation maintenance.

Technical notes here describe the current system and must be maintained when it changes. Feature specifications, roadmaps, temporary design decisions, implementation plans and candidate-specific release/evidence checklists belong in ignored `specs/`, not in source-controlled documentation. Tracked documentation must stand on its own without those local planning files.
