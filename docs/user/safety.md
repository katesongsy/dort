# Safety and troubleshooting

[User guide](README.md) · [Command reference](commands.md)

## Read-only does not mean secret-proof

Checks/discovery/plans/status/history/Git inspection do not write. Explicit init/file/recovery mutations can change repositories and live files only after approval. Start with a disposable HOME; these are experimental prototypes. `--dry-run` never creates state/locks/artifacts or prompts. JSON/nonterminal writes require `--yes`; link deletion additionally requires exact-ID acknowledgement. Manifest checks validate declarations; file plans may read/hash ordinary files. Discovery reads metadata only.

Known key/credential locations, caches, browsers and generated data are excluded from discovery. Ordinary adoption also checks sensitive names/content. These are conservative heuristics: they can miss secrets and reject harmless content. Do not rename a sensitive file just to bypass a blocker, and do not paste its contents into a bug report.

Reports can contain personal paths, metadata and hashes. Review them privately before sharing. Do not use `sudo` or weaken permissions to bypass protection.

## Terminal and interruption

The TUI is not a separate safety engine. It consumes actual core previews/opaque approvals, requires literal `yes` (and a separate exact file ID for deletion), and preserves lock/fresh-guard checks. `--dry-run` disables confirmation, and terminals below 48×14 cannot approve. Metadata is escaped before rendering; no file bodies, local variable values or Git patches are displayed. Paths/IDs can still be private.

Esc dismisses a preview/confirmation without writing. Once approved, the worker runs to completion: q/Ctrl-C asks to exit afterward, not to cancel a transaction. Input/render errors restore the terminal and wait for the worker before the process exits. There is no automatic undo, detached mutation, editor/shell suspension or startup recovery replay. Another process may invalidate a preview; execution must then refuse it rather than use stale inputs.

Normal returns, handled terminal errors and panics attempt raw-mode/alternate-screen restoration. A disconnected terminal, SIGKILL or other forced termination cannot guarantee cleanup or completion. If the shell is left unusable, run `stty sane` in that shell and reopen the terminal if necessary. Then inspect `dort history`; do not delete journals, rerun blindly or assume a terminated write made no changes. A clean TUI exit is not proof that all operations succeeded; service errors remain visible in its scrollable error view (Esc dismisses; refresh Recovery afterward).

## Git inspection boundary

`git status` and default `git diff` reports are metadata-only. Only explicitly adding `git diff --patch` discloses file content, which can include secrets. Patch escaping prevents terminal control injection; it does **not** redact credentials. Patches never enter Dort logs/errors/journals, but redirected output is your responsibility. Do not upload patches by default when reporting a problem.

Dort uses your installed trusted Git executable with a cleared child environment, fixed read-only options, no optional locks/index refresh, hooks, external diff/textconv, fsmonitor or submodule inspection/recursion. Global/system Git configuration and global/system attributes are ignored. Repository-configured content filters and partial-clone/promisor declarations are refused conservatively, even if unused/false. Inspection exposes no installation, arbitrary arguments, network or write operation. Separately, approved local init can initialize Git only in its new private stage with templates disabled, then publish the repository without replacement; it never stages, commits, pushes or clones.

This is hardening, **not an OS sandbox or protection from a malicious same-user configuration race**. Git reads repository-controlled configuration/includes/attributes/object paths through Git's own filesystem access, not Dort's no-follow observer. Use trusted installations/repositories. Summary and explicit patch are separate, non-atomic reads. There is no automatic Git merge into live-linked sources.

| Git problem | Action |
| --- | --- |
| Executable not found/unavailable | Install Git separately and use a trusted absolute PATH directory; relative/empty PATH entries are ignored. Init/attachment also needs Git; other commands do not. |
| Filter/partial-clone refusal | Inspect configuration privately, including includes. Use Git yourself with appropriate precautions; Dort offers no bypass or automatic config editing. |
| Invalid repository/root | Supply the exact non-bare working-tree root, not a subdirectory, dummy `.git` directory or bare repository. Linked worktrees are supported; a manifest is not needed. |
| Time/output limit | Each child is limited to 10 seconds and 4 MiB combined stdout/stderr. Narrowing arbitrary Git arguments or lifting bounds is not exposed. No partial report is emitted. |
| Generic Git read failure | Check Git availability/version and repository integrity privately. Stderr is intentionally not echoed because it can contain sensitive values/content. |

Git access may update filesystem access times; “read-only” does not promise otherwise. Tests verify index/config/source bytes, identities, modes and modification times stay unchanged, including a source reached through a live link.

## Where local state lives

Both macOS and Ubuntu use:

| Purpose | Default |
| --- | --- |
| Configuration | `$HOME/.config/dort/local.toml` |
| State | `$HOME/.local/state/dort/` |
| Repository transactions | `repositories/<repository-id>/transactions/<transaction-id>/` below state |

`XDG_CONFIG_HOME` and `XDG_STATE_HOME` override the respective base directories. Empty overrides are ignored; relative overrides are rejected. Missing state is not created by read-only commands or dry runs.

Configuration contains machine-local profiles, variables and repository registration. Approved `init PATH` registers/selects a created or already-valid repository; relocation remains core-only. Do not copy another machine's local configuration into your environment repository.

Private state uses owned directories/files without symlink traversal or hard links. Configuration/checkpoints/backups are private (0600), and transaction directories use 0700. Retained originals/prepared objects can preserve their original modes inside those private directories.

## Common blockers

| Symptom | What to check |
| --- | --- |
| No repository selected | Pass `--repo /absolute/path/to/dotfiles`, or use `manifest check --manifest /path/to/dort.toml`. Approved `init PATH` can register/select a valid repository. |
| Unknown profile or inactive dependency | Declare optional profiles and ensure selected resources have active dependencies. `base` is always active. |
| Valid manifest, blocked file plan | Source/destination inspection and runtime capabilities are separate from schema validation. Read each mapping's blocker. |
| Template/secret or prerequisite blocker | These capabilities are not implemented; a link fallback would be unsafe. |
| Source missing | Create/review the source yourself before an apply preview. Adoption previews instead require a **new**, missing source and an existing safe regular destination. |
| Directory/special/foreign/broken link | Initial ordinary-file adoption does not support these entry types. |
| Hard link, changed identity or symlinked parent | Dort refuses ambiguous paths. Investigate the real path/ownership privately instead of bypassing the guard. |
| Non-UTF-8 path | Current report formats reject it explicitly rather than corrupting its identity. |
| File exceeds observation limit | Checked regular-file reads are bounded to 16 MiB; large/generated data is outside this workflow. |
| Discovery limit/warning | Discovery has a 4,096-name budget. Raced/unreadable nested subtrees produce warnings; an oversized root can fail the command. |

On macOS, aliases such as `/var` may be symlinks. When supplying explicit state paths or temporary demo roots, resolve the physical path with `pwd -P` from inside that directory. Do not rewrite your real HOME/XDG setup merely to run a demo; use the [isolated walkthrough](getting-started.md#try-file-previews-without-using-your-own-configuration).

## Recovery and write blockers

```sh
dort --json recovery list
```

This command never creates state, takes a write lock, repairs records or changes files. Exit 4 means at least one incomplete or malformed transaction blocks subsequent new writes, not that recovery has occurred.

Adoption, link apply and unmanage are experimental shared core/CLI services. Their private journals retain immutable checksummed checkpoints, verified backups and prepared/original objects. Schema 1 remains adoption-specific; schema 2 covers apply/unmanage; dedicated schema 3 covers local bootstrap, without migrating existing records. They require repository/sources, destinations and state on one filesystem with atomic rename support. Ordinary backups are **not encrypted**. Individual operations are atomic; the full multi-path transaction is not.

Apply groups all changed mappings from one approved request into a single recoverable transaction; repeated correct apply creates no transaction. Unmanage keeps repository sources and changes the destination before removing the mapping. Removal needs a separate matching acknowledgement (`--confirm-remove ID` for automation, or a separate typed-ID terminal confirmation). These prototypes support at most 128 selected mappings and 256 operations per request.

`recovery list` includes retained completed transactions and the last validated journal schema. A completed journal is not proof that live files are unchanged or that rollback is still safe. The command never reads live source content to establish those guards.

If a transaction is incomplete:

1. Stop new write attempts; retain the transaction directory, backups and originals. A failed call may have partially published effects.
2. Collect `history` / `recovery list` output and the error; review paths before sharing.
3. For schemas 1/2, inspect `rollback REPOSITORY_ID TRANSACTION_ID --dry-run`. For schema 3, inspect explicit `recovery resume` or `recovery abandon` with both IDs and `--dry-run`. Approve only after reviewing guards/effects; see [syntax](commands.md#explicit-recovery).
4. If guards or integrity checks fail, seek maintainer assistance. Do not delete the journal, remove the lock inode or edit checksums to bypass a blocker. A truncated tail must not be silently discarded.

Rollback is explicit for schemas 1/2 and bound to a fresh guarded preview/journal head, with independent revalidation under the lock. Apply/unmanage watch all selected sources, including no-op mappings, without writing them. Rollback preflights the entire transaction and refuses changed sources (including edits through a live link), destinations, parents, prepared objects, unexpected files in created directories or corrupted backups rather than overwriting user work. A malformed record requires manual diagnosis/repair, not speculative replay. There is no automatic startup rollback or garbage collector.

Bootstrap first stages/syncs a minimal repository privately, then records intent before no-replace repository publication and atomic registration publication/exchange. Configuration backups/retained originals can contain local variable values; they are private, not encrypted. The three locations must share a filesystem. Interruption blocks new writes until explicit guarded resume/abandon; incomplete private preparation can only be abandoned/retried. **Abandon never deletes a published repository, undoes registration or removes retained artifacts.** Edited/substituted repositories/configuration/parents stop recovery. Malformed chains require manual diagnosis. No automatic startup cleanup/replay occurs.

The application lock coordinates Dort writers only, not editors or malicious same-user processes. No-follow access is not full filesystem isolation, and the recovery protocol does not promise safety after every power or storage failure.

## Reliability limits

Dort remains experimental. Repository sources, destinations and state must share a filesystem supporting atomic no-replace/exchange renames; there is no cross-device fallback. Individual publications are atomic, not an entire multi-path transaction. Disk exhaustion, interruption or power loss may leave incomplete work or a malformed checkpoint that requires manual diagnosis. Automatic recovery cannot safely discard a torn immutable tail.

Keep independent backups and start with a disposable environment. If a write fails, preserve journals/backups/originals, inspect history and use only the guarded recovery actions described above. Never fill your disk, weaken permissions or try recovery against real dotfiles merely to work around a blocker.

## Reporting a problem

Include the Dort version, OS/architecture, exact command, exit status and a minimized/redacted manifest when useful. State whether the error came from manifest validation, discovery, a file preview, Git inspection or recovery diagnosis. Omit patch content unless you have privately reviewed/redacted it. Do not upload local variables, private backup files, credential material or full transaction directories.
