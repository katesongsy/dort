# Tools

Declare a tool in the manifest, find out whether the package manager reports it installed, and install what is missing. Package-manager runs are the only part of Dort that reaches outside the repository.

## Sub-features

- **Declared tools** (`1`, `t`): status Installed / Missing / Version differs / Couldn't check / Needs attention.
- **Installed packages** (`2`, `v`): current inventory, no declaration or repository needed. `a` turns a package into a declaration with the fields filled in.
- **Installation history** (`3`, `h`): install records including failures. `x` prepares closure of an unresolved record; closure re-queries and only acknowledges the unchanged record.
- **Check installed** (`C`): reviews the detected package-manager queries, then runs them once.
- **Install missing** (`i`): review one eligible set, then install serially. First failure stops the run; earlier effects stay.
- **Declaration editing**: `a` add, `e` edit, `d` remove.
- **Tool settings** (`s`, only when detection needs paths): program path, extra search directories, npm CLI and global prefix.

## How to get to it (user POV)

TUI: `t`. Footer shows `a Add declaration`, `v Installed packages`, `h Installation history`. Numbers `1`/`2`/`3` switch tabs.

CLI:

```sh
dort tools inspect --dry-run
dort --yes --json tools inspect
dort tools check --dry-run
dort tools install-missing
dort tools declare --declaration '{"id":"rg","binary":"rg","install":{"provider":"brew","package":"ripgrep"}}'
dort tools attempts
dort tools close-attempt RECORD_ID --dry-run
```

## Driving it with harness

The run starts with zero declarations, so Tools opens on "No declared tools". Add one to the manifest first:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
cat >> "$DORT_REPO/dort.toml" <<'TOML'
[[tools]]
id = 'rg'
binary = 'rg'
[tools.install]
provider = 'brew'
package = 'ripgrep'
TOML
python3 .pi/skills/verify-dort/drive.py tui my-proof --label declared \
  "wait:loaded." "send:'t'" "wait:Declared tools" "snap:declared"
```

A declaration appears without a package manager. To check list navigation, seed more rows than fit on screen. On a wide terminal, select a row and confirm its details appear on the right. Press End and then Up. The highlight should move up while the last row stays visible. Use Ctrl-B and Ctrl-F to page in both directions.

## Observable end state

- **Declaration added:** `dort.toml` contains the `[[tools]]` block; the Declared tab lists the tool with a status, not a blank list.
- **Check run:** status becomes Installed / Missing / Couldn't check, and `dort tools attempts` gains a record.
- **Install:** a record under `$XDG_STATE_HOME/dort/tools-attempts/`, plus the package manager's own state. Installed only means the package manager reports the package; it does not prove the command runs.

## Gotchas

- **Checking and installing reach real package managers.** This is the one feature where a disposable HOME is not enough: `brew` and `npm` write shared state. The repo's own tests use fake package managers; a real install in a verification run is out of scope. Prove the read side, and say in the proof that the install leg was not exercised.
- **Zero declarations means nothing to check.** `tools check` on an empty manifest reports nothing rather than an error.
- **Approval is separate for inventory and for declarations.** Querying installed packages does not approve a declaration write, and the reverse.
- **Checks are point-in-time.** After an install, check again before another install.
- **No rollback.** Installs stop on first failure and leave earlier effects.
- **npm needs a runtime tool.** An npm declaration with `runtime_tool` needs that tool installed too; a missing runtime is a blocked plan, not a package-manager failure.
- **`close-attempt` only acknowledges.** It re-queries and refuses if the record changed. It never retries or repairs.
