# Getting started

[User guide](README.md) · [Command reference](commands.md)

Dort runs on macOS, Linux and WSL. It needs a trusted Git executable on PATH for repository and Git workflows.

## Installation

### Script

Install the latest release with one command:

```sh
curl -fsSL https://raw.githubusercontent.com/katesongsy/dort/main/install.sh | bash
```

The script detects your host, offers to install missing prerequisites, verifies the release SHA-256 and installs `dort` into `/usr/local/bin` when that is writable, otherwise `~/.local/bin`. It prints the export line when the install directory is missing from PATH. Answers come from the terminal, so piping the script into `bash` works.

| Option | Effect |
| --- | --- |
| `--tag TAG` | Install a specific release, for example `v0.2.0`. Default: the latest release. |
| `--install-dir DIR` | Install `dort` into `DIR`. |
| `--yes` | Accept every prerequisite install. |
| `--skip-deps` | Leave Homebrew, Git and Node.js alone. |
| `--version` | Print the installer version. |
| `--help` | Print usage. |

Pass options through the pipe with `bash -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/katesongsy/dort/main/install.sh | bash -s -- --tag v0.2.0
```

The install needs `curl` or `wget`, and `sha256sum` or `shasum`. The checksum matches the archive against the release `SHA256SUMS` file, which detects a corrupt download.

### Homebrew

The [Dort tap](https://github.com/katesongsy/homebrew-tap) installs a verified release:

```sh
brew install katesongsy/tap/dort
```

This supports macOS 15+ and Ubuntu 24.04+ (glibc 2.39+).

### Prebuilt archives

Download `SHA256SUMS` and the archive for your host from the same [GitHub release](https://github.com/katesongsy/dort/releases):

| Host | Archive ends with |
| --- | --- |
| macOS, Apple Silicon | `aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `x86_64-apple-darwin.tar.gz` |
| Linux, ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |
| Linux, x86_64 | `x86_64-unknown-linux-gnu.tar.gz` |

Verify the checksum before extracting. Set `archive` to the filename you downloaded:

```sh
archive=dort-v0.2.0-aarch64-apple-darwin.tar.gz
# macOS:
grep -F "  $archive" SHA256SUMS | shasum -a 256 -c -
# Linux:
grep -F "  $archive" SHA256SUMS | sha256sum -c -
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
tmp=$(mktemp -d)
tmp=$(cd "$tmp" && pwd -P)
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

Use `o`/`f`/`t`/`r`/`c`/`g` to move between destinations. `F1` opens Help. `q` quits.

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
