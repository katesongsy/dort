# Dort architecture

Dort turns a Git-tracked manifest into reviewed local changes. This page covers boundaries and flow. Code and tests own the details.

## Boundaries

- `crates/dort-core` holds UI-independent behavior: manifest/profile rules, file and tool planning, observation, recovery and Git/package-manager adapters.
- `crates/dort` holds the CLI, TUI, host discovery and presentation.
- The pure planner takes explicit facts and returns a plan. It does no I/O, clock reads or randomness.
- Host paths and platform values enter through `HostContext`. Core code does not discover HOME.

## Flow

1. **Read.** Load the manifest and local registration. Discovery and inspection return metadata, never file bodies or raw package-manager output.
2. **Plan.** Resolve profiles and dependencies. Report satisfied, missing, blocked and unsafe items. Plans are previews, not approval.
3. **Approve.** Show exact effects. One real in-memory preview becomes one single-use approval. JSON cannot recreate approval.
4. **Write.** Take the application lock, revalidate sources and paths, record the intended change durably, then mutate. Verification follows.
5. **Recover.** An interrupted or failed write keeps a private record. Recovery is explicit. Nothing replays on startup.

File writes use same-filesystem atomic rename, private backups and safe path checks. Tool installs use the package manager with a fixed command line, cleared environment and bounded output. Git access is read-only with fixed arguments.

## Persistence

- Configuration: `${XDG_CONFIG_HOME:-$HOME/.config}/dort/`
- State: `${XDG_STATE_HOME:-$HOME/.local/state}/dort/`
- Repository history: `state/repositories/<id>/`
- Tool install history: `state/tools-attempts/`

Backups are private, not encrypted. File and install history are separate.

## Safety limits

- Writes require approval, a lock and fresh checks.
- Multiple files are not one atomic filesystem change.
- A failed write can leave partial effects. Recovery is manual.
- Package managers, Homebrew formulae and npm packages run outside Dort. They can change shared state.
- Checks do not isolate a malicious same-user process. They do not prove power-loss safety.

## Interface rules

CLI and TUI use the same services. Every supported workflow needs both. The TUI reuses shared components and shows one foreground action strip. See the [design guide](docs/design-system.md).
