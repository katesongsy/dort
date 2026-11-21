# dort-core

Core services for [Dort](https://github.com/katesongsy/dort), a Git-native development environment manager for macOS, Linux and WSL.

This crate holds manifest and profile rules, observation, planning, file and tool workflows, recovery and read-only Git inspection. The CLI and TUI in `dort` share these services. Install the `dort` executable to use Dort.

The library API is experimental. It is not a general-purpose filesystem sandbox. Writes need approval, a lock and fresh checks. Recovery is explicit. See [architecture](https://github.com/katesongsy/dort/blob/main/ARCHITECTURE.md) and the [user safety guide](https://github.com/katesongsy/dort/blob/main/docs/user/safety.md).

MIT-licensed. Building requires Rust 1.99.1.
