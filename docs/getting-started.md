# Getting started

[User guide](README.md) · [Command reference](commands.md)

## Requirements and build

Supported hosts are **macOS or Ubuntu**, on Intel/x86_64 or ARM64. Other Linux distributions are not currently supported. You need Git and a Rust installation capable of using the pinned **Rust 1.99.1** toolchain; Cargo/rustup may download dependencies and toolchain components.

From a checkout of Dort:

```sh
cargo build --release --workspace --locked
./target/release/dort --help
./target/release/dort --version
```

There is no Dort installer or package-manager integration yet. In other guide pages, `dort` means this executable; replace it with its absolute path if it is not on your PATH.

## First check: declarations only

Run from the checkout:

```sh
./target/release/dort manifest check --manifest examples/dort.toml
./target/release/dort --json manifest check --manifest examples/dort.toml --profile dev
```

The example includes future template/secret/tool capabilities. A successful manifest check means the declarations are valid, **not** that files exist, tools are installed or deployment is supported. The CLI reports unsupported capabilities.

## Try file previews without using your own configuration

The commands below create only a new temporary demo directory. Dort is given that demo HOME and explicit XDG roots; it never inspects your normal HOME in this walkthrough. Keep the same shell session and start from the Dort checkout.

```sh
dort_bin="$(pwd -P)/target/release/dort"
demo="$(mktemp -d)"
# macOS temporary paths can contain /var symlink aliases; use the physical path.
demo="$(cd "$demo" && pwd -P)"
mkdir -p "$demo/home" "$demo/dotfiles/files"

cat > "$demo/dotfiles/dort.toml" <<'TOML'
schema = 1

[[files]]
id = "editor"
source = "files/editorrc"
target = "~/.editorrc"
mode = "link"
TOML

printf 'set editor-mode safe\n' > "$demo/dotfiles/files/editorrc"
printf 'notes-format plain\n' > "$demo/home/.notesrc"

demo_dort() {
    HOME="$demo/home" \
    XDG_CONFIG_HOME="$demo/home/.config" \
    XDG_STATE_HOME="$demo/home/.local/state" \
    "$dort_bin" "$@"
}

demo_dort doctor
demo_dort --repo "$demo/dotfiles" manifest check
demo_dort files discover
demo_dort --repo "$demo/dotfiles" files list
demo_dort --repo "$demo/dotfiles" files plan apply
demo_dort --repo "$demo/dotfiles" --json files plan apply
```

The apply preview proposes a link for `.editorrc`. It does not create the link or any state directory.

Now preview adopting the existing ordinary `.notesrc` into a **new** mapping:

```sh
demo_dort --repo "$demo/dotfiles" files plan adopt '~/.notesrc' \
    --id notes --source files/notesrc
demo_dort --json recovery list
printf 'Demo directory: %s\n' "$demo"
```

Adoption previews source copy, manifest addition, backups and destination-link effects together. Nothing is copied, edited, backed up or linked. The demo stays on disk for your inspection; remove that demo directory yourself when finished.

## Use an existing environment repository

Specify it explicitly:

```sh
dort --repo /absolute/path/to/dotfiles manifest check
dort --repo /absolute/path/to/dotfiles files list
dort --repo /absolute/path/to/dotfiles files plan apply
```

There is no CLI repository registration, `init` or `clone` command yet. Without `--repo`, repository commands require an already registered active repository. Manifest/file explicit-repository commands need a directory containing `dort.toml`; they do not initialize Git or register it. The separate Git inspection commands need a real non-bare Git working-tree root but no manifest:

```sh
dort --repo /absolute/path/to/dotfiles git status
dort --repo /absolute/path/to/dotfiles git diff
dort --repo /absolute/path/to/dotfiles git diff --staged
```

These reports are metadata-only. Content requires the explicit `git diff --patch` option and can expose secrets; read the [Git safety boundary](safety.md#git-inspection-boundary) first. A trusted installed Git must be available through an absolute PATH entry; other Dort commands do not require Git at runtime.

Quote `'~/.editorrc'` when passing a destination that Dort should expand against its configured HOME. Do not use `sudo`. Read the [safety guide](safety.md) before inspecting personal configuration or sharing reports.
