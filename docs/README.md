# Dort user guide

Dort helps describe a development environment in a Git repository. The experimental CLI supports local initialization, regular-file adoption/link deployment, unmanage and guarded recovery after explicit approval. A minimal interactive interface provides Overview, active Files, previews/approval/progress and Recovery over the same services. **Start with a disposable HOME**, not real dotfiles. Tool execution, templates, secrets and remote Git workflows are not implemented.

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
| Inspect Git status/diff | Yes: metadata-only `git status` / `git diff`; explicit `--patch` discloses bounded escaped content. |
| Inspect retained transaction history/integrity | Yes: `history` / `recovery list`, all three journal formats, no replay. |
| Create/attach a local repository | Experimental: `init PATH`. |
| Adopt/deploy/unmanage regular-file links | Experimental: `files add`, `files apply`, `files remove`. |
| Guarded rollback / bootstrap recovery | Experimental: `rollback`, `recovery resume` / `abandon`. |
| Install tools or Git staging/commit/push | Not yet. |
| Interactive Overview, Files/apply/unmanage and Recovery | Experimental: terminal `dort` or `dort tui`; [keys and limitations](commands.md#interactive-interface). |
| Templates, secrets or Git clone/sync | Not yet. |

Use `--dry-run` to inspect effects without writes, locks or prompts. Writes require terminal confirmation or `--yes`; JSON/nonterminal writes always need `--yes`. Destructive link deletion needs a separate exact-ID acknowledgement. These sandbox-tested prototypes are not a production deployment guarantee.

For project status, see [README](../README.md). Internals are in [ARCHITECTURE.md](../ARCHITECTURE.md); contributor instructions are in [AGENTS.md](../AGENTS.md). [CHANGLOG.md](../CHANGLOG.md) records committed history and is not a list of uncommitted work.
