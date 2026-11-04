# Dort user guide

Dort helps describe a development environment in a Git repository. The experimental CLI supports local initialization, regular-file adoption/link deployment, unmanage and guarded recovery after explicit approval. The interactive interface provides searchable Actions/help, guided create/attach, session repository/profiles, Files discovery/adoption/apply/unmanage, Checks/Diagnostics, all-repository Recovery and read-only Git over the same services. **Start with a disposable HOME**, not real dotfiles. Tool execution, templates, secrets and remote Git workflows are not implemented.

## Start here

1. [Getting started](getting-started.md): build Dort and follow a TUI-only walkthrough in a disposable HOME.
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
| Create/attach a local repository | Experimental: interactive Actions → Create/attach, or `init PATH`. |
| Adopt/deploy/unmanage regular-file links | Experimental: `files add`, `files apply`, `files remove`. |
| Guarded rollback / bootstrap recovery | Experimental: `rollback`, `recovery resume` / `abandon`. |
| Install tools or Git staging/commit/push | Not yet. |
| Interactive onboarding, Files, profiles/checks/diagnostics, Recovery and read-only Git | Experimental: terminal `dort` or `dort tui`; [keys and limitations](commands.md#interactive-interface). |
| Templates, secrets or Git clone/sync | Not yet. |

Use `--dry-run` to inspect effects without writes, locks or prompts. Writes require terminal confirmation or `--yes`; JSON/nonterminal writes always need `--yes`. Destructive link deletion needs a separate exact-ID acknowledgement. These sandbox-tested prototypes are not a production deployment guarantee.

See the [project overview](../../README.md) and [release history](../../CHANGLOG.md) for more information.
