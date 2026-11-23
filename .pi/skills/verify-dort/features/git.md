# Git

Read-only view of the repository's Git state. Fixed arguments, no network, no hooks, no writes.

## Sub-features

- **Status** (`1`): changed paths with staged/unstaged kind.
- **Unstaged** (`2`) and **Staged** (`3`): per-side change lists.
- **Choose Git scope** (`P` / `g`): which repository the view covers.
- **Patch preview**: highlight a tracked change to see its patch. CLI `git diff` lists paths unless you pass `--patch`.
- **Filter** (`/`): narrow the list. Numbers are text inside a filter.

## How to get to it (user POV)

TUI: `g`. Highlight a change with ↑/↓ or click it. In a narrow terminal, press Enter to open the patch. Use `F3` for scope details.

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

To prove the patch preview, commit two files in the disposable repository, then change both:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
printf 'old A\n' > "$DORT_REPO/a.txt"
printf 'old B\n' > "$DORT_REPO/b.txt"
git -C "$DORT_REPO" add a.txt b.txt
git -C "$DORT_REPO" -c user.name=Smoke -c user.email=smoke@example.invalid commit -m 'Seed patches'
printf 'new A\n' > "$DORT_REPO/a.txt"
printf 'new B\n' > "$DORT_REPO/b.txt"
cd /abs/path/to/repo
python3 .pi/skills/verify-dort/drive.py tui my-proof --label git-patch --size 30 140 \
  "wait:loaded." "send:'g'" "wait:+new A" "snap:first" \
  "send:'\x1b[B'" "wait:+new B" "snap:second"
```

## Observable end state

- Status lists each changed path with a kind (`untracked`, `modified`, `added`, …).
- `git diff --staged` shows only what is staged; `git diff` shows only what is not.
- The right pane shows the highlighted file’s changes. After moving down, it shows `+new B`, not `+new A`.
- Nothing in the working tree changes. `git -C "$DORT_REPO" status --porcelain` before and after must match — that is the proof the view is read-only.

## Gotchas

- **Patches show file content.** Use synthetic files in evidence. Review real patches privately.
- **Untracked files have no patch.** Add a tracked change to the fixture to test patch previews.
- **The header root is truncated** to the terminal width. Use `dort git status --json` or the repository path from `run.json` when you need the full path.
- **Scope and filter are separate.** `P` picks the repository. `/` filters the displayed paths.
- **The Git executable is resolved once at startup.** A run with `git` off `PATH` fails repository workflows at startup rather than at the first Git call.
