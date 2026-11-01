# Manifest guide

[User guide](README.md) · [Command reference](commands.md)

An environment directory contains a shared `dort.toml`. Keep machine-local variables/state outside that repository. Schema 1 is strict: unknown fields, duplicate resource IDs, invalid paths/profiles/dependencies and conflicting destinations are errors.

## Minimal file mapping

```toml
schema = 1

[[files]]
id = "editor"
source = "files/editorrc"
target = "~/.editorrc"
mode = "link"
```

Create the repository source yourself before previewing an apply. A manifest check does not read source files; a file plan does.

- `id`: unique among file mappings. Tool IDs use a separate namespace.
- `source`: repository-relative file path; no traversal, absolute paths or protected metadata such as `.git` or `dort.toml`.
- `target`: absolute or `~/…`, expanded below the configured HOME. HOME itself, Dort configuration/state, the environment repository and overlapping ancestor/descendant destinations are forbidden.
- `mode`: `link`, `template` or `secret`. **Only ordinary-file links can currently produce supported file-change previews.** Templates/secrets are validated declarations, but file plans block their execution.
- `profiles`: defaults to `["base"]`; an explicit empty list is invalid.
- `requires`: tool IDs required by this file. Dependencies are validated, but file plans block prerequisites because installed tools cannot yet be verified.
- `overrides`: optional platform-specific repository source paths.
- `permissions`: not allowed for links; templates accept an octal mode, and secrets require `"0600"`. These declarations do not enable deployment.

For example, a source override:

```toml
[[files]]
id = "shell"
source = "files/shellrc"
target = "~/.shellrc"
mode = "link"

[files.overrides]
macos = "files/macos/shellrc"
ubuntu = "files/ubuntu/shellrc"
```

Both source variants must obey the same path restrictions. Foreign/broken links, symlinked source/parent paths, directories, hard links and special files cannot be adopted as ordinary files.

## Profiles

```toml
[profiles]
available = ["dev", "macos", "ubuntu"]
default = ["base"]

[profiles.platform]
macos = ["macos"]
ubuntu = ["ubuntu"]
```

`base` is built in and always selected. Other names must be declared in `available`. Effective profiles combine base, manifest defaults, platform profiles, local active profiles and repeatable command-line `--profile` selections.

A resource with several profiles is selected if **any** is active, not only if all are active:

```toml
[[files]]
id = "editor-dev"
source = "files/editor-devrc"
target = "~/.editor-devrc"
mode = "link"
profiles = ["dev", "macos"]
```

A dependency must also be active; profiles cannot silently disable a required resource. The active platform must have install definitions for selected tools. Command-line profiles affect only that invocation and do not save preferences.

## Tool declarations

```toml
[[tools]]
id = "git"
binary = "git"

[tools.install]
macos = { provider = "brew", package = "git" }
ubuntu = { provider = "apt", package = "git" }
```

This declares intent only: Dort does not inspect or install Git. Supported provider names in the schema are `brew`, `apt`, `mise` and `npm`. Packages/versions are validated for the provider; brew/apt currently allow only omitted or `latest` versions. See the [tracked example](../examples/dort.toml) for mise/npm declarations.

Tool `requires` must name declared tools, and dependency cycles are rejected. An npm install definition must explicitly name its runtime via `runtime_tool`; that runtime also becomes a dependency.

## Validate and inspect separately

```sh
dort manifest check --manifest /absolute/path/to/dort.toml --profile dev
dort --repo /absolute/path/to/dotfiles files list --profile dev
dort --repo /absolute/path/to/dotfiles files plan apply --profile dev
```

`--manifest` belongs to `manifest check` and cannot be combined with global `--repo`. File commands use an environment directory, not an arbitrary manifest filename. A valid manifest can still produce blocked file plans because of unsafe/missing files or unsupported capabilities.

For sensitive data, do not put plaintext secrets into an ordinary source, example or command line. Secret deployment is not available yet; exclusions are heuristics, not a guarantee.
