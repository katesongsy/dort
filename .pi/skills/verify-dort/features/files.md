# Files

Adopt a real file from HOME into the repository, and manage the resulting link. This is Dort's core write path: a source copy, a manifest mapping and a link replacement as one recoverable transaction.

## Sub-features

- **Discover** (`2`): hidden `$HOME` entries and `$HOME/.config` entries, metadata only. Marks entries Eligible / Group / Ignored.
- **Managed** (`1`): active mappings with their link state.
- **Ignored paths** (`3`): exclusions.
- **Adopt**: copy one regular file into `files/`, add a base-profile mapping, replace the destination with a symlink.
- **Apply** (`a` from Managed): deploy all changed active links as one reviewed batch.
- **Unmanage** (`u`, or `U` for marked files): drop the mapping and keep the local file.
- **Remove**: delete the managed link, keep the repository source.
- **History**: per-operation records under `$XDG_STATE_HOME/dort/repositories/<id>/transactions/`.

## How to get to it (user POV)

Press `f` to open Managed. Press `2` for Discover or `3` for Ignored paths. In Discover, `Enter` opens details, `i` ignores the selected path, and Space marks or unmarks it. Press `a` to adopt the selected file or `A` to adopt marked files. Press `g` to open Git.

CLI:

```sh
dort --repo "$DORT_REPO" files discover
dort --repo "$DORT_REPO" files list
dort --repo "$DORT_REPO" files add --id demorc --source files/demorc "$HOME/.demorc"
dort --repo "$DORT_REPO" files remove demorc --mode keep
```

## Driving it with drive.py (PTY harness)

Seeding a real dotfile first is required — Discover lists hidden HOME entries, and a run with only Dort's own config has nothing to find. `discover --json` returns `{"candidates": [{"entry": {...}, "link_state", "supported_for_adoption"}]}`, so the path lives at `candidates[].entry.path`, not `candidates[].path`.

```sh
python3 .pi/skills/verify-dort/drive.py cli my-proof --label discover -- \
  --json --repo @REPO files discover
```

Proven end to end on this repo:

```sh
python3 .pi/skills/verify-dort/drive.py cli my-proof --label add -- \
  --yes --repo @REPO files add --id demorc --source files/demorc @HOME/.demorc
[ -L "$HOME/.demorc" ] && readlink "$HOME/.demorc"
cat "$DORT_REPO/files/demorc"
grep -A4 demorc "$DORT_REPO/dort.toml"
ls "$XDG_STATE_HOME/dort/repositories/"*/transactions/
```

TUI reach, same run:

```sh
python3 .pi/skills/verify-dort/drive.py tui my-proof --label discover \
  "wait:loaded." "send:'f'" "wait:Managed" "send:'2'" "wait:Discover" "snap:discover"
```

## Observable end state

An adoption worked when all four hold:

- `$HOME/<file>` is a symlink (`-L`) pointing at `$DORT_REPO/files/<source>`.
- The repository source holds the original bytes.
- `dort.toml` carries the mapping with `mode = "link"` and `profiles = ["base"]`.
- A new transaction directory exists under `$XDG_STATE_HOME/dort/repositories/<id>/transactions/`.

`files list` re-reads live state and should report `destination: Some(Symlink)`. That is the cheapest single check.

## Gotchas

- **Empty Discover.** Nothing seeded in `$HOME`, or the only entries are Dort's own config. Seed `.demorc` first.
- **`--yes` required.** Nonterminal and `--json` writes exit `2` without it, printing "write requires --yes in JSON/nonterminal mode". Pass `--dry-run` to preview without the flag.
- **`--dry-run` writes nothing.** It plans the source copy, manifest edit and link replacement, and creates no backup or journal. Confirm no destination file appeared and no transaction directory was added.
- **Adoption is destructive.** The destination is replaced by a link. Backups live under the repository, not in HOME.
- **`--confirm-remove ID`** is an extra guard beyond `--yes` for `files remove --mode remove`.
- **Groups cannot be adopted directly.** `.config` shows as Group; open it or mark its children.
- **Templates and secrets are blocked**, not silently allowed.
