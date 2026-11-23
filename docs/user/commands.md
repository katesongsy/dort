# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

Read-only commands never install packages. Writes and package-manager queries need a preview and confirmation.

## Global options

- `--json`: structured output on stdout; errors on stderr.
- `--repo PATH`: use a registered repository for this command.
- `--dry-run`: preview only. No writes, locks, prompts or package-manager queries.
- `--yes`: approve noninteractive writes and package-manager queries.
- `--confirm-remove ID`: extra guard for link deletion.

JSON and nonterminal writes still need `--yes`.

## Interactive interface

Run `dort` or `dort tui`.

- `F1`: Help.
- `F2`: More actions, including preview-only mode and About.
- Sections: Overview `o`, Files `f`, Tools `t`, Recovery `r`, Checks `c`, Git `g`.
- `Enter`: open the selected item or invoke the primary action.
- `Esc`: back.
- `q` / `Ctrl-C`: quit. Active work finishes first.

Wide review lists show details for the selected row on the right. Use `Enter` for a full view, or `F3` in Ignored paths.
In Files, Recovery, Git and Tools lists, `Ctrl-F` and `Ctrl-B` work like `PgDn` and `PgUp`. They move selection. Use `/` where a filter is offered.
Named actions start checks, installs or reviews. Numbers switch collection tabs; in fields and filters they are text.

Reviews show exact effects. Confirm once. Help keeps drafts; choosing another destination drops approval. A window below 48×14 clears approval.

After a failure, `R` retries a read or asks for a fresh preview. It never replays an approval. After a failed write, `R` only refreshes; use `h` for Recovery. A blocked earlier operation names its repository and action; open Recovery.

### Files
Use `1` for Managed, `2` for Discover and `3` for Ignored paths. `g` opens Git.
- **Managed:** `u` Unmanage keeps the local file; `U` Unmanage marked does the same for marked files. `m` Manual adopt opens an adoption draft. `a` Apply reviews all active files; `A` Apply marked reviews marked files only.
- **Discover:** `a` Adopt opens a draft for the selected file; `A` Adopt marked opens a batch draft. `i` Ignore reviews excluding the selected path from discovery.
- In either list, `Space` marks or unmarks, and `Home` / `End` select the first or last row. Hidden marks still count.
- **Delete link** and **Delete marked** are separate actions. Review the paths and confirm the exact mapping IDs before deleting links.
## Tools

Tools has three tabs: **1 Declared**, **2 Installed**, **3 History**.

### Declared tools

`Enter` opens details. Actions:

- `C` Check installed: review detected package-manager queries, then run once.
- `i` Install missing: review one eligible set, then install serially.
- `a` Add declaration, `e` Edit, `d` Remove.
- `s` Tool settings, only when detection needs paths.

Status is Installed, Missing, Version differs, Couldn't check or Needs attention. Installed means the package manager reports the package. Proving the command runs is a separate check.

Checks are point-in-time. After an install, check again before another install. Installs stop on the first failure. Earlier effects stay; there is no rollback.

### Installed packages

`2` opens the current package inventory. `R` prepares a fresh query. `/` filters the display. `Enter` opens package details. `a` starts a declaration with known fields filled in.

The result needs no repository or tool declaration. Query approval is separate from declaration approval.

### Installation history

`3` lists install records. `Enter` opens details. `x` prepares closure of an unresolved record. Closure runs a fresh query and acknowledges the unchanged record. Retry and repair stay manual.

### Tool settings

Use settings only when detection cannot find the right program or folder. Enter exact paths for:

- The package-manager program.
- Extra program search directories.
- npm: Node's npm CLI and global prefix.

Saved settings affect later checks. They do not run anything.

### CLI

```sh
dort tools inspect --dry-run
dort tools inspect
dort --yes --json tools inspect

dort tools check --dry-run
dort tools check
dort tools install-missing

dort tools declare --declaration '{"id":"rg","binary":"rg","install":{"provider":"brew","package":"ripgrep"}}'
dort tools edit --declaration '{"id":"rg","binary":"rg","install":{"provider":"brew","package":"ripgrep"}}'
dort tools remove rg --dry-run
dort tools remove rg

`provider` names the package manager: `brew` or `npm`.

dort tools attempts
dort tools attempts --id RECORD_ID
dort tools close-attempt RECORD_ID --dry-run
dort tools close-attempt RECORD_ID --provider brew \
  --executable /opt/homebrew/bin/brew \
  --search-dir /opt/homebrew/bin \
  --prefix /opt/homebrew
```

Check and install accept `--manifest PATH`, `--repo PATH`, repeated `--profile NAME`, and optional per-tool paths:

```sh
dort tools check --program node=/path/to/node
dort tools check --npm-cli node=/path/to/npm-cli.js --npm-prefix node=/path/to/global
```

Declaration JSON is one complete Tool. It is not approval.

## Files

### Discover

```sh
dort files discover
```

Shows hidden HOME entries and `.config` entries. Contents stay unread.

### List

```sh
dort --repo PATH files list --profile dev
```

Shows active mappings and blockers without touching files or tools.

### Plan

```sh
dort --repo PATH files plan apply --profile dev
dort --repo PATH files plan adopt --id editor --source files/editorrc ~/.editorrc
dort --repo PATH files plan unmanage editor
```

Previews only. No writes, backups or history records.

`adopt` copies an owned regular file into a new repository source and creates one mapping. `unmanage` previews removal of a mapping.

### Apply

```sh
dort --repo PATH files apply --profile dev
```

Deploys all changed active links as one reviewed operation. Correct links are untouched.

### Add

```sh
dort --repo PATH files add --id editor --source files/editorrc ~/.editorrc
```

Adopts one existing regular file, adds a base-profile mapping and creates the link.

### Remove

```sh
dort --repo PATH files remove editor --mode keep
dort --repo PATH files remove editor --mode remove
```

- `keep`: keep the current file, remove the mapping.
- `remove`: delete the managed link, keep the repository source.

Add `--confirm-remove editor` for an extra guard.

Only ordinary-file links are supported. Templates and secrets are declared but blocked.

## Init

```sh
dort init /path/to/new-repository
dort init /path/to/existing-repository
```

Creates a local repository at an absent path, or attaches an existing Git tree with a valid `dort.toml`. Clone and sync stay manual.

## Checks

```sh
dort doctor
dort --json doctor
dort manifest check --manifest /path/to/dort.toml
dort --repo /path/to/dotfiles manifest check --profile dev
```

`doctor` shows host and local state paths. Package checks and configuration repair stay manual.

`manifest check` validates declarations and profiles. File sources and installed tools belong to the other checks. `--manifest` takes no `--repo`.

## Git

```sh
dort --repo PATH git status
dort --repo PATH git diff
dort --repo PATH git diff --staged
dort --repo PATH git diff --patch
```

Status and paths by default. `--patch` prints escaped content and may expose secrets. Review privately. No network, hooks or writes.

## History and recovery

```sh
dort history
dort --json recovery list
```

`history` and `recovery list` inspect all local operation records. They do not change files or replay work.

```sh
dort rollback REPOSITORY_ID OPERATION_ID --dry-run
dort rollback REPOSITORY_ID OPERATION_ID

dort recovery resume REPOSITORY_ID OPERATION_ID --dry-run
dort recovery abandon REPOSITORY_ID OPERATION_ID --dry-run
```

- `rollback`: reverse an unchanged file or declaration operation.
- `resume`: continue an intact repository setup.
- `abandon`: mark an incomplete setup abandoned. It keeps any published repository and registration.

Recovery checks current files and refuses changed state. It never runs automatically. An unresolved or damaged record can block new writes until you inspect it.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | I/O failure or unavailable data |
| 2 | Invalid input, configuration or unsupported host |
| 3 | Another Dort process holds the write lock |
| 4 | Blocked plan or recovery diagnosis reports write blockers |

Exit 4 still prints the full report on stdout. JSON errors go to stderr.
