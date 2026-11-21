# Dort

Git-native development environment manager for macOS, Linux and WSL. This crate builds the `dort` executable; Rust 1.99.1 is required to build it. Prebuilt archives are on [GitHub Releases](https://github.com/katesongsy/dort/releases).

Dort manages local repositories, file discovery and links, profiles, manifest checks, guarded recovery, read-only Git inspection, and Homebrew/npm tool checks and installs. CLI and TUI share the same services.

Start with [getting started](https://github.com/katesongsy/dort/blob/main/docs/user/getting-started.md), then read [safety](https://github.com/katesongsy/dort/blob/main/docs/user/safety.md).

Not supported: templates, secrets, Git clone/sync, staging/commit/push, and general package upgrades. Writes need approval and fresh checks. Recovery never runs automatically. Backups are private, not encrypted. Tests do not prove power-loss safety.

MIT-licensed.
