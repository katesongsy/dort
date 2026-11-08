# Dort

Experimental, TUI-first local dotfile management for **macOS and Ubuntu**, on Intel/x86_64 or ARM64. This crate builds the `dort` executable; Rust **1.99.1** is required to build it. Ready-to-run binaries are available from [GitHub Releases](https://github.com/katesongsy/dort/releases).

Dort provides guided local repository creation/attachment, file discovery and reviewed adoption/link apply, Keep/Delete unmanage, profiles, manifest checks, guarded recovery, and read-only Git inspection with deliberate patch disclosure. The CLI supports automation; supported human workflows also have TUI paths.

Start with the [installation and disposable-HOME walkthrough](https://github.com/katesongsy/dort/blob/main/docs/user/getting-started.md), then read the [safety guide](https://github.com/katesongsy/dort/blob/main/docs/user/safety.md) before using real configuration. Run `dort --version` to inspect the executable, or `dort` in a terminal to open the workbench.

## Safety boundaries

Dort remains experimental. It does not execute package managers/providers, clone/sync repositories, stage/commit/push Git changes, render templates or manage secrets. Supported writes require explicit approval, fresh validation and recoverable intent; recovery never automatically replays or overwrites later edits. Managed paths must satisfy same-filesystem and ownership/type guards. Backups are private, not encrypted. Tests do not establish real power-loss or failing-fsync safety.

The source package includes its MIT license and example manifest. Source-built dependency and Rust standard-library licenses apply separately; prebuilt archives bundle their notices. See the [repository](https://github.com/katesongsy/dort) for maintained architecture and usage documentation.
