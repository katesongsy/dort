# Getting started

[User guide](README.md) · [Command reference](commands.md)

## Requirements and build

Supported hosts are **macOS or Ubuntu**, on Intel/x86_64 or ARM64. Other Linux distributions are not currently supported. You need a Rust installation capable of using pinned **Rust 1.99.1**; Cargo/rustup may download dependencies/toolchain components. Local init/attach and Git inspection additionally need an installed trusted Git executable in an absolute PATH directory. Dort never installs Git.

From the Dort checkout:

```sh
cargo build --release --workspace --locked
./target/release/dort --help
./target/release/dort --version
```

There is no installer or package-manager execution yet. Elsewhere, `dort` means this executable; use its absolute path if necessary.

## First check: declarations only

```sh
./target/release/dort manifest check --manifest examples/dort.toml
./target/release/dort --json manifest check --manifest examples/dort.toml --profile dev
```

A successful check validates declarations, **not** live files or installed tools. The example includes future template/secret/tool capabilities, which are reported as unsupported.

## Try file previews without using your own configuration

The following walkthrough first previews, then explicitly executes **inside a disposable demo HOME**. It never uses your normal dotfiles. These are experimental write commands; review the [safety guide](safety.md) first. Keep one shell session, starting from the Dort checkout.

```sh
dort_bin="$(pwd -P)/target/release/dort"
demo="$(mktemp -d)"
# Resolve macOS /var aliases; no-follow access requires physical paths.
demo="$(cd "$demo" && pwd -P)"
mkdir -p "$demo/home"
printf 'notes-format plain\n' > "$demo/home/.notesrc"

demo_dort() {
    HOME="$demo/home" \
    XDG_CONFIG_HOME="$demo/home/.config" \
    XDG_STATE_HOME="$demo/home/.local/state" \
    "$dort_bin" "$@"
}

demo_dort doctor
# Parent exists; dotfiles destination must be absent.
demo_dort init "$demo/dotfiles" --dry-run
demo_dort init "$demo/dotfiles" --yes
demo_dort files discover
demo_dort files add '~/.notesrc' --id notes --source files/notesrc --dry-run
```

The init dry run creates nothing. Approved init publishes a minimal local Git repository and `dort.toml`, then registers/selects it. It does not clone, stage or commit. The add dry run previews source copy, manifest addition, backups and destination-link effects without performing them.

Now explicitly approve adoption and inspect the result:

```sh
demo_dort files add '~/.notesrc' --id notes --source files/notesrc --yes
demo_dort status
demo_dort plan
demo_dort --json files plan apply
demo_dort git status
demo_dort git diff
# Correct apply is a no-op, with no new transaction.
demo_dort files apply --yes
demo_dort history
```

`.notesrc` is now linked to its copied repository source. Git diff remains metadata-only; new untracked files appear in status, not diff. Dort exposes no Git staging/commit command. You can manage the disposable Git repository yourself, but never automatically merge into live-linked sources.

Unmanage while retaining an ordinary destination file:

```sh
demo_dort files remove notes --mode keep --dry-run
demo_dort files remove notes --mode keep --yes
demo_dort --json recovery list
printf 'Demo directory: %s\n' "$demo"
```

Keep removes the mapping and materializes the source bytes at `.notesrc`; the repository source remains. The alternative `--mode remove` deletes only the correct managed link and requires separate `--confirm-remove notes` acknowledgement. Do not run that alternative after keep: its mapping/link is already gone.

The demo remains for inspection; remove only that demo yourself when finished. On failure, stop and inspect `history`; never assume a failed write had no effects. See [explicit recovery](commands.md#explicit-recovery).

## Optional: try the interactive interface

In the same disposable demo shell, with terminal stdin/stdout/stderr:

```sh
demo_dort --dry-run tui
```

Overview, active Files and Recovery use the same services as the CLI. Press `?` for keys, `Tab` to switch views and `q` to exit. `a` previews apply; Enter cannot approve while `--dry-run` is set. After the keep step above, there are no active mappings; Recovery still shows retained history.

To try interactive mutations, omit `--dry-run` **only within the disposable demo**. Review the scrollable preview, Enter, then type `yes`; deleting a managed link additionally asks for its exact ID first. Esc cancels before execution. q/Ctrl-C after approval waits for the write to finish. Plain terminal `dort` also opens the interface; explicit CLI commands remain available for init/adoption/Git and scripting. See [interactive keys](commands.md#interactive-interface).

## Use an existing environment repository

```sh
dort --repo /absolute/path/to/dotfiles manifest check
dort --repo /absolute/path/to/dotfiles files list
dort --repo /absolute/path/to/dotfiles files plan apply
# Optional attachment/selection: valid non-bare Git root plus dort.toml required.
dort init /absolute/path/to/dotfiles --dry-run
```

Read-only manifest/file commands can inspect an explicit directory with `dort.toml` without registration. Mutation commands require prior approved init/attachment. Init accepts only valid existing repositories or an absent root under an existing safe parent—not arbitrary empty directories. Relative init paths resolve from the current directory; explicit physical absolute paths are recommended. There is no clone command.

Quote `'~/.notesrc'` so Dort expands it against the configured HOME. JSON/nonterminal writes require `--yes` and never prompt; human terminal mode can confirm interactively. `--dry-run` never prompts or acquires a lock. Do not use `sudo` or share reports blindly. Only explicit `git diff --patch` reveals bounded escaped file content, potentially secrets; read the [Git boundary](safety.md#git-inspection-boundary) before using it.
