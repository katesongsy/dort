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
- `Ctrl-P`: Choose a repository. `Ctrl-T`: Open its profiles. You can also click the scope in the header. Leave a review before switching.
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
- In **Managed**, use **Add mapping** for a file already in the repository. Select a mapping and use **Edit file mapping** to change its declaration. Review and save before applying. Adoption also lets you choose active profiles.
- Select a generated file in **Managed** and choose **Preview generated file** (`Enter` or `v`). Scroll with the arrows or page keys. `End` jumps to the end of the output; `Esc` returns to Managed.
- In either list, `Space` marks or unmarks, and `Home` / `End` select the first or last row. Hidden marks still count.
- **Delete link** and **Delete marked** are separate actions. Review the paths and confirm the exact mapping IDs before deleting links.
#### Generated files

In **Files**, open **Managed**. Press `s` or choose **New generated file**. Enter a file ID and a Home destination. Press `Tab` or `Shift+Tab` to move between fields. In **Template**, `Enter` adds a newline. Use the arrows, `Home`, and `End` to edit, or paste several lines at once. Use `{{ name }}` for an input. Keep secrets out of templates and inputs.

Choose **Open in editor** (`F5`) to edit the template in an external editor. Set `VISUAL` or `EDITOR` before starting Dort, for example `VISUAL='code --wait'`. Dort tries `VISUAL` first. Choose an editor command that waits until you close the file. The editor gets a temporary copy of the template. Save and exit the editor to return to the draft.

Choose **Continue** (`F4`). If the template uses inputs, enter the named values. Leave a value blank to render nothing. Choose **Review generated file** after entering inputs. Read the output. Press `y` when **Save generated file** is ready. `Esc` from the review or inputs returns to the draft. Saving changes only `dort.toml`.

To revisit a saved template, select it in Managed. Press `v` to preview it or `e` to choose **Edit generated file**. File-backed templates can be previewed; edit their source in the repository.

## Tools

Tools has three tabs: **1 Declared**, **2 Installed**, **3 History**.

### Declared tools

`Enter` opens details. Actions:

- `C` Check installed: review detected package-manager queries, then run once.
- `m` Check another manifest: enter its absolute path and review the check. The result does not change the selected repository or enable installs.
- `i` Install missing: review one eligible set, then install serially.
- `a` Add declaration, `e` Edit, `d` Remove.
- `s` Tool settings, only when detection needs paths.

Status is Installed, Missing, Version differs, Couldn't check or Needs attention. Installed means the package manager reports the package. Proving the command runs is a separate check.

Checks are point-in-time. After an install, check again before another install. Installs stop on the first failure. Earlier effects stay; there is no rollback.

### Installed packages

`2` opens the current package inventory. `R` prepares a fresh query. `/` filters the display. `Enter` opens package details. `a` starts a declaration with the package name as the Tool ID and Command name. Edit either field if needed. Saving returns to Installed.

The result needs no repository or tool declaration. Query approval is separate from declaration approval.

### Installation history

`3` lists install records. `Enter` opens details. `x` prepares closure of an unresolved record. Closure runs a fresh query and acknowledges the unchanged record. Retry and repair stay manual.

### Tool settings

Use settings only when detection cannot find the right program or folder. Enter exact paths for:

- The package-manager program.
- Extra program search directories.
- npm: Node's npm CLI and global prefix.

Saved settings stay on this host and apply to the selected repository and tool. npm settings apply to tools that share the selected Node runtime. Saving paths does not run a package manager.

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

### Preview a generated file

```sh
dort --repo PATH files preview-generated editor-settings
dort --repo PATH --json files preview-generated editor-settings
```

Renders the repository template with literal inputs from `dort.toml`. Shows if the destination is missing, exists and must be kept, or is unsafe. Dort checks the destination without reading its contents. No files are written. JSON includes the exact rendered text and input values.

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

Remove supports ordinary-file links only.

## Profiles

```sh
dort profiles list
dort profiles set work off
dort profiles set work on
dort profiles set work defaults
dort profiles list --profile work
```

`base` is always on. A temporary `--profile` wins over the saved choice, which wins over repository and platform defaults. Saved choices affect only the selected registered repository. Saving a choice leaves files and tools in place. An installed link can appear as **No longer selected** until you remove it separately.

From **Overview**, open **Profiles** with `2`, then choose **New profile**. Enter a name, then press **Space** on each default row to turn it on or off. **All hosts** turns the profile on everywhere; **macOS** and **Linux** turn it on for that platform. Leave all three off for an optional profile. **Edit profile defaults**, **Rename profile**, and **Delete profile** let you pick an existing profile first. Only unused profiles can be deleted. Press **F1 Help** for field details.

In **Overview → Profiles**, select a profile and choose **Local choice**. Select **Use defaults**, **On**, or **Off**, then review and save. Use the temporary profile picker in **Checks** to try additions without saving them.

## Clone or init

```sh
dort --dry-run clone https://example.org/dotfiles.git /path/to/new-repository
dort clone /absolute/path/to/local/source /path/to/new-repository
dort init /path/to/new-repository
dort init /path/to/existing-repository
```

`clone` accepts an HTTPS URL without credentials or an absolute local Git path. The destination must be new and its parent must exist. Dort keeps a clone with an invalid `dort.toml` without registering it. Fix the manifest, then attach the clone with `dort init PATH`. Clone leaves Home targets, profile choices, and packages unchanged.

`init` creates a local repository or attaches an existing Git tree. In the TUI, use **Clone repository** or **Create or attach** in **Overview**. If a clone needs repair, choose **Repair manifest** on the clone result.

In **Overview → F2 More**, set the startup repository, forget a registration, or reconnect a moved repository. Forgetting leaves the repository, Home files, and history in place. Reconnecting keeps saved choices and history. Check existing Home links after moving a repository.

## Environment

```sh
dort environment plan
dort environment check
dort --dry-run environment apply
dort environment apply
dort --dry-run environment resume
dort environment resume
dort --dry-run environment abandon
dort environment abandon
```

Use **Environment check** to update unknown tool results. It runs the reviewed package managers and saves results for ten minutes. **Plan** and `--dry-run` never run a manager. Pass the same `--profile NAME` additions to each command in the workflow.

Review tools and links together before applying. A required tool must be selected and available at the checked version before a dependent link changes. A blocked plan makes no changes. Package managers may install dependencies. Dort does not roll back packages.

If a run stops, review **Resume**. Dort rechecks confirmed work without reinstalling it. Run **Environment check** after an uncertain install. Repair the tool if the check cannot confirm its version and command. Changed declarations, choices, files, or approved effects can block the old resume.

An unfinished run cannot be replaced with a new plan. If Resume is blocked and you want to start over, review **Abandon environment run**. Dort keeps the old run record and leaves completed links and packages in place. Before retrying an uncertain install, close any unresolved install record in **Tools History** (or `dort tools close-attempt ID`), run **Environment check**, and review a new plan. Dort does not upgrade, downgrade, or uninstall a mismatched package. Use the package manager outside Dort to select the declared version, or correct the declaration, then check again.

In the TUI, open **Overview → Environment** with `3`, or choose **Review environment**. Press `C` or choose **Check environment** to review and run the checks, including package-manager checks for the selected tools. Dort returns to the updated environment review. Checking does not apply files or install packages. To continue after a stopped run, open **Environment run** in **Recovery** and choose **Review resume**. To keep completed changes and start over, choose **Abandon run** on the run record.

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
