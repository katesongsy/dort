---
# How to use this map
# Pick the feature you changed. Each file names the user path, the harness
# steps, and the observable end state that proves it. One proof per change is
# enough; check the index so you know which entry points you left uncovered.
---

# Dort feature map

A service change usually reaches the CLI and TUI. State which surface you proved.

| Feature | TUI | CLI |
| --- | --- | --- |
| [Help for the current page or form](help.md) | `F1` or `?` outside text fields | — |
| [Repository create and attach](repository.md) | `o` Overview, `F2` More → Create or attach | `dort init PATH` |
| [Profiles, clone and environment](environment.md) | `F2` More | `dort profiles`, `dort clone`, `dort environment` |
| [Files: discover, adopt, apply, keep, remove](files.md) | `f` Files | `dort files ...` |
| [Generated configuration](generated.md) | Files → Managed → Preview generated file | `dort files preview-generated ID` |
| [Tools: declared, installed, history](tools.md) | `t` Tools | `dort tools ...` |
| [Checks: manifest and diagnostics](checks.md) | `c` Checks | `dort manifest check`, `dort doctor` |
| [Git: read-only status and diff](git.md) | `g` Git | `dort git status`, `dort git diff` |
| [Recovery and history](recovery.md) | `r` Recovery | `dort history`, `dort recovery list` |

## Common setup

```sh
python3 .pi/skills/verify-dort/drive.py build
python3 .pi/skills/verify-dort/drive.py new <name>
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env <name>)"
```

`new` already runs `dort --yes init` against `$DORT_REPO` and drops a `demo.txt` in it. Sections use these names throughout.

## Not covered here

Template deployment, secrets, Git sync, staging/commit/push and general package upgrades are not implemented. A journey that needs them fails at the declaration, not at a bug — record the block in that feature's Gotchas instead of weakening the journey.
