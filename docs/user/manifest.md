# Manifest guide

An environment repository contains `dort.toml`. It describes files, tools and profiles.

`schema = 1` is the manifest format version. Unknown fields, duplicate IDs, invalid paths and broken references are errors.

## Upgrade from v0.1.0

Back up `dort.toml` before upgrading. Keep `schema = 1` and make these changes:

- Rename the `ubuntu` key under `[profiles.platform]` and `[files.overrides]` to `linux`. Profile names and file paths can stay the same.
- Replace the `macos` and `ubuntu` entries under each `[tools.install]` with one rule for all hosts. For example, replace `macos = { provider = "brew", package = "git" }` with `provider = "brew"` and `package = "git"` on separate lines.
- Replace APT and mise rules with a Homebrew or npm rule where one fits. Otherwise, manage the tool separately, remove its declaration and update references to its ID in `requires` and `runtime_tool`.

Run `dort manifest check --manifest /absolute/path/to/dort.toml` before applying changes.

### Repair an unregistered clone

1. Choose **Repair manifest** on the clone result. For another unregistered Git working tree, use **Repair unregistered manifest** in **Checks** and enter its absolute path. Opening the draft displays the manifest text.
2. Review the draft suggestions for legacy `ubuntu` keys and single Homebrew or npm rules. Edit the remaining lines. Use **Up/Down** to select a line, **Ctrl-U** to clear it, **Del** to remove it, and **F4** to insert a line.
3. Press **Enter** to validate the draft and open the diff. Read it and confirm **Save repaired manifest**. Dort writes only `dort.toml` and keeps its original contents in a private backup. The Git index, registration, Home files and packages stay untouched.
4. Choose **Review attach** on the repair result to review registration for that folder.

Dort does not choose between competing platform install rules, substitute a manager for APT or mise, or change an unsupported `schema` value. Use a compatible Dort version or migrate the manifest externally if the schema differs. For unsupported managers, choose a supported rule or manage the package separately and update the declaration and its references. Use an external editor for larger files, registered repositories, or files the draft refuses. The guarded TUI write needs the repository and Dort state on the same filesystem.

## Files

```toml
schema = 1

[[files]]
id = "editor"
source = "files/editorrc"
target = "~/.editorrc"
mode = "link"
```

- `id`: unique name for this mapping.
- `source`: file inside the repository. No absolute paths, `..`, `.git` or `dort.toml`.
- `target`: absolute path or `~/...` under your HOME. Dort config, state, the repository and overlapping targets are refused.
- `mode`: `link`, `template` or `secret`. Only links can be deployed. Use `template` for generated previews.
- `profiles`: defaults to `["base"]`. An empty list is invalid.
- `requires`: tool IDs this file needs. Missing or unverified prerequisites block the file plan.
- `permissions`: links do not take one. Templates accept an octal mode; secrets require `"0600"`.

Use platform-specific sources when needed:

```toml
[files.overrides]
macos = "files/macos/shellrc"
linux = "files/linux/shellrc"
```

Symlinks, directories, hard links and special files cannot be adopted as ordinary files.

### Generated preview

```toml
[[files]]
id = "editor-settings"
recipe = "theme = {{ theme }}\n"
target = "~/.config/editor/settings.conf"
mode = "template"
inputs = { theme = "dark" }
```

Dort renders `theme = dark` followed by a newline. Use `recipe` for text in `dort.toml`, or use `source` to point to a UTF-8 file in the repository. An inline recipe cannot use `source` or platform source overrides. Use a single input name in each `{{ ... }}`. Names use ASCII letters, digits and underscores and start with a letter or underscore. Values are literal strings from `dort.toml`, never expanded again. Keep secrets out of recipes and inputs. Dort refuses content that looks like credentials.

Limits: 64 KiB per UTF-8 template, 16 KiB per input value, and 128 KiB of rendered text.

## Profiles

```toml
[profiles]
available = ["dev", "macos", "linux"]
default = ["base"]

[profiles.platform]
macos = ["macos"]
linux = ["linux"]
```

`base` is always active. Other names must appear in `available`.

Effective profiles combine base, manifest defaults, platform profiles, local choices and `--profile` flags. A resource is active if any of its profiles is active. Dependencies must also be active.

Tools use one install rule on every host. Only file overrides and profile selection use platform keys.

## Tools

```toml
[[tools]]
id = "git"
binary = "git"

[tools.install]
provider = "brew"
package = "git"
```

This only describes the tool. Checks and installs come from a separate command. `provider` names the package manager.

Package managers:

- `brew`: Homebrew. Version may be omitted or `latest`.
- `npm`: npm. Version may be default/latest or exact. Installing needs an exact version and a declared runtime.

npm runtime example:

```toml
[[tools]]
id = "node"
binary = "node"

[tools.install]
provider = "brew"
package = "node"

[[tools]]
id = "formatter"
binary = "formatter"
requires = ["node"]

[tools.install]
provider = "npm"
package = "prettier"
version = "3.3.3"
runtime_tool = "node"
```

`requires` must name declared tools, and cycles are refused. An npm install names its runtime with `runtime_tool`; that tool becomes a dependency.

## Validate

```sh
dort manifest check --manifest /absolute/path/to/dort.toml --profile dev
dort --repo /absolute/path/to/dotfiles files list --profile dev
dort --repo /absolute/path/to/dotfiles files plan apply --profile dev
```

`--manifest` works only with `manifest check`; do not combine it with `--repo`.

A valid manifest can still produce a blocked plan. Check the reported reason.

Do not put plaintext secrets in the repository or on the command line. Secret deployment is not available yet.
