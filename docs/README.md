# Dort user guide

Dort helps describe a development environment in a Git repository. **The current CLI inspects and previews only:** it does not install tools, deploy files, adopt files, remove files or repair transactions.

## Start here

1. [Getting started](getting-started.md): build Dort and try a safe, disposable environment.
2. [Manifest guide](manifest.md): describe files, tools and profiles in `dort.toml`.
3. [Command reference](commands.md): available commands, output and exit codes.
4. [Safety and troubleshooting](safety.md): understand blockers, sensitive data and recovery diagnostics.

## What works today?

| Task | Available? |
| --- | --- |
| Validate a manifest and resolve profiles/dependencies | Yes: `manifest check`. |
| Discover hidden configuration metadata | Yes: `files discover`. |
| Inspect declared active files | Yes: `files list`. |
| Preview links, adoption and unmanage effects | Yes: `files plan …`; no changes are made. |
| Diagnose local transactions | Yes: `recovery list`; never replays or repairs them. |
| Install tools, deploy/adopt/unmanage files or recover through the CLI | Not yet. |
| Templates, secrets, Git clone/sync or a TUI | Not yet. |

An experimental adoption/rollback API exists for core developers, not as a supported end-user write workflow. Do not bypass CLI limits to manage your real configuration.

For project status, see [README](../README.md). Internals are in [ARCHITECTURE.md](../ARCHITECTURE.md); contributor instructions are in [AGENTS.md](../AGENTS.md). [CHANGLOG.md](../CHANGLOG.md) records committed history and is not a list of uncommitted work.
