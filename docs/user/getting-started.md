# Getting started

[User guide](README.md) · [Command reference](commands.md)

## Requirements and build

Supported hosts are **macOS or Ubuntu**, on Intel/x86_64 or ARM64. Other Linux distributions are not currently supported. You need a Rust installation capable of using pinned **Rust 1.99.1**; Cargo/rustup may download dependencies/toolchain components. Local init/attach and Git inspection additionally need an installed trusted Git executable in an absolute PATH directory. Dort never installs Git.

From the Dort checkout:

```sh
cargo build --release --workspace --locked
./target/release/dort --version
```

There is no installer or package-manager execution yet. Elsewhere, `dort` means this executable; use its absolute path if necessary. Run `dort` or `dort tui` with terminal stdin/stdout/stderr for the interactive interface. Use at least 48×14; 80×24 or larger gives more room for reports.

## Start in a disposable HOME

Dort is experimental. Try this walkthrough before using your own configuration, and read the [safety guide](safety.md). Keep one shell session, starting from the Dort checkout:

```sh
dort_bin="$(pwd -P)/target/release/dort"
demo="$(mktemp -d)"
# Resolve macOS /var aliases: no-follow access requires physical paths.
demo="$(cd "$demo" && pwd -P)"
mkdir -p "$demo/home"
printf 'notes-format plain\n' > "$demo/home/.notesrc"

demo_dort() {
    HOME="$demo/home" \
    XDG_CONFIG_HOME="$demo/home/.config" \
    XDG_STATE_HOME="$demo/home/.local/state" \
    "$dort_bin" "$@"
}

# Keep this path for the Create/attach form below.
printf 'Demo repository path: %s/dotfiles\n' "$demo"
demo_dort
```

Only this child command uses the demo HOME/XDG paths; your shell's HOME is unchanged. The demo repository destination must be **absent**, with its parent already present. Keep the demo on one filesystem: repository, HOME and state must support the same-filesystem atomic-rename operations. Do not use `sudo`.

### Find your way around

Welcome shows the next steps. `;` opens **Actions** with search focused; type a label, use Up/Down to choose and Enter to request it. Context sections contain this view’s item, marked-item and view commands; Global contains navigation/session controls. Switch views for their workflows. Letter shortcuts below are optional accelerators. F1 opens contextual Help anywhere; `?` does so outside text. Esc/F1 returns to the retained draft/screen/focus without submitting it. Discover shows **Ignore selected** (`I`) and **Ignored paths** (`U`) directly; both preference changes require review and literal `yes`. Tab/Shift-Tab moves among form controls, or switches Discover file-list/content focus outside text/dialogs. Highlighted borders/buttons show focus; Esc returns from content to the list. Left/Right changes destination outside text/dialogs, as do `1` Overview, `2` Files, `3` Recovery, `4` Checks and `5` Git.

PgUp/PgDn scroll reports; Home returns to the start. File reviews use Up/Down to select a mapping and `i` to toggle the complete report. In that full report, Left/Right scrolls long lines; Home resets both axes. Esc returns without writing: confirmation → preview → retained form → destination. In text fields/search, printable `q`, `?` and `/` are text, Backspace edits and Ctrl-U clears the current field. Outside text entry, `q` exits; Ctrl-C exits anywhere, waiting for an active worker.

### 1. Create a local repository

1. Open Actions → **Create/attach local repository** (`n`). Enter the physical demo repository path printed above, for example `/private/tmp/your-demo/dotfiles`. Do not enter the literal `$demo`, `~`, or an existing empty directory: the form does not expand shell variables/tilde.
2. Enter advances from the path field to **Review read-only preview**. Enter on that button requests the preview. Inspect its resolved paths and effects, scrolling as needed. Esc returns to the retained path without creating anything; Enter requests a fresh preview again.
3. When the preview is intended and unblocked, Enter opens confirmation. Type literal `yes`, then Enter. Wait for observations to refresh before choosing another action.

Approved creation publishes a minimal local Git repository and `dort.toml`, registers/selects it and shows the result in Overview. It does not clone, stage or commit. Missing Git is a prerequisite error: install trusted Git separately and restart Dort; other read-only workflows remain available. A preview alone never authorizes execution.

For inspection-only sessions, use `demo_dort --dry-run tui` instead. This disables all write approvals, including creation/adoption; read-only discovery/checks remain available, but automatic Discover file-content previews are withheld. The remaining write steps require a normal session **only inside this disposable demo**.

### 2. Discover and adopt the demo file

1. Open Actions → **Discover files** (`f`). This scans bounded metadata and automatically previews only the selected eligible regular file, with sensitive/binary/oversized content withheld. Directories and other ineligible rows are labeled and cannot open a candidate adoption draft. Find the `.notesrc` regular-file row with **Eligible hint** in its ADOPTION column with Up/Down or `/notesrc`; Esc leaves search while retaining the display filter.
2. Enter inspects the candidate without starting adoption (optional). Press `e` or click **Adopt selected** to open the editable draft as a separate action. Set TARGET to `~/.notesrc`, ID to `notes`, and SOURCE to `files/notesrc`. Enter advances fields; Tab/Shift-Tab moves between fields and buttons. The suggested ID/source may differ, so edit them explicitly for this walkthrough.
3. On **Review read-only preview**, Enter requests the actual preview. Review source-copy, manifest, destination-link and backup effects. PgDn scrolls selected effects; `i` opens the full report, including permissions and warnings. Esc retains the draft without writing. To execute adoption, activate **Adopt all** with Enter or a click from the unblocked preview; no typed yes is needed. Dort returns to refreshed Discover, retaining receipt details.

The demo `.notesrc` becomes a link to its copied repository source, with a new base-profile mapping. Discovery eligibility is not approval or proof that content is safe: privately review any real file before adoption. Only Discover's ephemeral selected-file preview displays ordinary content; reports, receipts and history stay content-free. Unsupported kinds, sensitive paths/content and unsafe parents are refused by core checks.

### 3. Inspect files, checks and Git

- Open **Files** (`2`). If a discovery list is still shown, use Actions → **Files: managed mappings**. Delete clears the display filter; select `notes`, then `i`/Enter for full metadata. Compact layouts use Esc to return to the list; wide layouts show list/details together.
- **Preview apply** (`a`) covers the entire active-profile scope, not the filtered list. After adoption the correct link is a successful no-op: approving it creates no new transaction. Esc can dismiss any preview without executing.
- **Check current repository manifest** (`c`) checks declarations, not live files or installed tools. **Profiles: temporary repository additions** (`t`) adds session-only profiles without removing base/default/platform/local contributors.
- Open **Git** (`5`) for Status metadata. Untracked files appear in Status, not tracked diffs. Unstaged/Staged summaries are metadata-only; Dort exposes no staging/commit/push. You do not need to view a patch for this walkthrough. **View patch content** is a separate secrets warning and literal `view patch` confirmation; patches may disclose secrets.
- **Checks** (`4`) → **Diagnostics** (`j`) shows host, registry and global journal blockers. About/version is in Actions. These do not repair state or inspect live tools.

### 4. Keep the file and inspect history

1. In managed Files, select `notes`. Choose Actions → **Keep local file** (`k`). Review the preview, Enter, then `yes` and Enter. Wait for observations to refresh.
2. Keep removes the mapping and materializes an ordinary `.notesrc`; `files/notesrc` remains in the repository. The alternative **Delete managed link** requires a separate exact-ID acknowledgement before `yes` and deletes only the correct link. Do not try that alternative after Keep: its mapping/link is already gone.
3. Open **Recovery** (`3`). Search transaction metadata, select a row and Enter/`i` for full IDs, phase, integrity and action hints. The global blocker count is never filtered away. Completed history is not proof that rollback is still safe.
4. For the latest schema-2 Keep transaction, **Preview rollback** (`b`) requests a fresh guarded preview. Inspect its inverse effects; Esc returns without executing. Only if you intend to restore the managed link/mapping, obtain the preview again, Enter, then `yes` and Enter. Later edits/transactions can invalidate guards; a refusal must not be bypassed. Schema-1 adoption rollback instead withdraws its newly published source into retained private transaction storage.

On any failed write, stop and inspect Recovery: partial effects may remain. Error `h` opens independent all-repository Recovery. Error `r` retries a read/fresh preview, or only refreshes after a failed write; it never replays approval. There is no automatic undo or repair. See [explicit recovery](commands.md#explicit-recovery).

Exit with `q` outside text entry or Ctrl-C. The demo remains for inspection; remove **only that demo directory** yourself when finished. Backups/private transaction artifacts are retained, not garbage-collected.

## Use an existing environment repository

Launch `dort`, then Actions → **Choose repository (session only)** (`p`). Select a known root or enter an exact physical directory containing `dort.toml`; selection does not register it or save configuration. To enable writes, use **Create/attach** with that exact existing non-bare Git working-tree root and approve its attachment preview. Arbitrary empty directories are refused. Relative form paths resolve from Dort's launch directory; no upward repository search or clone operation occurs.

**Check standalone manifest** (`v`) works without a selected repository, registration or Git. Enter an exact manifest path, then activate Check. A successful check validates declarations, not live files or installed tools. Future templates/secrets/tools are reported as unsupported.

## Optional CLI and automation

All supported local workflows also have explicit CLI commands. See the [command reference](commands.md) for flags, JSON, profile additions and recovery:

```sh
demo_dort doctor
demo_dort files list
demo_dort --json recovery list
# Preview only; no writes, locks or prompts.
demo_dort files apply --dry-run
```

Quote `'~/.notesrc'` in CLI arguments so Dort expands it against the configured HOME. JSON/nonterminal writes require `--yes` and never prompt; `--yes` does not bypass safety validation or separate deletion acknowledgement. Do not share reports blindly. Only explicit patch disclosure reveals bounded escaped file content, potentially secrets; read the [Git boundary](safety.md#git-inspection-boundary) first.
