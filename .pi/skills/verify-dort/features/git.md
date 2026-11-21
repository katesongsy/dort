# Git

Read-only view of the repository's Git state. Fixed arguments, no network, no hooks, no writes.

## Sub-features

- **Status** (`1`): changed paths with staged/unstaged kind.
- **Unstaged** (`2`) and **Staged** (`3`): per-side change lists.
- **Choose Git scope** (`P` / `g`): which repository the view covers.
- **Diff**: paths by default; `--patch` prints escaped content after an explicit disclosure.
- **Filter** (`/`): narrow the list. Numbers are text inside a filter.

## How to get to it (user POV)

TUI: `g`. Footer shows `Enter Details`, `F3 Scope details`, `F2 More`.

CLI:

```sh
dort --repo "$DORT_REPO" git status
dort --repo "$DORT_REPO" git diff
dort --repo "$DORT_REPO" git diff --staged
dort --repo "$DORT_REPO" git diff --patch
```

## Driving it with harness

The run's repository is a real Git tree with `demo.txt` and `dort.toml` untracked, so `g` shows two entries immediately:

```sh
python3 .pi/skills/verify-dort/drive.py tui my-proof --label git \
  "wait:loaded." "send:'g'" "wait:Git / Status" "snap:status"
python3 .pi/skills/verify-dort/drive.py cli my-proof --label git-status -- \
  --repo @REPO git status
```

To prove the change-tracking leg, dirty the tree first:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
printf 'more\n' >> "$DORT_REPO/demo.txt"
git -C "$DORT_REPO" add dort.toml
dort --repo "$DORT_REPO" git status
```

## Observable end state

- Status lists each changed path with a kind (`untracked`, `modified`, `added`, …).
- `git diff --staged` shows only what is staged; `git diff` shows only what is not.
- The TUI header reads `Git / Status` with a `Root:` line and an entry count.
- Nothing in the working tree changes. `git -C "$DORT_REPO" status --porcelain` before and after must match — that is the proof the view is read-only.

## Gotchas

- **`--patch` prints file content.** A diff of a dotfile can expose secrets. Review privately and do not paste it into evidence.
- **Untracked means unchanged in Git's eyes.** A freshly init'd repository shows everything as `untracked` (`-` in the XY column), which is correct, not a failure.
- **The header root is truncated** to the terminal width. Use `dort git status --json` or the repository path from `run.json` when you need the full path.
- **Scoped by repository, not by directory.** `P` chooses among registered repositories; there is no path filter.
- **The Git executable is resolved once at startup.** A run with `git` off `PATH` fails repository workflows at startup rather than at the first Git call.
