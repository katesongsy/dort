---
name: verify-dort
description: Launch Dort in a disposable HOME and prove a workflow end to end — adopt a file, check a manifest, run a tool check, read Git status, or drive the TUI. Use when you need to prove a Dort change works the way a user meets it, on the surface the user reaches it from.
---

# Verify Dort

Dort is a Rust CLI plus a Ratatui TUI over the same services. Both surfaces are real; a workflow only counts as proven when the surface the user reaches it from works.

Everything runs against a throwaway `HOME`. Never point a verification run at your real dotfiles: `dort files apply` rewrites links outside the repository, and recovery is manual.

## Launch

Build once, then create an isolated run:

```sh
python3 .pi/skills/verify-dort/drive.py build
python3 .pi/skills/verify-dort/drive.py new my-proof
```

`new` prints `scratch`, `home`, `repo` and `evidence`, and runs `dort --yes init` for you. Put a shell in that run to seed real user files:

```sh
eval "$(python3 /absolute/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
printf 'demo settings\n' > "$HOME/.demorc"
```

Use the absolute path for `drive.py`: `env` ends with `cd $HOME`, so a relative path stops resolving.

Readiness: `new` exits after `init` returns, and the manifest exists. There is no server to wait for. The TUI is short-lived — each `tui` command launches it, drives it, quits it, and asserts it left the alternate screen and restored terminal modes.

Teardown: `cleanup` (below).

## Doctor

```sh
python3 .pi/skills/verify-dort/drive.py doctor
```

Read-only. Checks cargo and git on `PATH`, that `target/debug/dort` is built and runs, that the scratch base is writable, and that every recorded run's scratch is present, symlink-free, and still has its evidence. Run it whenever a run looks wrong.

One check exists because of a real trap: Dort refuses to open a state directory through a symlink, and macOS `/tmp` is a symlink to `/private/tmp`. A raw `mktemp -d /tmp/...` path fails with `Not a directory (os error 20)`. `drive.py` resolves every scratch path, and `doctor` fails any run whose path is not symlink-free.

## Drive

Three ways in. Pick the lowest one that reaches the user path you are proving.

**CLI, scripted.** Deterministic and JSON-friendly. Output plus exit code is recorded as evidence automatically.

```sh
python3 .pi/skills/verify-dort/drive.py cli my-proof --label add -- \
  --yes --repo @REPO files add --id demorc --source files/demorc @HOME/.demorc
```

`@REPO` and `@HOME` expand to the run's paths. Everything after `--` goes to `dort` untouched, so dort's `--json`, `--yes` and `--dry-run` cannot collide with `drive.py`.

**TUI, scripted.** Steps run in order:

```sh
python3 .pi/skills/verify-dort/drive.py tui my-proof --label managed \
  "wait:loaded." "send:'f'" "wait:Managed" \
  "send:'2'" "wait:Discover" "snap:discover" \
  "send:'\x1b[B'" "snap:selected"
```

| step | meaning |
| --- | --- |
| `wait:TEXT` | block until `TEXT` appears on the rendered screen |
| `send:EXPR` | write bytes; the value is a Python literal, so `send:'\x1b'`, `send:'\x1b[B'` (down arrow), `send:'\x1bOQ'` (F2) |
| `click:TEXT` | click the first visible match for the label |
| `snap:NAME` | settle, then store the rendered screen as evidence |
| `resize:RxC` | resize to `R` rows, `C` columns |
| `sleep:SECONDS` | wait |

A failing `wait` still writes the frames it captured plus the error, so a broken attempt leaves a trace. Exit code and full terminal state are asserted after `q`: the alternate screen must be left and the original termios modes restored.

For a guarded action, Enter can open the review but cannot approve it. Read the remaining effects when prompted. Wait for the `y` action before sending `y` or clicking it. Use Esc to cancel.

The footer is the navigation truth, not the body prose. `drive.py` reuses `scripts/test-terminal.py`'s `Screen`/`Session`, which render the VT stream properly — raw byte matching misses labels that Ratatui does not re-emit.

**Plain shell.** When a journey needs a side effect the two above do not cover, `eval "$(drive.py env my-proof)"` and use the real `dort` binary and the real filesystem. Still record evidence by hand into `$DORT_EVIDENCE`.

## Evidence

Every command appends a numbered file to `temp/verify-dort/runs/<name>/evidence/`. `temp/` is gitignored, and cleanup never deletes this directory.

A finished proof needs all four:

1. **The action.** The preview the user approved, or the keystrokes that reached the state.
2. **The resulting state.** The screen after the action, or stdout after the write.
3. **The side effect, observed directly.** `[ -L "$HOME/.demorc" ]` and `readlink`, the repository source content, the manifest mapping, a transaction directory under `$XDG_STATE_HOME/dort/repositories/*/transactions/`. A screen that says "Applied" is not evidence the link exists.
4. **The exit status.** CLI codes are meaningful: `0` success, `1` I/O, `2` invalid input, `3` writer lock held, `4` blocked plan. Plan `4` prints its body on stdout, so read the body and the code.

Proof standards:

- Drive the real command or keystroke. Never call an internal setter, and never use a test-only hook as the proof.
- Do not trust a dry-run's name. `--dry-run` for `files add` plans a source copy, a manifest edit and a link replacement but writes nothing — confirm no destination appeared and no new transaction directory exists. Some previews still query package managers unless `--dry-run` is passed.
- Writes need `--yes` in JSON/nonterminal mode; without it Dort exits `2`. That is the intended guard, not a bug to work around with a non-JSON call.
- Fake package managers are what the repo's own tests use. Real `brew`/`npm` installs are out of scope: say so in the proof rather than faking a pass.

## Cleanup

```sh
python3 .pi/skills/verify-dort/drive.py cleanup my-proof
python3 .pi/skills/verify-dort/drive.py cleanup --all
```

Removes the scratch `HOME`, repository and state, marks the run cleaned, and keeps its evidence. `--all` also sweeps `dort-verify-*` scratch roots whose run record is gone.

Never kill Dort by process name — an interactive session may belong to the user. `drive.py` runs are synchronous and kill only the child they started, so nothing outlives a command. After cleanup, confirm the evidence is still there and that the scratch path is gone; a cleanup that eats the proof has failed.

## Features

Start at `features/README.md`. Drive the feature you changed; the map lists the other entry points so a proof that only covers the convenient one is visible as incomplete.

## Proof of this skill

This skill was executed end to end on macOS 15 / ARM64 against `dort 0.1.0`: build, doctor, `new`, seed `$HOME/.demorc`, `files discover --json`, `files add`, side effects (symlink, repository source, manifest mapping, two transaction directories), TUI Managed showing `demorc  Linked  .demorc`, cleanup, then doctor confirming the scratch was gone and the evidence kept. That evidence is at `temp/verify-dort/runs/selftest/evidence/`.

Unverified on the stand-in, and therefore not yet proven:

- **Linux and WSL.** The `/tmp` symlink trap and `$TMPDIR` differ; `doctor` is the check that catches a bad scratch path there.
- **Real package-manager installs.** `tools check` and `tools install-missing` were not run against a real `brew` or `npm`.
- **Cross-device and storage-fault behaviour.** Needs separate infrastructure; see `docs/validation.md`.

## Gotchas

- **`/tmp` symlink.** Covered above; `drive.py` handles it. Do not hand-roll `mktemp -d /tmp/...` and pass that path to `dort`.
- **A lock from another run.** Exit `3` means another Dort process holds the writer lock. It is per-state-directory, so a second run with its own `XDG_STATE_HOME` is fine; the same run driven twice at once is not.
- **`files discover` finds nothing.** It lists hidden `$HOME` entries and `$HOME/.config` entries, and skips Dort's own configuration. Seed a dotfile first.
- **Zero tools in the manifest.** `tools check` needs `[[tools]]` declarations. `tools inspect` needs none, but still asks the package manager for approval.
- **Real git.** Repository workflows use the real `git` on `PATH`. The repo's test fixtures fake it; verification should not, because the point is to catch what the fake does not.
