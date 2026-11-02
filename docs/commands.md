# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

**Every command below is read-only.** None installs tools, creates backups, writes registration, executes a preview or repairs a journal. Use `dort --help` and `dort <command> --help` for current argument syntax.

## Global options

- `--json`: structured reports on stdout; structured errors on stderr.
- `--repo PATH`: override the selected environment directory for repository commands. It does not register the directory.
- `--help`, `--version`: available without a configured HOME.

Other commands require a supported host and HOME. No command starts a TUI. Do not rely on the working directory being auto-discovered as a repository; supply `--repo`.

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
- `unmanage --mode remove`: preview deleting that link and removing the mapping; marked as needing separate destructive-removal acknowledgement (available only in the experimental core API, not collected by this command).

Both unmanage modes retain the repository source. A drifted or foreign destination is blocked. These modes **do not execute**, collect acknowledgement or change files.

Repeatable `--profile` is supported for file list/plan operations. Human plans show source/destination paths, permissions, backups, effects, conflicts and destructiveness. JSON includes schema-versioned snapshots and hashes; saving it does not create execution authorization.

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
```

This inspects **all** local repository transaction histories. Omit `--repo`; it is rejected for this command. Reports include transaction/repository IDs, phase, last validated `journal_schema` (1 for adoption, 2 for apply/unmanage, or null), valid-checkpoint count and integrity problems. Both completed and incomplete transactions are retained; this is journal history/integrity diagnosis, not a check for current live-file drift.

Incomplete or malformed histories are write blockers for core writers. The CLI never replays or rolls back anything, and there is no `recovery rollback` command. See [safety and troubleshooting](safety.md#recovery-and-write-blockers).

## Output and exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command succeeded; not proof deployment is supported. |
| 1 | I/O failure. |
| 2 | Invalid input/configuration, unsupported host/arguments, unavailable or failed/refused/bounded Git read. |
| 3 | Writer lock contention (primarily relevant to core APIs; current CLI commands do not acquire write locks). |
| 4 | A file plan is blocked, or recovery diagnosis reports write blockers. |

Blocked plans/diagnosis still emit their full report on **stdout**. Other runtime errors go to **stderr**, as JSON when `--json` is selected. Clap help/argument-parsing output has its own normal formatting; `--json` does not turn invalid CLI syntax into a JSON report.

Ordinary reports expose paths/metadata/hashes but not file bodies or local variable values. Explicit `git diff --patch` additionally discloses bounded escaped content, which may contain secrets. Even metadata reports can reveal personal paths or configuration names; review before sharing.
