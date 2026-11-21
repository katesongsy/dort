# Repository create and attach

`dort init` creates a private staged Git repository at an absent path, or attaches an existing Git working tree that already has a valid `dort.toml`. It registers the repository and makes it the selected one. Clone and sync stay manual.

## Sub-features

- **Create**: absent path → staged Git repo plus a minimal manifest.
- **Attach**: existing Git tree with `dort.toml` → validate, register, select.
- **Register and select**: writes `$XDG_CONFIG_HOME/dort/local.toml`.
- **Overview** (`o`): repository path, profiles, file counts, history blockers.
- **Repository details** (`F3`) and **Profiles** (`T`).

## How to get to it (user POV)

First run with no repository lands on an onboarding state: "Start with a repository", with Create or attach as the primary action. After one exists, `o` shows Overview and `F2` More still offers Create or attach.

CLI:

```sh
dort init /path/to/new-repository
dort --yes init /path/to/existing-repository
```

## Driving it with harness

`drive.py new` does this for you and records the preview and the result:

```sh
python3 .pi/skills/verify-dort/drive.py new my-proof
python3 .pi/skills/verify-dort/drive.py tui my-proof --label overview \
  "wait:loaded." "snap:overview"
```

Attaching an existing tree by hand:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
mkdir -p "$HOME/existing" && git -C "$HOME/existing" init -q && printf 'schema=1\n' > "$HOME/existing/dort.toml"
dort --yes init "$HOME/existing"
```

## Observable end state

- `$XDG_CONFIG_HOME/dort/local.toml` names the repository and its id.
- `$DORT_REPO/dort.toml` exists and contains `schema=1`.
- `$DORT_REPO/.git` exists.
- A transaction directory exists under `$XDG_STATE_HOME/dort/repositories/<id>/transactions/`.
- The TUI Overview shows `Read-only data loaded.` and the repository path.

## Gotchas

- **Previews print before the write.** `dort init` without `--dry-run` prints the preview object, then does the work. Exit `2` with "write requires --yes in JSON/nonterminal mode" means it stopped after the preview.
- **Registration is global.** `local.toml` is shared by every run that uses the same `XDG_CONFIG_HOME`. Isolate it — `drive.py env` does.
- **Attach does not repair.** A tree without a valid `dort.toml` is rejected; it is not scaffolded.
- **Unresolved transactions block later writes** until inspected in Recovery (`r`).
- **`--repo` picks a registered repository** for one command; it is not a path override for an unregistered tree.
