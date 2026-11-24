# Safety and troubleshooting

[User guide](README.md) · [Command reference](commands.md)

Dort is experimental. Start with a disposable HOME. Review every preview before approving a write.

## Approvals

Reads, checks, plans and status do not write. Writes need an in-memory preview and confirmation. `--yes` is the noninteractive approval.

One preview creates one approval. Navigation, resizing below 48×14, or a failed write discards it. Choose the action again for a fresh preview.

`--dry-run` never writes, locks, prompts or queries package managers.

## Sensitive content

Dort reports names, paths and package facts. File bodies stay unprinted.

Two deliberate exceptions:

- The Git page previews changes to the selected file. Patches can contain secrets. Review privately.
- On the command line, use `dort git diff --patch` to print patch content.
- In Discover, the selected regular file can be shown in a bounded preview.

Review both privately. Do not share screenshots or logs with secrets. Discovery exclusions are heuristics, not a guarantee.

Do not put plaintext secrets in the repository, examples or command lines. Secret deployment is not available.

## Tool checks and installs

Package-manager queries can run package-manager code, read configuration and change caches. Installs can change shared files outside the repository.

- **Homebrew** uses the current core formula, not an exact pin. Build and dependency code may run. Dependencies may change. Links can appear on PATH.
- **npm** installs an exact package into the selected global prefix. Lifecycle scripts are disabled. Links and dependencies can still change shared state.
- Dort does not edit PATH or runtime configuration.
- There is no sandbox, automatic rollback or readiness guarantee.

Homebrew needs its prefix folders to exist and be writable by you. World-writable folders and folders writable by groups you are not in are refused. Homebrew's usual admin group is accepted when it is one of your groups.

npm needs a global prefix inside HOME, separate from Node and the npm CLI. Choose another destination in Tool settings if needed. Dort does not silently move it.

Quit waits for the active install. A timeout or failure can leave partial effects.

## Recovery and blocked writes

A failed or interrupted write keeps a private record. New writes may be blocked until you inspect it.

- Run `dort recovery list` or open Recovery.
- Open the affected record. Dort names the repository and operation.
- Review the offered action: rollback, resume or abandon.
- Never delete recovery state to bypass a warning.

Recovery rechecks current files and refuses changed state. It never replays automatically. A completed operation can still be rolled back if its files still match.

File and install history are separate. Closing an install record acknowledges it. Retry, repair and rollback stay manual.

## Terminal and interruption

Normal exit, handled errors and panics restore the terminal. Forced termination cannot.

If the terminal is broken, run `stty sane` and reopen it. Then inspect `dort history`.

`q` and `Ctrl-C` wait for active work. They do not cancel an approved write. No worker is detached.

## Where state lives

macOS, Linux and WSL use:

- Configuration: `${XDG_CONFIG_HOME:-$HOME/.config}/dort/`
- State: `${XDG_STATE_HOME:-$HOME/.local/state}/dort/`
- Repository records: `state/repositories/<id>/`
- Tool install records: `state/tools-attempts/`

Backups are private, not encrypted. Do not share state directories blindly.

## Common blockers

| Message or symptom | Meaning | Next step |
| --- | --- | --- |
| Git not found | Git is required for repository and Git workflows | Install Git and put it on PATH |
| HOME not safe | Target is HOME, Dort state/config, the repository or an overlapping path | Choose another target |
| Existing link or file | Adoption and apply refuse unexpected files | Inspect the path and keep, remove or move it deliberately |
| Unsafe parent or permissions | A parent is a symlink, not writable or owned by another account | Fix ownership/permissions or choose another path |
| Package manager not found | Detection found no usable program | Open Tool settings and set the exact program path |
| Homebrew folder refused | A required folder is missing or writable by others | Fix the folder, or choose another Homebrew prefix |
| npm prefix refused | The prefix is outside HOME or too close to Node/the CLI | Choose a safe global prefix |
| Installation needs attention | No tool can be installed in this review | Open the reason and fix it; check again |
| Changes blocked | An earlier operation is unfinished or its record cannot be verified | Open Recovery |
| Version differs | The installed version does not match the declaration | Edit the declaration or leave it; Dort does not silently upgrade |

## Reporting a problem

Include:

- Dort version from `dort --version`.
- Host, platform and terminal size.
- The command or TUI path.
- The visible error text.
- Relevant `--dry-run` or read-only output.

Remove paths, tokens and secrets before sharing. Do not attach state directories or full patches.
