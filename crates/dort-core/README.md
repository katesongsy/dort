# dort-core

UI-independent services behind [Dort](https://github.com/katesongsy/dort), an experimental, TUI-first local dotfile manager for macOS and Ubuntu on Intel/x86_64 or ARM64. Building requires Rust **1.99.1**.

This crate contains explicit host context, configuration/manifest validation, bounded observation, pure planning, approved filesystem lifecycles, transaction diagnosis and guarded recovery. Dort's CLI and TUI share these services. Install the `dort` executable rather than this library to use the workbench. The pre-1.0 library API is experimental, not a general-purpose filesystem sandbox.

## Safety boundaries

The pure planner has no filesystem I/O, clocks or randomness. Writers require an application lock, opaque approval, independent fresh validation and immutable recoverable intent. Live/state access uses descriptor-relative no-follow helpers and refuses unsafe ownership, parents, file types and cross-device mutations. Recovery is explicit, refuses uncertain or edited state and never automatically replays on startup. These guards do not isolate a malicious same-user/root process or establish real power-loss/failing-fsync safety.

Read the [architecture](https://github.com/katesongsy/dort/blob/main/ARCHITECTURE.md) and [user safety guide](https://github.com/katesongsy/dort/blob/main/docs/user/safety.md) before integrating. No package-manager/provider execution, repository cloning/sync, arbitrary Git writes, templates or secrets are supported. Reports stay metadata-only except the separately bounded and deliberately scoped content-disclosure services.

MIT-licensed; the source package includes the license and example manifest. Dependencies retain their own licenses.
