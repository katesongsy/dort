# Recovery and history

Inspect every local operation record and act on an interrupted one. Recovery is explicit: nothing replays on startup.

## Sub-features

- **All repositories** (`r`): operations across every registered repository, with a blocked-writes count.
- **Details** (`Enter`): one record's effects and state.
- **`R` retry**: after a failure, retries a read or asks for a fresh preview. It never replays an approval.
- **Rollback**: reverse an unchanged file or declaration operation. Refuses changed state.
- **Resume**: continue an intact repository setup.
- **Abandon**: mark an incomplete setup abandoned. Keeps any published repository and registration.

## How to get to it (user POV)

TUI: `r`. Footer shows `Enter Details`. Filter with `/`.

CLI:

```sh
dort history
dort --json recovery list
dort rollback REPOSITORY_ID OPERATION_ID --dry-run
dort recovery resume REPOSITORY_ID OPERATION_ID --dry-run
dort recovery abandon REPOSITORY_ID OPERATION_ID --dry-run
```

## Driving it with harness

Every write creates a record, so a fresh run already has one:

```sh
python3 .pi/skills/verify-dort/drive.py tui my-proof --label recovery \
  "wait:loaded." "send:'r'" "wait:All repositories" "snap:recovery"
python3 .pi/skills/verify-dort/drive.py cli my-proof --label history -- history
```

Rolling back the adoption from `files.md`:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
dort --json recovery list
dort rollback <REPOSITORY_ID> <OPERATION_ID> --dry-run
```

## Observable end state

- `history` lists operations with type, state and status. A completed `Repository setup` and a completed file operation are the expected pair after `new` plus an adoption.
- The TUI header reads `Recovery / All repositories` with `N/N shown | Blocked writes: 0`.
- A rollback that succeeds removes the mapping and restores the destination; a rollback over changed state exits nonzero with a diagnosis and touches nothing.
- `Blocked writes: 0` after a clean teardown is the signal that nothing is left to resolve.

## Gotchas

- **Changed state blocks rollback.** Editing the linked file or the manifest after the operation makes it refuse. That refusal is the correct behaviour to prove, not a bug.
- **An unresolved record blocks new writes.** A blocked operation names its repository and action; open Recovery.
- **`R` after a failed write only refreshes.** Retry stays manual: `h` (or `r`) opens Recovery.
- **Abandon keeps the repository and registration.** It is not a cleanup.
- **`--dry-run` on every recovery verb** prints the diagnosis without changing anything. Use it first, and keep its output as evidence.
- **Records are per state directory.** A run with its own `XDG_STATE_HOME` sees only its own history.
