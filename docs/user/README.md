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
| Inspect retained transaction history/integrity | Yes: `history` / `recovery list`, all five journal formats, no replay. |
| Create/attach a local repository | Experimental: interactive Actions → Create/attach, or `init PATH`. |
| Adopt/deploy/unmanage regular-file links | Experimental: `files add`, `files apply`, `files remove`. |
| Guarded rollback / bootstrap recovery | Experimental: `rollback`, `recovery resume` / `abandon`. |
| Inspect provider-reported installed packages | Yes: Tools or `tools inspect`; [explicit approval and config/cache risks](commands.md#provider-inspection). No ownership inference. |
| Reconcile declarations against explicit snapshots | Yes: Tools `s`/`p` or `tools reconcile`; [scope and binary-metadata limits](commands.md#snapshot-reconciliation). Reconciliation alone does not install or verify tool readiness. |
| Inspect proposed install argv/effects | Eligible reconciliation rows show preview-only command metadata; no command can be run by Dort. |
| Install one missing supported package | Tools comparison → Review install or `tools reconcile --install-tool`; [exact mise](commands.md#exact-mise-installation), [exact npm](commands.md#exact-npm-installation), [provider-current Homebrew core](commands.md#homebrew-core-installation), deadline and attempts. npm/Homebrew links may expose commands; Homebrew build/dependency code can execute and dependencies may upgrade. No rollback. |
| Add desired tool declarations | Tools → Add tool declaration or `tools declare`; edits the manifest only, with guarded recovery. |
| Install tools or Git staging/commit/push | Not yet. |
| Interactive onboarding, Files, profiles/checks/diagnostics, Recovery, Tools and read-only Git | Experimental: terminal `dort` or `dort tui`; [keys and limitations](commands.md#interactive-interface). |
| Templates, secrets or Git clone/sync | Not yet. |

Use `--dry-run` to inspect effects without writes, locks or prompts. Provider inspection needs an explicit Run query action, CLI `y/N` confirmation or `--yes`. Writes require terminal confirmation or `--yes`; JSON/nonterminal writes always need `--yes`. Deletion confirmation identifies the exact selected mapping; no typing challenge is required. These sandbox-tested prototypes are not a production deployment guarantee.

See the [project overview](../../README.md) and [release history](../../CHANGLOG.md) for more information.
