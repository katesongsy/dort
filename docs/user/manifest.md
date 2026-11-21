# Manifest guide

An environment repository contains `dort.toml`. It describes files, tools and profiles.

`schema = 1` is the manifest format version. Unknown fields, duplicate IDs, invalid paths and broken references are errors.

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
- `mode`: `link`, `template` or `secret`. Only links can be deployed today. Templates and secrets are validated but blocked.
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

This only describes the tool. It does not check or install anything by itself. `provider` names the package manager.

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
