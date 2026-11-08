# Getting started

[User guide](README.md) · [Command reference](commands.md)

## Installation

Supported hosts are **macOS or Ubuntu**, on Intel/x86_64 or ARM64. Other Linux distributions are not currently supported. Local init/attach and Git inspection additionally need an installed trusted Git executable in an absolute PATH directory. Dort never installs Git.

### Prebuilt archives

Download an archive and `SHA256SUMS` from [GitHub Releases](https://github.com/katesongsy/dort/releases). Rust is not needed to run these binaries.

| Host | v0.1.0 archive |
| --- | --- |
| macOS 15+, Apple Silicon | `dort-v0.1.0-aarch64-apple-darwin.tar.gz` |
| macOS 15+, Intel | `dort-v0.1.0-x86_64-apple-darwin.tar.gz` |
| Ubuntu 24.04+, ARM64 | `dort-v0.1.0-aarch64-unknown-linux-gnu.tar.gz` |
| Ubuntu 24.04+, x86_64 | `dort-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` |

Linux binaries require glibc 2.39 or newer. Choose your archive, then verify its matching checksum before extracting. For example, on Apple Silicon macOS:

```sh
archive=dort-v0.1.0-aarch64-apple-darwin.tar.gz
curl -fLO "https://github.com/katesongsy/dort/releases/download/v0.1.0/$archive"
curl -fLO https://github.com/katesongsy/dort/releases/download/v0.1.0/SHA256SUMS
awk -v file="$archive" '$2 == file' SHA256SUMS > selected.sha256
test -s selected.sha256 && shasum -a256 -c selected.sha256
# On Ubuntu, use: sha256sum -c selected.sha256
# Continue only when the selected checksum reports OK.
mkdir dort-v0.1.0 && tar -xzf "$archive" -C dort-v0.1.0
./dort-v0.1.0/dort --version
```

Use an empty download directory, not an existing configuration repository. Archives contain `dort`, the MIT `LICENSE`, dependency notices (`THIRD-PARTY-NOTICES.txt`), Rust standard-library notices (`RUST-COPYRIGHT.html`) and `BUILD-INFO.json` (version, target, commit, Rust toolchain and minimum runtime). Checksums detect corruption; they are not an independent publisher signature. Use the binary's absolute path, or place it in a directory you already manage on PATH. Dort has no installer and does not bypass operating-system download/security prompts; do not disable platform protections to run it.

### Install with Cargo

The [`dort`](https://crates.io/crates/dort) executable and [`dort-core`](https://crates.io/crates/dort-core) library are published as source packages. Install the executable with Rust **1.99.1** active:

```sh
cargo install dort --version 0.1.0 --locked
dort --version
```

With rustup, use `cargo +1.99.1 install dort --version 0.1.0 --locked` to select the pinned toolchain explicitly. Cargo builds from source and installs into its configured binary directory, which must be on PATH. This does not initialize a Dort repository or manage any files.

### Build from source

You need pinned **Rust 1.99.1**; Cargo/rustup may download dependencies/toolchain components. From the Dort checkout:

```sh
cargo build --release --workspace --locked
./target/release/dort --version
```

Elsewhere, `dort` means your chosen executable; use its absolute path if necessary. Run `dort` or `dort tui` with terminal stdin/stdout/stderr for the interactive interface. Use at least 48×14; 80×24 or larger gives more room for reports.

## Start in a disposable HOME

Dort is experimental. Try this walkthrough before using your own configuration, and read the [safety guide](safety.md). Keep one shell session. The example below uses a source build from the Dort checkout; for a prebuilt archive, set `dort_bin` to the extracted binary's absolute path instead:

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

Welcome shows the next steps. `;` opens **Actions** with search focused; type a label, use Up/Down to choose and Enter to request it. Actions contains only this view’s item, marked-item and view operations. Use F10 or the top menu controls for navigation and session commands: Files → Managed / Discover / Ignored paths, Checks → Manifest checks / Diagnostics, and Git → Status / Unstaged / Staged. Use arrows and Enter to choose; Esc returns to the current view. Compact layouts show a Menu control, then a group list. The current subview stays visible; switch views for their workflows. Letter shortcuts below are optional accelerators. F1 opens contextual Help anywhere; `?` does so outside text. Esc returns to the retained draft/screen/focus with the draft intact. Discover shows **Ignore selected** (`I`) directly; Files → **Ignored paths** (`U` in Discover) is in the menu; both preference changes require review and literal `yes`. Tab/Shift-Tab moves among form controls, or switches Discover file-list/content focus outside text/dialogs. Highlighted borders/buttons show focus; Esc returns from content to the list. Left/Right changes destination outside text/dialogs. The bottom row collects Menu, Actions and Help shortcuts; the top row holds menu names and location.

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

- Open Files → **Managed** from the menu. Delete clears the display filter; select `notes`, then Enter for full metadata. Compact layouts use Esc to return to the list; wide layouts show list/details together.
- **Preview apply** (`a`) covers the entire active-profile scope, not the filtered list. After adoption the correct link is a successful no-op: approving it creates no new transaction. Esc can dismiss any preview without executing.
- **Check current repository manifest** (`c`) checks declarations, not live files or installed tools. **Profiles: temporary repository additions** (`t`) adds session-only profiles without removing base/default/platform/local contributors.
- Open Git → **Status** for metadata. Untracked files appear in Status, not tracked diffs. Unstaged/Staged summaries are metadata-only; Dort exposes no staging/commit/push. You do not need to view a patch for this walkthrough. **View patch content** is a separate secrets warning and literal `view patch` confirmation; patches may disclose secrets.
- Checks → **Diagnostics** (`j`) shows host, registry and global journal blockers. About/version is in the Session menu. These do not repair state or inspect live tools.

### 4. Keep the file and inspect history

1. In managed Files, select `notes`. Choose **Keep local file** (`u`), also available in Actions. Review the preview, Enter, then `yes` and Enter. Wait for observations to refresh.
2. Keep removes the mapping and materializes an ordinary `.notesrc`; `files/notesrc` remains in the repository. The alternative **Delete managed link** requires a separate exact-ID acknowledgement before `yes` and deletes only the correct link. Do not try that alternative after Keep: its mapping/link is already gone.
3. Open Recovery → **All repositories**. Search transaction metadata, select a row and Enter for full IDs, phase, integrity and action hints. The global blocker count is never filtered away. Completed history is not proof that rollback is still safe.
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
