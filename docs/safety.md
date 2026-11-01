# Safety and troubleshooting

[User guide](README.md) · [Command reference](commands.md)

## Read-only does not mean secret-proof

The CLI does not change live files, register repositories or execute previews. Manifest checks validate declarations; file plans may read and hash ordinary owned regular files. Discovery reads candidate metadata only.

Known key/credential locations, caches, browsers and generated data are excluded from discovery. Ordinary adoption also checks sensitive names/content. These are conservative heuristics: they can miss secrets and reject harmless content. Do not rename a sensitive file just to bypass a blocker, and do not paste its contents into a bug report.

Reports can contain personal paths, metadata and hashes. Review them privately before sharing. Do not use `sudo` or weaken permissions to bypass protection.

## Where local state lives

Both macOS and Ubuntu use:

| Purpose | Default |
| --- | --- |
| Configuration | `$HOME/.config/dort/local.toml` |
| State | `$HOME/.local/state/dort/` |
| Repository transactions | `repositories/<repository-id>/transactions/<transaction-id>/` below state |

`XDG_CONFIG_HOME` and `XDG_STATE_HOME` override the respective base directories. Empty overrides are ignored; relative overrides are rejected. Missing state is not created by CLI commands.

Configuration contains machine-local profiles, variables and repository registration. Registration/relocation currently exist only as core APIs; do not copy another machine's local configuration into your environment repository.

Private state uses owned directories/files without symlink traversal or hard links. Configuration/checkpoints/backups are private (0600), and transaction directories use 0700. Retained originals/prepared objects can preserve their original modes inside those private directories.

## Common blockers

| Symptom | What to check |
| --- | --- |
| No repository selected | Pass `--repo /absolute/path/to/dotfiles`, or use `manifest check --manifest /path/to/dort.toml`. There is no CLI registration/init command yet. |
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

This command never creates state, takes a write lock, repairs records or changes files. Exit 4 means at least one incomplete or malformed transaction is a blocker for subsequent core writes, not that recovery has occurred.

Core adoption, link apply and unmanage are experimental, with no CLI write interface. Their private journals retain immutable checksummed checkpoints, verified backups and prepared/original objects. Schema 1 remains adoption-specific; schema 2 covers apply/unmanage, without migrating existing records. They require repository/sources, destinations and state on one filesystem with atomic rename support. Ordinary backups are **not encrypted**. Individual operations are atomic; the full multi-path transaction is not.

Apply groups all changed mappings from one approved request into a single recoverable transaction; repeated correct apply creates no transaction. Unmanage keeps repository sources and changes the destination before removing the mapping. Core removal needs a separate matching acknowledgement. These prototypes support at most 128 selected mappings and 256 operations per request.

`recovery list` includes retained completed transactions and the last validated journal schema. A completed journal is not proof that live files are unchanged or that rollback is still safe. The command never reads live source content to establish those guards.

If a transaction is incomplete:

1. Stop further core write attempts; keep the transaction directory, backups and originals intact.
2. Collect `recovery list` output and the relevant error; review paths before sharing.
3. Seek maintainer assistance. There is no end-user recovery write command yet.
4. Do not delete the journal, remove the lock inode or edit checksums to make a blocker disappear. A truncated tail must not be silently discarded.

Rollback is explicit and core-only, through the API corresponding to the journal schema. Apply/unmanage watch all selected sources, including no-op mappings, without writing them. Rollback preflights the entire transaction and refuses changed sources (including edits through a live link), destinations, parents, prepared objects, unexpected files in created directories or corrupted backups rather than overwriting user work. A malformed record requires manual diagnosis/repair, not speculative replay. There is no automatic startup rollback or garbage collector.

The application lock coordinates Dort writers only, not editors or malicious same-user processes. No-follow access is not full filesystem isolation. Tests use disposable homes and process exits at durable boundaries; they do not establish real power-loss safety.

## Reporting a problem

Include the Dort version, OS/architecture, exact command, exit status and a minimized/redacted manifest when useful. State whether the error came from manifest validation, discovery, a file preview or recovery diagnosis. Do not upload local variables, private backup files, credential material or full transaction directories.
