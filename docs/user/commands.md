# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

Checks, discovery, inventory, plans, status, history and Git inspection are read-only. Explicit mutation commands are experimental and require approval. None installs tools, clones/syncs, stages, commits or pushes. Use `dort --help` and `dort <command> --help` for syntax.

## Global options

- `--json`: structured reports on stdout; structured errors on stderr.
- `--repo PATH`: override the selected environment directory for repository commands. It does not register the directory.
- `--dry-run`: mutation preview only; never creates state/directories/locks/backups/journals or prompts. Read-only commands remain read-only regardless.
- `--yes`: explicit CLI write approval without prompting. Required for JSON/nonterminal writes; does not bypass guards. Not accepted by the TUI.
- `--confirm-remove ID`: separate exact-ID acknowledgement for deleting a managed link with `files remove --mode remove`.
- `--help`, `--version`: available without a configured HOME.

Other commands require a supported host and HOME. Terminal invocation without a subcommand opens the TUI; nonterminal invocation without a subcommand fails promptly with exit 2. Do not rely on the working directory being auto-discovered as a repository; use the selected repository or supply `--repo`.

## Interactive interface

The interface provides Overview/Welcome, guided local create/attach, session repository/profile selection, Files discovery/adoption/apply/unmanage, structural Checks, all-repository Recovery and read-only Git inspection. The workbench uses highlighted tabs, focused pane borders, state columns, action buttons, and bordered fields with a caret. Color supplements explicit selection markers and state labels. Space opens searchable Actions; `?` opens Help, and Actions → About shows the build version and capability limits.

```sh
dort                       # terminal only; same as dort tui
dort --repo /absolute/path/to/dotfiles tui
dort --dry-run tui         # inspection/previews only; confirmation disabled
```

All three standard streams must be terminals. TUI entry rejects `--json`, `--yes` and `--confirm-remove`; no flags silently approve TUI writes. Use explicit CLI subcommands for automation. Startup observes only: it does not create missing configuration/state, acquire a lock or replay recovery. Without a repository, Welcome offers Choose repository and Create/attach; Recovery still diagnoses all retained local transactions. Discovery and standalone manifest checks work without a repository. Git also accepts an independent session-only working-tree root without a manifest or registration.

| Key | Action |
| --- | --- |
| `1`, `2`, `3`, `4`, `5` | Change destination: Overview, Files, Recovery, Checks, Git, outside text entry. |
| Left / Right | Change destination directly, outside text entry, previews and dialogs. Private patch arrows scroll content instead. |
| Tab / Shift-Tab | Move among fields/buttons of an open form only. No workbench pane-focus cycle. |
| Space, `/` | Space opens Actions with search focused: type a label immediately. Up/Down chooses results; `/` refocuses search only from results. In Files/Git/Recovery content, `/` searches displayed metadata. Esc leaves search, retaining its filter; Delete outside text clears it. |
| `?` | Help; Esc returns to the prior destination. About/version is in Actions. |
| `p`, `n` | Choose repository (session only), create/attach local repository. |
| Up / Down | Select a metadata row, without changing destination. PgUp/PgDn scrolls its details. |
| PgUp / PgDn; Home | Scroll view/form details; Home resets report/help/error scroll outside text entry. In previews, Home resets both axes. |
| `i`, Enter (Files) | `i` inspects any row; Enter folds a directory or inspects a file. Wide layouts show list/details together; compact layouts use Esc to return to the list. |
| `r` | Refresh observations/current check/Diagnostics, independent all-repository history in Recovery, or Git metadata (never a patch). In an error, retry the failed read/fresh preview; after a failed write, refresh only. |
| `g`, `z`, `u`, `s` (Git content) | Choose exact Git-only root; Status; Unstaged; Staged. |
| `w` (Git content) | View patch content: warning then literal `view patch` for one whole-comparison read. |
| `i`, Enter (Git content) | Inspect selected metadata details; Esc returns to list. |
| `f`, `o` | Discover file metadata; open retained manual adoption form. After candidate inspection, press `e` or click Adopt selected for an eligible regular file; ineligible entries remain inspectable but cannot open a candidate draft. |
| `x`, `h` / `l` (Files) | Toggle marks; fold / unfold selected directory. A file's `h` folds its parent. Click chevrons/checkboxes for the same controls. |
| `E` (Discover); `A` / `K` / `D` (Managed Files) | Edit marked adoption drafts; preview selected apply / keep / delete links. |
| `t` | Temporary repository profile additions; cannot remove mandatory contributors. |
| `c`, `v` | Check current repository manifest; open standalone manifest check form. |
| `j`, `m` (Checks) | Read host/registry Diagnostics; return to the manifest result. |
| `i`, Enter (Recovery content) | Full transaction IDs, schema/phase, integrity, checkpoint count and contextual action hints. Esc returns to the list. |
| `h` (error) | Open and refresh independent all-repository Recovery; no repair or replay. |
| `a` | Preview apply (Overview / Files). |
| `k` / `d` | Preview unmanage-and-keep / delete managed link for the selected file. Both retain its repository source. |
| `b` | Preview guarded schema-1/2/4 rollback of the selected transaction. |
| `s` / `x` (Recovery) | Preview schema-3 bootstrap resume / abandon. Abandon retains published data/artifacts. |
| Up / Down (file review); `i` | Select a mapping in the summary; toggle the complete report. Selection never reduces scope. PgUp/PgDn scroll selected effects. |
| Left / Right (full preview report) | Scroll long report lines horizontally. |
| Enter (preview) | Open confirmation, unless blocked, dry-run or smaller than 48×14. |
| Esc | Confirmation → preview → retained form → invoking destination; errors/help return to their prior scope. No execution is started. |
| Ctrl-U (text field) | Clear current field. Backspace edits; Tab/Shift-Tab moves between fields/buttons. Enter advances adoption/check/path fields, then activates the focused Review/Check/Select button. |
| `q` / Ctrl-C | Quit outside text entry / anywhere; an active worker finishes first. Printable `q`, `?`, `/` are text in forms/search. |

All five destinations remain labeled at compact widths, and Welcome leads with create/choose/discover/check guidance. Preview scrolling is bounded to the escaped report and current viewport; Home returns to its origin. File reviews show typed change/unchanged/blocked counts, a mapping table and selected effects. `i` opens the full report for complete permissions, warnings and guards. Review the full effects before Enter. Type literal `yes` and Enter to execute. Deletion first requires its exact file ID in a separate acknowledgement, then `yes`; typing yes alone cannot delete a link. Execution independently revalidates under the same application lock as the CLI. Read-only inspection remains available while another process holds that lock. During work, destination navigation, Help, scrolling, redraw/resize and exit requests stay responsive; scope changes and new jobs are disabled. Only one service worker runs at a time. Forms/pickers keep the focused control visible at compact sizes; long field values show an escaped tail with full input available through PgDn. Confirmation PgUp/PgDn scrolls the prompt independently of its input, keeping long exact IDs inspectable. Shrinking below 48×14 clears confirmation text and the deletion acknowledgement; growing requires both again.

**Metadata navigation:** Files, Recovery and Git show list/details together at 110 or more columns with enough height (22 rows; Git Status with branch strip needs 23). Up/Down always selects rows; Enter/`i` inspects, PgUp/PgDn scrolls details independently and Home returns to the start. In compact layouts, inspection/page keys show details and Esc returns to the list. Resize retains visible inspection, never an invisible focused pane. Tab does nothing outside forms. Inspection never requests a preview or patch; private patch input/scrolling remains separate.

**Mouse:** click destination tabs, metadata rows, filters and labeled action buttons. A row click selects only; Inspect opens compact details and Adopt selected remains a separate action. Wheel over a list selects rows; over details, previews or a dialog it scrolls that area. Click form fields to edit or buttons to request their action. In repository selection, click a row then Enter; profile checkboxes toggle on click and Apply remains separate. Mouse input never submits deletion acknowledgement, literal `yes`, or `view patch`; those require typing and Enter. Busy work cannot start a second action. Mouse capture is restored on normal exit, errors and unwinding where terminal cleanup is possible.

**Actions:** current-item and current-view actions lead the grouped table; all supported actions remain searchable by label, case-insensitively. Up/Down chooses a result and Enter requests it; filtering/highlighting never changes scope or starts work. Mutation previews and content disclosure are separate groups, never approvals. An `x` marks an unavailable action; Enter leaves Actions open with its prerequisite reason and no task. PgDn opens the complete selected label, query, reason and limits from their first row; PgUp/PgDn scrolls and Home returns to controls. Use Up/Down for results and `/` to edit search; Tab does not change workbench focus. Esc first leaves search, then closes Actions and restores the invoking destination. Printable `q`, `?`, `/` stay literal while search is focused; Ctrl-U clears it. Use page buttons or direct keys for ordinary work without opening Actions.

**Overview:** summary cards show registration, active file count and global history blockers. Short layouts use a compact summary strip. PgUp/PgDn inspects the full root, selection origin, profile union, linked/needs-apply/blocked counts, observation errors and retained operation result; Home returns to the start. Unknown counts mean unavailable observations, not a healthy zero. A zero history-blocker count does not establish live-file safety or authorize a write.

**Choose repository:** the table separates exact-path entry, the registered active root and known roots; a separate Create/attach button opens its retained draft. Up/Down selects a row/button; Enter activates it. The registered-active choice resolves configuration, not the current explicit root. PgDn opens full selected/current paths, registry availability and selection details at their first row; further PgUp/PgDn scrolls, and Home restores the controls. Selection is session-only: no registration/configuration write or upward repository search occurs. The compact header identifies the scope; Overview shows the full root, selection origin, profiles and registration or observation errors. An unregistered root can be inspected but requires approved attachment before writing. History always includes all repositories.

**Create/attach:** enter a destination; Enter advances to Review read-only preview, and Enter on that button requests the preview. PgDn exposes full draft/resolved-path details; Home returns to the field controls. Relative input resolves from the directory where Dort was launched; no tilde/environment expansion occurs. An absent destination creates a minimal repository; an existing valid Git root with `dort.toml` is attached. The existing parent must be safe; arbitrary existing directories are refused. Git is resolved at startup; if unavailable, install it separately and restart Dort. Esc/error retains the input in memory; nothing is saved on exit. Only preview approval with literal `yes` persists registration/selection. On success the session switches to that root and Overview shows the transaction ID, or returns to an invoking adoption draft. Other operations retain their transaction ID or explicit no-op result through refresh; Esc clears the result.

**Discover/adopt:** `f` opens bounded metadata candidates, with kind/link/eligibility hints, warnings and the heuristic notice. A selected repository protects its containers; an invalid selected root is an error, not unscoped discovery. The table separates name, metadata eligibility and kind. Directories, links, special files and regular files failing ownership/link-count/special-mode checks are not eligible; select `i` for details. Enter folds/unfolds directories or inspects files without changing the retained draft; `i` inspects any row. After inspection, `e` or clicking Adopt selected fills editable TARGET/ID/SOURCE suggestions only for an eligible regular file; an ineligible row keeps its explanation and retained draft. Alternatively find Adopt selected in Actions; `o` opens the retained manual draft. Tab/Shift-Tab selects fields or buttons, and Enter on Review requests a fresh read-only preview. SOURCE is repository-relative. The new link mapping belongs to `base`; requested profiles do not change its membership. Choose/Create/attach buttons preserve all adoption input and return after selection/attachment. Esc/errors retain drafts in memory; Discard clears the draft. Core path/kind/sensitivity/collision refusals remain authoritative. No candidate/file contents are displayed: review content privately before adoption.

**Tree and batch actions:** Files derives its hierarchy from existing metadata, without extra filesystem traversal. ASCII chevrons and checkboxes work without special fonts; color supplements eligibility/state labels. `h`/`l` folds/unfolds, and `x` toggles a file mark. On a group, `x` toggles all matching descendants, including folded rows; discovery marks regular files only, not directories or links. Filters affect group-marking candidates, never existing marks. The header reports marked and hidden counts. Actions offers Clear marks and Expand/Collapse all. Marks are session-local; managed marks clear on repository change. Refresh does not silently discard missing marked IDs: clear/reselect them if the next preview refuses them.

In Discover, `E` opens retained TARGET/ID/SOURCE drafts for every marked file. Use Tab to select fields or Prev/Next/Review batch; activate Next to edit another file, then Review batch to request one combined preview. Suggestions can collide; edit them rather than silently dropping files. In Managed Files, uppercase `A`, `K`, `D` preview Apply selected, Keep selected, Delete selected links. Lowercase `a` remains full active-scope Apply. Hidden/folded marks remain included; row selection in the review never narrows the approved set.

A batch contains 1–32 explicitly chosen files, one action and one repository. Every item and cross-item conflict is validated before live changes; one blocker refuses the whole batch. Review all effects, then one literal `yes` approves one schema-4 transaction. Delete separately prompts for **every exact mapping ID**, then one final `yes`; mouse clicks cannot acknowledge or approve. Shrinking below 48×14 clears the entire acknowledgement sequence. Cancellation/dry-run creates no transaction. Failures during execution may leave partial effects: retain history/backups and use explicit guarded whole-transaction rollback, never an automatic undo or per-file retry queue. Adoption rollback withdraws its new sources into private storage; selected apply/keep/delete and their rollback leave repository sources unchanged. These batch mutation controls are TUI-only; existing CLI commands are unchanged. Recovery remains individually guarded, not bulk.

**Managed inventory:** `2` shows the active mappings with source/destination kinds, drift and blockers in details. Invalid observation is reported as unavailable, never as a healthy empty inventory. `/` filters display only; `a` always previews the complete active-profile scope, even with no displayed matches. Esc leaves search without clearing the filter; Delete or Actions → Clear Files / Recovery display filter removes it. Refresh preserves selected identity where present. Actions → Keep local file: enter exact mapping ID / Delete managed link: enter exact mapping ID accepts a valid ID even outside the current filter. Enter advances the ID field to Review; only the subsequent preview can proceed to approval. Both modes retain repository sources; deletion still needs exact-ID acknowledgement and literal yes.

**Git:** `5` opens metadata inspection. Its root defaults to the current file repository; `g` accepts an independent exact non-bare working-tree root. Relative paths resolve from the launch directory without tilde/environment expansion. This requires no manifest/registration and never changes file scope or configuration. Actions → Git: use current file repository root restores the default. `z` reads Status (branch headers, kind/XY, names and rename originals); `u` reads tracked worktree-vs-index metadata; `s` reads index-vs-HEAD metadata, including unborn repositories. Status tables separate PATH/KIND/XY; diff tables separate PATH/CHANGE/MODES. `/` filters metadata only with a visible escaped query tail and caret; Delete clears the filter, Up/Down selects, and `i`/Enter inspects full paths (including rename originals), root, comparison and branch headers. Compact layouts open a separate detail pane; wide layouts show the table and details together. PgUp/PgDn scrolls ordinary metadata details with overlapping steps at compact sizes; Home returns to the start, and Esc returns to the list. Refresh preserves selected raw path where present. Empty comparisons, no display matches and unavailable reads are distinct; unavailable entry counts are unknown, not zero. Missing Git requires installing it separately and restarting Dort; path/read errors retain the root draft. No staging, commit, push or other Git mutation exists.

**Patch disclosure:** from a loaded Unstaged/Staged summary, `w` or Actions → View patch content opens a secrets warning. Type literal `view patch` and Enter once to fetch the whole comparison, **not** only the selected/displayed file. `yes` cannot authorize disclosure. Arrow keys/PgUp/PgDn scroll the bounded escaped content, including horizontal scrolling. Esc closes; root/comparison/destination changes, shrinking below 48×14 or exit also discard the UI reference, without promising secure memory erasure. There is no prefetch, automatic patch refresh, clipboard/export or retained patch history. Each new view needs the warning/phrase again; `r` fetches metadata only. Status/summaries, ordinary diagnostics and progress never contain patch content. Terminal capture/scrollback is outside Dort's control. Read-only patch viewing remains available in `--dry-run`, but not below 48×14. See [Git safety](safety.md#git-inspection-boundary).

**Profiles:** `t` opens a checkbox/profile/membership table distinguishing mandatory base/default/platform/local contributors from optional session additions. Up/Down selects; Space/Enter or a checkbox click toggles optional names. A separate Apply additions button validates the new union before changing scope. PgDn opens full manifest, selected profile, contributor, requested-addition and effective-union details from their first row; PgUp/PgDn scrolls and Home returns to controls. Esc leaves scope unchanged. Mandatory contributors cannot be unchecked. Profile additions affect complete inventory/plans/repository checks and never persist; choosing a repository resets these additions.

**Checks:** `c` checks the current repository manifest. `v` opens an independent standalone path draft; enter a path, then activate Check. Its Temporary profiles button uses separate draft additions, still unioned with local/base/default/platform profiles. Standalone checks require no repository, registration or Git; relative paths resolve from the launch directory. Summary cards separate structural validity from selected file/tool counts; the pane title labels repository or standalone scope. PgUp/PgDn inspects the complete manifest path, schema, effective profile union and its contributors, dependency order and unsupported capabilities; Home returns to the start. Structural validity is not execution readiness: live files and installed tools were not inspected. Errors retain the path/additions; Discard clears them.

**Recovery:** `3` shows retained transactions across **all repositories**, independent of file selection/configuration. `/` searches metadata; Delete or Actions → Clear Files / Recovery display filter clears the display filter. The global write-blocker count is never filtered away, including while scrolling details. The table separates short transaction ID, schema (SCM), phase and integrity/write-block state; terminal history does not imply live-file safety. Compact layouts keep the summary, filter and selected row visible; wide layouts show the table and details together. A failed diagnosis is unavailable, not a healthy empty history. Up/Down selects; `i`/Enter shows full repository/transaction IDs, phase, schema, checkpoint count and integrity details without hash transcription. Use PgUp/PgDn for long details and Home to return to the start; Esc returns to the list. Page action buttons show eligible previews only. `b` previews schema-1/2/4 rollback; `s`/`x` previews schema-3 resume/abandon only when the diagnosis offers that action. Preparing offers Abandon, not Resume. Inapplicable operations remain discoverable in Actions with an explanation, also retained in full details. Hints are not permission: the core preview independently checks current paths/artifacts, and completed history is not proof rollback is safe. Schema-1 rollback withdraws the newly adopted source into private transaction storage; schema-2 rollback leaves repository sources unchanged. Backups/private artifacts are retained, not garbage-collected. Preparing bootstraps cannot resume: explicitly abandon, then start a new Create/attach transaction. Terminal or uncertain bootstrap histories cannot resume. Abandon retains published repositories, registration and private artifacts; it does not undo publication. Review the real preview and type literal `yes` only when its effects are intended. No automatic recovery or repair occurs.

**Diagnostics/About:** in Checks (`4`), `j` reads host/architecture, HOME/config/state paths, registered count/active ID, session root, Git availability and global write blockers. Registry, history and blocker summary cards stay visible while PgUp/PgDn inspects full host/path and capability sections; Home returns to the start. Configuration and history availability are reported independently as unknown when unavailable, never zero; no local variable values or file contents appear. Diagnostics never installs tools or repairs configuration. `r` refreshes this report; `m` returns to the retained manifest result. Actions → About shows Dort's build version, safeguards and unsupported capabilities; Esc returns to the prior destination.

**Activity/results:** Reading and Approved execution panels show the task's fixed scope, even if you change destination while it runs. Progress reports only received transaction boundaries/sequences, with no estimated percentage or Cancel control. q/Ctrl-C waits for completion. Result receipts retain the operation and full transaction ID through refresh, or explicitly say no changes were needed and no transaction was created. Use PgUp/PgDn for details and Home for the start. Click Refresh (`r`), Recovery (`h`) or Back (Esc); direct destination/action keys also return to the workbench. There is no timed dismissal. Completed results remain inspectable in Overview and Files details after Back.

**Errors/retry:** The error panel separates the cause from retained task scope, with clickable controls. Esc/Back returns to retained input. `r` retries read/preview intent against current state, never replays a consumed approval or disclosed patch. After a failed write, `r` only refreshes observations; `h` opens all-repository Recovery. Possible partial effects stays visible while scrolling, and an earlier-write warning remains for the session after Back or later reads. The warning is not proof of live drift, nor does dismissal repair anything. Retain artifacts and inspect Recovery. Choose an action again for a new preview and a new confirmation; do not assume a failed call made no changes.

Actions includes a session preview-only switch; it cannot change mode during a preview/worker. An entry `--dry-run` locks the entire session to preview-only and cannot be switched off.

Esc does **not** interrupt an approved write. q/Ctrl-C requests exit after it finishes, including on a small screen. Input/render errors restore the terminal then wait for the worker; no write is detached or automatically rolled back. A failed write may leave durable partial effects: dismiss the error, refresh Recovery, and explicitly preview the appropriate recovery action. Panics restore the terminal where cleanup is possible; forced termination cannot guarantee this. See [terminal/interruption safety](safety.md#terminal-and-interruption).

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

This inspects **all** local repository transaction histories. Omit `--repo`; it is rejected for this command. Reports include transaction/repository IDs, phase, last validated `journal_schema` (1 adoption, 2 apply/unmanage, 3 bootstrap, 4 selected file batch, or null), valid-checkpoint count and integrity problems. Both completed and incomplete transactions are retained; this is journal history/integrity diagnosis, not a check for current live-file drift.

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

`recovery rollback` is an alias route for top-level `rollback`, handling schemas 1/2/4. Its preview checks affected guards and conservatively lists inverse paths without mutation. Approved execution independently revalidates the journal head/layout under the lock, then performs restartable guarded inverses. Completed transactions may be rolled back only while guards still match; later transactions/edits can invalidate them. An already-rolled-back transaction has no inverse paths.

For schema 3, remove `--dry-run` and confirm (or add `--yes`) to resume or abandon. Resume recognizes only exact prepared/published identities/content and exact before/after registration; edits/substitutions are refused. Private incomplete preparation can only be abandoned and retried. **Abandon retains published repositories, registration and private artifacts**, merely marking terminal; it never deletes or silently undoes them. If registration never published, a retained repository can be attached later via init.

There is no automatic startup replay/rollback. Malformed chains, uncertain layouts, edited sources/destinations/parents or damaged backups require investigation, not a force flag. Do not discard damaged checkpoint tails. Recovery flags approve only a newly observed preview, not a saved JSON document.

## Output and exit codes

The table below describes explicit CLI commands. A clean TUI exit returns 0 even if it displayed service/preview errors or write blockers; those stay in the interface. Fatal terminal failures return 1 (I/O), and a worker panic returns 2 after cleanup. If a worker fails while an exit request is waiting, the failure is instead emitted on stderr after restoration with its normal error code (1/2/3), not silently discarded. Do not use the TUI exit code as an automation success report.

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
