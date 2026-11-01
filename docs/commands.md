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
- `unmanage --mode remove`: preview deleting that link and removing the mapping; marked as needing separate destructive-removal acknowledgement in a future executor.

Both unmanage modes retain the repository source. A drifted or foreign destination is blocked. These modes **do not execute**, collect acknowledgement or change files.

Repeatable `--profile` is supported for file list/plan operations. Human plans show source/destination paths, permissions, backups, effects, conflicts and destructiveness. JSON includes schema-versioned snapshots and hashes; saving it does not create execution authorization.

## Recovery diagnosis

```sh
dort recovery list
dort --json recovery list
```

This inspects **all** local repository transaction histories. Omit `--repo`; it is rejected for this command. Reports include transaction/repository IDs, phase, valid-checkpoint count and integrity problems.

Incomplete or malformed histories are write blockers for core writers. The CLI never replays or rolls back anything, and there is no `recovery rollback` command. See [safety and troubleshooting](safety.md#recovery-and-write-blockers).

## Output and exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command succeeded; not proof deployment is supported. |
| 1 | I/O failure. |
| 2 | Invalid input/configuration, unsupported host or argument error. |
| 3 | Writer lock contention (primarily relevant to core APIs; current CLI commands do not acquire write locks). |
| 4 | A file plan is blocked, or recovery diagnosis reports write blockers. |

Blocked plans/diagnosis still emit their full report on **stdout**. Other runtime errors go to **stderr**, as JSON when `--json` is selected. Clap help/argument-parsing output has its own normal formatting; `--json` does not turn invalid CLI syntax into a JSON report.

Reports expose paths/metadata/hashes but not file bodies or local variable values. These reports can still reveal personal paths or configuration names; review before sharing.
