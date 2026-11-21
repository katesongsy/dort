# Getting started

[User guide](README.md) · [Command reference](commands.md)

Dort runs on macOS, Linux and WSL. It needs a trusted Git executable on PATH for repository and Git workflows.

## Installation

### Homebrew

The [Dort tap](https://github.com/katesongsy/homebrew-tap) installs a verified release:

```sh
brew install katesongsy/tap/dort
```

This supports macOS 15+ and Ubuntu 24.04+ (glibc 2.39+).

### Mise

Mise can install the pinned GitHub release:

```sh
mise use --global github:katesongsy/dort@0.1.0
mise exec github:katesongsy/dort@0.1.0 -- dort --version
```

Configure mise's PATH/shims, or use `mise exec` each time.

### Prebuilt archives

Download the archive for your host from [GitHub Releases](https://github.com/katesongsy/dort/releases):

| Host | Archive |
| --- | --- |
| macOS, Apple Silicon | `dort-v0.1.0-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `dort-v0.1.0-x86_64-apple-darwin.tar.gz` |
| Linux, ARM64 | `dort-v0.1.0-aarch64-unknown-linux-gnu.tar.gz` |
| Linux, x86_64 | `dort-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` |

Verify the checksum before extracting:

```sh
shasum -a 256 -c selected.sha256
# Linux:
sha256sum -c selected.sha256
```

Use an empty download directory, not an environment repository. Archives contain `dort`, the license, notices and build info.

### Cargo or source

Cargo builds need Rust 1.99.1:

```sh
cargo install dort --locked
```

From a checkout:

```sh
cargo build --workspace --locked
target/debug/dort --version
```

## Try it in a disposable HOME

Use a temporary HOME. Do not point Dort at real dotfiles yet.

```sh
tmp=$(mktemp -d /tmp/dort-demo.XXXXXX)
mkdir -p "$tmp/home"
export HOME="$tmp/home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_STATE_HOME="$HOME/.local/state"
cd "$tmp"
dort init repo
```

Add a demo file to the repository:

```sh
printf 'hello\n' > repo/demo
```

Open the workbench:

```sh
dort
```

Use Menu (`Ctrl-G`) to move between destinations. `F1` opens Help. `q` quits.

1. Open **Files**. Choose **Discover**, select the repository file and adopt it with a preview.
2. Open the file's details and review **Apply**. Confirm once.
3. Inspect **Status** and **History**. Keep or delete the link when finished.
4. Open **Checks** to validate the manifest.

The same steps work from the CLI:

```sh
dort --repo "$tmp/repo" files discover
dort --repo "$tmp/repo" files plan apply
dort --repo "$tmp/repo" files apply
dort --repo "$tmp/repo" manifest check
```

`--dry-run` previews without writing. `--yes` approves noninteractive writes.

Remove only the demo directory when finished:

```sh
rm -rf "$tmp"
```

## Use an existing repository

`dort init` accepts an absent path, or an existing Git working tree with a valid `dort.toml`. It registers the repository; it does not clone or sync.

To select another registered repository for one command, use `--repo`.

## Tools

Tools → **1 Declared** lists tools from the manifest. **Check installed** asks before running package managers. **Install missing** reviews one exact set and runs it serially.

Tools → **2 Installed** shows the current package inventory. You can turn a package into a declaration.

Tools → **3 History** shows install records, including failures. Open a record to inspect it or close an unresolved one.

The CLI equivalents are `tools check`, `tools install-missing`, `tools inspect` and `tools attempts`. See the [command reference](commands.md#tools).

## Next steps

- Read the [manifest guide](manifest.md).
- Read [safety](safety.md) before using real files or package managers.
- Use `--dry-run` and review every preview.
