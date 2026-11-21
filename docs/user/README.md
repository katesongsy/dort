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
| Inspect retained transaction history/integrity | Yes: `history` / `recovery list`, all supported journal formats, no replay. |
| Create/attach a local repository | Experimental: interactive Actions → Create/attach, or `init PATH`. |
| Adopt/deploy/unmanage regular-file links | Experimental: `files add`, `files apply`, `files remove`. |
| Guarded rollback / bootstrap recovery | Experimental: `rollback`, `recovery resume` / `abandon`. |
| Inspect installed packages and seed a declaration | Tools → Installed tools or `tools inspect`; [explicit query approval](commands.md#installed-tool-discovery), one current result and no ownership inference. |
| Check declared tools | Tools **Check installed** (`C`) or `tools check`; [derived scopes and per-tool status](commands.md#declared-tool-workflow). |
| Review exact query/install effects | Human-readable reviews show programs, arguments, destinations and risks before approval. |
| Install missing supported packages | Tools **Install missing** (`i`) or `tools install-missing`; [exact npm and provider-current Homebrew](commands.md#installation-safety), serial stop-on-failure, durable attempts and no rollback. |
| Add, edit or remove desired declarations | Tools `a` / `e` / `d`, or CLI `tools declare` / `edit` / `remove`; manifest-only changes with guarded recovery. |
| General package upgrades, runtime configuration or Git staging/commit/push | Not yet. |
| Interactive onboarding, Files, profiles/checks/diagnostics, Recovery, Tools and read-only Git | Experimental: terminal `dort` or `dort tui`; [keys and limitations](commands.md#interactive-interface). |
| Templates, secrets or Git clone/sync | Not yet. |

Use `--dry-run` to inspect effects without writes, locks or prompts. Provider inspection needs an explicit Run query action, CLI `y/N` confirmation or `--yes`. Writes require terminal confirmation or `--yes`; JSON/nonterminal writes always need `--yes`. Deletion confirmation identifies the exact selected mapping; no typing challenge is required. These sandbox-tested prototypes are not a production deployment guarantee.

See the [project overview](../../README.md) and [release history](../../CHANGLOG.md) for more information.
