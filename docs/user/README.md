# Dort user guide

Dort keeps your development environment in a Git repository: dotfiles, profiles and the tools you expect. It works on macOS, Linux and WSL.

Dort is experimental. Start with a disposable HOME. Do not point it at real dotfiles until you understand the workflows.

## Start here

1. [Getting started](getting-started.md): install Dort and try a complete workflow in a temporary HOME.
2. [Manifest guide](manifest.md): describe files, tools and profiles in `dort.toml`.
3. [Command reference](commands.md): keys, commands, output and exit codes.
4. [Safety and troubleshooting](safety.md): approvals, recovery, common blockers and state locations.

Upgrading? See [Upgrade from v0.1.0](manifest.md#upgrade-from-v010).

## What works

| Task | Use |
| --- | --- |
| Validate a manifest | `manifest check`, Checks in the TUI |
| Discover and manage dotfiles | `files discover`, `list`, `plan`, `add`, `apply`, `remove` |
| Create or attach a local repository | `init PATH`, Create/attach |
| Inspect Git status and diff | Git page shows patch previews; CLI uses `git status` and `git diff --patch` |
| Check installed tools | Tools **Check installed** (`C`), `tools check` |
| Install missing Homebrew or npm tools | Tools **Install missing** (`i`), `tools install-missing` |
| Browse installed packages | Tools **Installed** (`2`), `tools inspect` |
| Inspect install history and close old records | Tools **History** (`3`), `tools attempts`, `tools close-attempt` |
| Add, edit or remove declarations | Tools `a` / `e` / `d`, `tools declare` / `edit` / `remove` |
| Inspect or recover interrupted writes | `history`, `recovery list`, Recovery |

Not implemented: templates, secrets, Git clone/sync, staging/commit/push, and general package upgrades.

Every write needs a preview and confirmation. `--dry-run` previews without writing. Run `dort --help` for command syntax.
