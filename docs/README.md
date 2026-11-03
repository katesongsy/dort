# Documentation

## End-user guides

Start with the [user guide](user/README.md):

- [Getting started](user/getting-started.md): build and try Dort with a disposable environment.
- [Commands](user/commands.md): current CLI/TUI behavior, options and exit codes.
- [Manifest](user/manifest.md): configuration syntax and profile selection.
- [Safety and troubleshooting](user/safety.md): limits, approvals and recovery.

`docs/user/` is for end users only. It describes available behavior and usable workflows, not development tasks, implementation plans, test infrastructure or release checklists.

## Durable technical references

- [Architecture](../ARCHITECTURE.md): implemented module boundaries, execution, persistence and recovery contracts.
- [Validation methods](validation.md): test isolation, repeatable checks, terminal/storage harnesses and what their results mean.
- [Contributor instructions](../AGENTS.md): workflow, safety constraints and documentation maintenance.

Technical notes here describe the current system and must be maintained when it changes. Feature specifications, roadmaps, temporary design decisions, implementation plans and candidate-specific release/evidence checklists belong in ignored `specs/`, not in source-controlled documentation. Tracked documentation must stand on its own without those local planning files.
