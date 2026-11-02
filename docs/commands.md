# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

Checks, discovery, inventory, plans, status, history and Git inspection are read-only. Explicit mutation commands are experimental and require approval. None installs tools, clones/syncs, stages, commits or pushes. Use `dort --help` and `dort <command> --help` for syntax.

## Global options

- `--json`: structured reports on stdout; structured errors on stderr.
- `--repo PATH`: override the selected environment directory for repository commands. It does not register the directory.
- `--dry-run`: mutation preview only; never creates state/directories/locks/backups/journals or prompts. Read-only commands remain read-only regardless.
- `--yes`: explicit write approval without prompting. Required for JSON/nonterminal writes; does not bypass guards.
- `--confirm-remove ID`: separate exact-ID acknowledgement for deleting a managed link with `files remove --mode remove`.
- `--help`, `--version`: available without a configured HOME.

Other commands require a supported host and HOME. No command starts a TUI. Do not rely on the working directory being auto-discovered as a repository; supply `--repo`.

## Initialization and approval

```sh
dort init /absolute/path/to/dotfiles --dry-run
dort init /absolute/path/to/dotfiles --yes
```

Init requires its explicit positional path (omit `--repo`). Relative paths resolve from the current directory; use physical absolute paths to avoid symlink aliases. It either creates a new minimal local Git repository and manifest at an **absent** destination, or registers/selects an already-valid non-bare Git root containing a host-valid `dort.toml`. Arbitrary existing/empty directories are refused. The destination's parent must already exist, be owned and not group/world writable. HOME/root and Dort metadata overlaps are forbidden.

Creation initializes only a private staged repository with templates disabled, branch `main`, SHA-1 objects and minimal `schema = 1` manifest. It journals intent before no-replace publication and then registration. No cloning/network/hooks/staging/commit occurs. State/configuration/repository must share a filesystem supporting atomic renames. Interrupted bootstrap blocks new writes; use explicit recovery below. Approved init/attach needs an installed trusted Git, with the same environment/configuration hardening as inspection. Attachment refuses filters/promisor configuration.

Human terminal mode (stdin/stdout/stderr all terminals, no `--json`) presents a preview then asks for literal `yes`. Remove mode additionally asks for the exact ID if no matching flag was supplied. `--yes` bypasses prompts, **not** the separate remove acknowledgement. JSON/nonterminal writes always require explicit flags and never prompt. Dry runs skip all approvals and mutations. Saved preview JSON is not execution authorization.

## Host and manifest checks

```sh
dort doctor
dort --json doctor
dort manifest check --manifest /path/to/dort.toml
dort --repo /path/to/dotfiles manifest check --profile dev --profile ubuntu
```

`doctor` shows platform/architecture, configuration/state locations and repository counts. It does not check installed tools or repair configuration.

`manifest check` validates schema, profiles, destination rules and dependency order. It does not inspect live file sources or installed tools. It explicitly reports unsupported runtime capabilities. `--manifest` and global `--repo` are mutually exclusive; without either, it uses the registered active repository.

## File discovery and inventory

```sh
dort files discover
dort --repo /path/to/dotfiles files discover
dort --repo /path/to/dotfiles files list --profile dev
```

`discover` needs no repository unless one is selected explicitly or locally. It examines metadata for hidden HOME entries and up to three levels beneath `.config`, with a 4,096-name budget. Other HOME directories are not traversed. Protected paths and known sensitive/generated locations are excluded. It does not follow directory symlinks or read candidate contents.

`list` observes active declared mappings and their sources/destinations. Inventory is not a safety decision; use a plan to see blockers/effects.

## File previews

```sh
dort --repo /path/to/dotfiles files plan apply --profile dev
dort --repo /path/to/dotfiles --json files plan apply
dort --repo /path/to/dotfiles files plan adopt '~/.editorrc' \
    --id editor --source files/editorrc
dort --repo /path/to/dotfiles files plan unmanage editor --mode keep
dort --repo /path/to/dotfiles files plan unmanage editor --mode remove
```

- `apply`: preview active link mappings. Correct managed links are no-ops. Replacing a safe ordinary destination includes a backup effect; unsafe destinations block the mapping.
- `adopt`: preview an existing owned, single-link regular destination into a new source/new base-profile mapping. Source copy, manifest addition, backups and destination link form one proposed transaction. Existing sources/mappings or unsafe/sensitive entries block adoption.
- `unmanage --mode keep`: preview replacing the currently correct managed link with a materialized regular file and removing its mapping.
- `unmanage --mode remove`: preview deleting that link and removing the mapping; marked as needing separate destructive-removal acknowledgement, not collected by preview commands.

Both unmanage modes retain the repository source. A drifted or foreign destination is blocked. These modes **do not execute**, collect acknowledgement or change files.

Repeatable `--profile` is supported for file list/plan operations. Human plans show source/destination paths, permissions, backups, effects, conflicts and destructiveness. JSON includes schema-versioned snapshots and hashes; saving it does not create execution authorization.

## File mutations, status and plan

```sh
dort files add '~/.editorrc' --id editor --source files/editorrc --dry-run
dort files add '~/.editorrc' --id editor --source files/editorrc --yes
dort status --profile dev
dort plan --profile dev
dort files apply --dry-run
dort files apply --yes
dort files remove editor --mode keep --yes
# Alternative to keep, while the mapping and correct link still exist:
# dort files remove editor --mode remove --yes --confirm-remove editor
```

These use `--repo` or the active registered repository. Mutation requires approved prior `init`/attachment. `files adopt` aliases `add`; `files unmanage` aliases `remove`. Each supports repeatable `--profile`.

- `add` executes the same one-file adoption request as `files plan adopt`: new base-profile mapping, new source, manifest edit and live link in one recoverable transaction.
- `apply` groups all changed active ordinary links into one transaction. Correct links remain untouched; entirely correct/empty apply returns a null transaction ID and creates no journal/backups.
- `remove --mode keep` materializes the source at the correct link before removing its mapping. `--mode remove` withdraws the correct managed link before mapping removal and requires separate matching acknowledgement. Both retain repository sources unchanged; drift blocks the request.
- Top-level `status` and `plan` both return the read-only active apply plan: current observations, effects and blockers. They do not execute, even with `--yes`. This is file lifecycle status, not `git status`.

Dry-run output is a preview; blocked requests return their plan with exit 4. Writes re-observe under the application lock and reject stale approval. On any execution error, effects may already exist: inspect history rather than retrying blindly. No automatic undo occurs. The supported subset remains owned regular-file links; templates/secrets/tool prerequisites stay blocked. Same-filesystem constraints and source-edit recovery guards apply.

## Git inspection

Requires an installed, trusted Git in an absolute PATH directory. No automatic installation occurs. `--repo` must name the exact non-bare working-tree root; linked worktrees are supported, subdirectories are rejected. Git commands need no `dort.toml` or registration.

```sh
dort --repo /path/to/dotfiles git status
dort --repo /path/to/dotfiles --json git status
dort --repo /path/to/dotfiles git diff
dort --repo /path/to/dotfiles git diff --staged
# Explicitly disclose CONTENT (possibly secrets); do not share blindly.
dort --repo /path/to/dotfiles git diff --patch
dort --repo /path/to/dotfiles --json git diff --staged --patch
```

- `status`: metadata-only branch headers and changed/untracked entries, with two-column index/worktree codes where applicable and rename original paths. Unborn and detached HEADs and unmerged entries are supported. Ignored files and submodule changes are excluded.
- `diff`: tracked worktree versus index; `--staged` compares index versus HEAD (or the empty tree before the first commit). Metadata-only summaries contain change codes, old/new modes and paths. Rename detection is disabled for diff: a rename appears as deletion/addition. Untracked contents are never shown.
- `--patch`: the **only content-disclosure option**, accepted only by `git diff`. Adds a separate bounded display patch. All non-ASCII/control bytes and backslashes are escaped, except LF line breaks. This is not a directly applicable patch file. Binary changes produce markers, not binary payloads. It can expose secrets; there is no secret-proof filtering.

JSON reports have `schema: 1`, `read_only: true` and `entries`. Status adds `branch` (Git branch-header strings without the `# branch.` prefix), with entry `kind`, nullable `xy`, `path` and nullable `original_path`. Diff adds `staged`, entry `change`, `old_mode`, `new_mode`, `path`, and `patch` only when requested. JSON escapes control/non-ASCII characters without changing decoded path identity; human metadata is quoted. Non-UTF-8 paths/metadata fail explicitly. Summary and patch reads are not one atomic snapshot.

No optional locks/index refresh, hooks, external diff/textconv, fsmonitor or submodule recursion are permitted. Global/system Git configuration is ignored. Configured content filters and partial-clone/promisor settings are refused even if unused/false; errors are deliberately generic and never include Git stderr. Each child has a 10-second deadline and combined 4 MiB stdout/stderr limit. Exceeding either fails without a partial report. There are no revision/pathspec passthrough arguments, writes, fetches or merges. See the [Git boundary and troubleshooting](safety.md#git-inspection-boundary).

## Recovery diagnosis

```sh
dort recovery list
dort --json recovery list
dort history
```

This inspects **all** local repository transaction histories. Omit `--repo`; it is rejected for this command. Reports include transaction/repository IDs, phase, last validated `journal_schema` (1 adoption, 2 apply/unmanage, 3 bootstrap, or null), valid-checkpoint count and integrity problems. Both completed and incomplete transactions are retained; this is journal history/integrity diagnosis, not a check for current live-file drift.

Incomplete or malformed histories block new writers. Diagnosis never replays or repairs them. Schema-3 bootstrap intermediate steps appear as running; abandoned appears as rolled_back (it does **not** mean the repository was deleted or registration undone). See [safety and troubleshooting](safety.md#recovery-and-write-blockers).

## Explicit recovery

Copy both 64-hex IDs from history and omit `--repo` (rejected for recovery); these are placeholders, not literal usable IDs:

```sh
dort rollback REPOSITORY_ID TRANSACTION_ID --dry-run
dort rollback REPOSITORY_ID TRANSACTION_ID --yes
# Schema-3 bootstrap only; choose resume OR abandon after reviewing:
dort recovery resume REPOSITORY_ID TRANSACTION_ID --dry-run
dort recovery abandon REPOSITORY_ID TRANSACTION_ID --dry-run
```

`recovery rollback` is an alias route for top-level `rollback`, handling schemas 1/2 only. Its preview checks affected guards and conservatively lists inverse paths without mutation. Approved execution independently revalidates the journal head/layout under the lock, then performs restartable guarded inverses. Completed transactions may be rolled back only while guards still match; later transactions/edits can invalidate them. An already-rolled-back transaction has no inverse paths.

For schema 3, remove `--dry-run` and confirm (or add `--yes`) to resume or abandon. Resume recognizes only exact prepared/published identities/content and exact before/after registration; edits/substitutions are refused. Private incomplete preparation can only be abandoned and retried. **Abandon retains published repositories, registration and private artifacts**, merely marking terminal; it never deletes or silently undoes them. If registration never published, a retained repository can be attached later via init.

There is no automatic startup replay/rollback. Malformed chains, uncertain layouts, edited sources/destinations/parents or damaged backups require investigation, not a force flag. Do not discard damaged checkpoint tails. Recovery flags approve only a newly observed preview, not a saved JSON document.

## Output and exit codes

| Code | Meaning |
| --- | --- |
| 0 | Read succeeded or explicitly approved operation completed; not a production-safety guarantee. |
| 1 | I/O failure. |
| 2 | Invalid input/configuration/approval, stale or unsafe guards, malformed history, unsupported host/arguments, unavailable or failed/refused/bounded Git operation. |
| 3 | Writer lock contention; dry runs/read-only commands never acquire it. |
| 4 | A file plan is blocked, or recovery diagnosis reports write blockers. |

Blocked plans/diagnosis still emit their full report on **stdout**. Other runtime errors go to **stderr**, as JSON when `--json` is selected. Clap help/argument-parsing output has its own normal formatting; `--json` does not turn invalid CLI syntax into a JSON report.

Successful mutation JSON has `schema: 1`, `operation`, `completed: true`, and nullable `transaction_id`; init additionally reports `repository`. Human mutation mode presents its preview before confirmation and then the same structured completion report. JSON mode emits a single preview (dry-run/blocked) or completion report, never prompts. Runtime errors use `error`/`kind` on stderr; an error can follow partial durable effects, so inspect history. Exit codes alone do not mean no effects occurred.

Ordinary reports expose paths/metadata/hashes but not file bodies or local variable values. Explicit `git diff --patch` additionally discloses bounded escaped content, which may contain secrets. Even metadata reports can reveal personal paths or configuration names; review before sharing.
