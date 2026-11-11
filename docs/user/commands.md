# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

Checks, discovery, file inventory, plans, status, history and Git inspection are read-only and never run providers. Separate Tools inspection requires approval: provider configuration/plugins may execute code and caches/logs may change. Explicit mutation commands are experimental and require approval. Only the explicit mise/npm/Homebrew/APT workflows below install packages. APT is root-native: scripts/hooks, dependency upgrades, services and system/config/data effects are possible, with required noninteractive sudo supervision and no rollback. npm/Homebrew native links may expose commands on PATH; Homebrew build/dependency code can execute and dependencies may upgrade. No command configures runtime activation, intentionally upgrades other top-level tools, clones/syncs, stages, commits or pushes. Use `dort --help` and `dort <command> --help` for syntax.

## Global options

- `--json`: structured reports on stdout; structured errors on stderr.
- `--repo PATH`: override the selected environment directory for repository commands. It does not register the directory.
- `--dry-run`: preview writes or provider inspection without execution; never creates state/directories/locks/backups/journals or prompts. Read-only commands remain read-only regardless.
- `--yes`: explicit CLI write or provider-inspection approval without prompting. Required for JSON/nonterminal writes or provider queries; does not bypass guards. Not accepted by the TUI.
- `--confirm-remove ID`: optional extra ID guard for `files remove --mode remove`; when supplied, it must match the selected mapping.
- `--help`, `--version`: available without a configured HOME.

Other commands require a supported host and HOME. Terminal invocation without a subcommand opens the TUI; nonterminal invocation without a subcommand fails promptly with exit 2. Do not rely on the working directory being auto-discovered as a repository; use the selected repository or supply `--repo`.

## Interactive interface

The interface provides Overview/Welcome, guided local create/attach, session repository/profile selection, Files discovery/adoption/apply/unmanage, structural Checks, all-repository Recovery, explicit Tools provider inspection and read-only Git inspection. The workbench uses navigation menus, a persistent current-subview label, focused pane borders, state columns, contextual action buttons, and bordered fields with a caret. Color supplements explicit selection markers and state labels. F10 opens menus; `;` opens contextual Actions; F1 or `?` outside text opens Help, and Session → About shows the build version and capability limits.

```sh
dort                       # terminal only; same as dort tui
dort --repo /absolute/path/to/dotfiles tui
dort --dry-run tui         # inspection/previews only; confirmation disabled
```

All three standard streams must be terminals. TUI entry rejects `--json`, `--yes` and `--confirm-remove`; no flags silently approve TUI writes. Use explicit CLI subcommands for automation. Startup observes only: it does not create missing configuration/state, acquire a lock or replay recovery. Without a repository, Welcome offers Choose repository and Create/attach; Recovery still diagnoses all retained local transactions. Discovery and standalone manifest checks work without a repository. Git also accepts an independent session-only working-tree root without a manifest or registration.

**Terminal appearance:** the canvas and essential neutral labels use terminal defaults. Nonempty `NO_COLOR` requests monochrome (including `NO_COLOR=0`); selection keeps reverse/bold and states remain textual. It does not disable Unicode. The first nonempty `LC_ALL`, `LC_CTYPE`, then `LANG` determines character support: UTF-8/UTF8 locales use Unicode borders, menu chevrons and content gutters, others (including missing locale) use ASCII. `TERM=vt100`, `vt102`, `ansi` or `dumb` also selects ASCII; vt100/vt102/dumb/xterm-mono selects monochrome. For example, `LC_ALL=C NO_COLOR=1 dort tui` requests both. No terminal background is guessed or setting saved. Cursor addressing is still required: setting `TERM=dumb` is not a line-mode backend. These fallbacks do not change keys, mouse targets or approval requirements.

**Reading reviews:** operation previews emphasize section headings, paths, ordered effects and blockers. Pane titles identify their content. Paging controls say **Scroll**; Help explains which visible content they scroll. File operations pair a summary table with numbered change details; `i` opens the full report. Selected-file content has source line numbers and wrapped continuation gutters, with configuration keys emphasized. Disclosed Git patches emphasize file headers and hunks; `+` marks additions and `-` marks removals, alongside color where supported. Content disclosure and confirmation requirements remain in their dedicated screens.

| Key | Action |
| --- | --- |
| F10 | Open navigation menus; arrows move, Enter chooses, Esc closes. The shortcut appears on the bottom row. |
| Left / Right | Change destination directly, outside text entry, previews and dialogs. Private patch arrows scroll content instead. |
| Tab / Shift-Tab | Switch Discover file-list/content focus outside text/dialogs; move among local fields/buttons inside forms. |
| `;`, `/` | `;` opens Actions with search focused: type a label immediately. Up/Down chooses results; `/` refocuses search only from results. In Files/Git/Recovery content, `/` searches displayed metadata. Esc leaves search, retaining its filter; Delete outside text clears it. |
| F1 | Contextual Help for the current view, pane, Actions, form, batch, preview, confirmation or task/error. Esc returns to the retained screen and draft. `?` remains literal in text/search. About/version is in the Session menu. |
| `p`, `n` | Choose repository (session only), create/attach local repository. |
| Up / Down | Select a metadata row, or scroll focused Discover content, without changing destination. PgUp/PgDn scrolls visible details/content; in a compact list it pages through that list. |
| PgUp / PgDn; Home | Scroll view/form details; Home returns to the top of the current list or detail/report/help/error viewport outside text entry. In previews, Home resets both axes. |
| Enter (Files) | Inspect the selected file or directory. Wide layouts show list/details together; compact layouts use Esc to return to the list. |
| `r` | Refresh observations/current check/Diagnostics, independent all-repository history in Recovery, or Git metadata (never a patch). In an error, retry the failed read/fresh preview; after a failed write, refresh only. |
| `g`, `z`, `u`, `s` (Git content) | Choose exact Git-only root; Status; Unstaged; Staged. |
| `a` (Tools content) | Add a desired tool declaration to the selected registered repository; no package installation. |
| `w` (Git content) | View patch content: warning then a View patch action for one whole-comparison read. |
| Enter (Git metadata) | Inspect selected metadata details; Esc returns to list. |
| `f`, `o` | Discover file metadata; open retained manual adoption form. Press `e` or click Adopt selected for an eligible regular file; ineligible entries remain inspectable but cannot open a candidate draft. |
| Space, `h` / `l` (Files) | Toggle marks; fold / unfold selected directory. All folders start folded. A file's `h` folds its parent. Click chevrons/checkboxes for the same controls; double-click a row to toggle its mark. |
| `j` / `k`, Ctrl-F / Ctrl-B, Ctrl-D / Ctrl-U, `gg` / `G` (Files list) | Next/previous row, full page, half page, first/last row. Text fields and dialogs retain their own keys. |
| `I` / `U` (Discover) | Ignore selected file/folder / list Ignored paths for confirmed Unignore. Ignore selected has a page button; Files → Ignored paths is in the menu. |
| `u` (Managed Files) | Review Keep for the selected mapping, retaining an ordinary local file and its repository source. `j`/`k` still navigates rows. |
| `E` (Discover); `A` / `K` / `D` (Managed Files) | Edit marked adoption drafts; preview selected apply / keep / delete links. |
| `t` | Temporary repository profile additions; cannot remove mandatory contributors. |
| `c`, `v` | Check current repository manifest; open standalone manifest check form. |
| `j`, `m` (Checks) | Read host/registry Diagnostics; return to the manifest result. |
| Enter (Recovery content) | Full transaction IDs, schema/phase, integrity, checkpoint count and contextual action hints. Esc returns to the list. |
| `h` (error) | Open and refresh independent all-repository Recovery; no repair or replay. |
| `a` | Preview apply (Overview / Files). |
| Actions → Keep local file / Delete managed link | Preview the selected file's unmanage operation. Both retain its repository source. |
| `b` | Preview guarded schema-1/2/4 rollback of the selected transaction. |
| `s` / `x` (Recovery) | Preview schema-3 bootstrap resume / abandon. Abandon retains published data/artifacts. |
| Up / Down (file review); `i` | Select a mapping in the summary; toggle the complete report. Selection never reduces scope. PgUp/PgDn scroll selected effects. |
| Left / Right (full preview report) | Scroll long report lines horizontally. |
| Enter (preview) | **Adopt all** approves adoption once, without typed yes. Other writes open confirmation. Blocked/dry-run/below-48×14 previews cannot approve. |
| Esc | Confirmation → preview → retained form → invoking destination; errors/help return to their prior scope. No execution is started. |
| Ctrl-U (text field) | Clear current field. Backspace edits; Tab/Shift-Tab moves between fields/buttons. Enter advances adoption/check/path fields, then activates the focused Review/Check/Select button. |
| `q` / Ctrl-C | Quit outside text entry / anywhere; an active worker finishes first. Printable `q`, `?`, `/` are text in forms/search. |

The compact Menu control exposes every destination, and Welcome leads with create/choose/discover/check guidance. Preview scrolling is bounded to the escaped report and current viewport; Home returns to its origin. File reviews show typed change/unchanged/blocked counts, a mapping table and selected effects. `i` opens the full report for complete permissions, warnings and guards. Review the full effects before Enter. Adoption uses **Adopt all** (Enter/click), without typed yes; other writes use Confirm (Enter/click). Deletion confirmation displays the exact selected IDs and binds them internally; typing yes alone cannot delete a link. Execution independently revalidates under the same application lock as the CLI. Read-only inspection remains available while another process holds that lock. During work, destination navigation, Help, scrolling, redraw/resize and exit requests stay responsive; scope changes and new jobs are disabled. Only one service worker runs at a time. Forms/pickers keep the focused control visible at compact sizes; long field values show an escaped tail with full input available through PgDn. Confirmation PgUp/PgDn scrolls the prompt independently of its input, keeping long exact IDs inspectable. Shrinking below 48×14 clears confirmation text and the deletion acknowledgement; growing requires both again.

**Metadata navigation:** Files, Recovery and Git show list/details together at 110 or more columns with enough height for both panes and the active controls. Up/Down selects rows unless Discover content is focused; Enter inspects, PgUp/PgDn scrolls visible details and Home returns to their start. In compact layouts, Enter opens details and Esc returns to the list; paging in a list moves through that list. Resize retains visible inspection, never an invisible focused pane. In Discover, Tab/Shift-Tab switches file-list/content focus; clicking either pane also focuses it, with an accented border. Focused content accepts j/k (or arrows), gg/G, Ctrl-F/B pages and Ctrl-D/U half-pages using wrapped screen rows, without moving file selection. Esc returns to the list. Switching focus does not reread content. Tab remains local to text/forms/dialogs and does not switch panes in other destinations. Inspection never requests a preview or patch; private patch input/scrolling remains separate.

**Mouse:** click destination tabs, metadata rows, filters and labeled action buttons. A row click selects only; Inspect opens compact details and Adopt selected remains a separate action. Wheel over a list selects rows; over details, previews or a dialog it scrolls that area. Click form fields to edit or buttons to request their action. In repository selection, click a row then Enter; profile checkboxes toggle on click and Apply remains separate. Mouse can activate **Adopt all** against a real unblocked adoption preview. Labeled Confirm, Run query and View patch controls can be activated by click or Enter after review; no typing challenge is required. Busy work cannot start a second action. Mouse capture is restored on normal exit, errors and unwinding where terminal cleanup is possible.

**Menus:** F10 opens navigation/session menus. At 80 columns or wider, click a top-level dropdown; compact layouts provide a visible Menu control and group list. Left/Right switches groups, Up/Down chooses an item, Enter opens it, and Esc returns to the current view. Each item also shows a working menu-local shortcut; for example, `j` opens Diagnostics from the Checks menu. In compact submenus, Left returns to groups. Files offers Managed, Discover and Ignored paths; Checks offers Manifest checks and Diagnostics; Git offers Status, Unstaged and Staged plus Git-only scope selection. Session contains repository selection, create/attach, preview-only mode, Help, About and Quit. The current destination/subview stays visible. F1 explains menus and restores the exact menu selection on return. Unavailable entries retain reasons; F1 includes the full selected reason. Menus cannot interrupt forms, previews, confirmations, task/error/result screens or Git disclosure input. Existing direct shortcuts remain available under their existing input guards.

**Actions:** one searchable panel contains only operations for the selected item, marked items and current view. Navigation and session commands live in menus, not Actions or the context action bar. Commands belonging to other views stay in those views: switch to Recovery for rollback, Checks for standalone validation, Git for patch viewing, or Discover for ignores. Search matches labels case-insensitively. Up/Down chooses a result and Enter requests it; filtering/highlighting never changes scope or starts work. Scope and effect are independent labels: editable drafts, mutation previews and content warnings are never approvals. An `x` marks an unavailable action; Enter leaves Actions open with its prerequisite reason and no task. The complete selected label, query, reason and limits appear inline below the list. PgUp/PgDn scrolls details and Home returns to their top; search and results stay visible. Use Up/Down for results and `/` to edit search; Tab does not change workbench focus. Esc first leaves search, then closes Actions and restores the invoking destination. Printable `q`, `?`, `/` stay literal while search is focused; Ctrl-U clears it. Use page buttons or direct keys for ordinary work without opening Actions. When primary controls do not fit, the visible Actions control reports how many more are available; F1 Help remains visible.

**Contextual Help:** F1 works even while editing text or reviewing/confirming an operation. `?` opens the same Help only outside text entry. Help is an exclusive screen: Enter, typed `yes` and clicks never submit or authorize the underlying screen. Esc restores retained input, selection and focus; PgUp/PgDn scrolls Help independently and Home returns to its start. Active work continues, and exit still waits for the worker. Shrinking below 48×14 clears the full approval/acknowledgement sequence even with Help open. Discover content is withheld while Help is open and can be reread on return. A still-live Git patch stays hidden behind Help; shrink/exit still drops it, with no automatic redisclosure.

**Overview:** summary cards show registration, active file count and global history blockers. Short layouts use a compact summary strip. PgUp/PgDn inspects the full root, selection origin, profile union, linked/needs-apply/blocked counts, observation errors and retained operation result; Home returns to the start. Unknown counts mean unavailable observations, not a healthy zero. A zero history-blocker count does not establish live-file safety or authorize a write.

**Choose repository:** the table separates exact-path entry, the registered active root and known roots; a separate Create/attach button opens its retained draft. Up/Down selects a row/button; Enter activates it. The registered-active choice resolves configuration, not the current explicit root. Full selected/current paths, registry availability and selection details appear below the controls. PgUp/PgDn scrolls those details; Home returns to their top while the controls remain visible. Selection is session-only: no registration/configuration write or upward repository search occurs. The compact header identifies the scope; Overview shows the full root, selection origin, profiles and registration or observation errors. An unregistered root can be inspected but requires approved attachment before writing. History always includes all repositories.

**Create/attach:** enter a destination; Enter advances to Review read-only preview, and Enter on that button requests the preview. Draft/resolved-path details appear below the fields. PgUp/PgDn scrolls details and Home returns to their top, retaining the focused field and draft. Relative input resolves from the directory where Dort was launched; no tilde/environment expansion occurs. An absent destination creates a minimal repository; an existing valid Git root with `dort.toml` is attached. The existing parent must be safe; arbitrary existing directories are refused. Git is resolved at startup; if unavailable, install it separately and restart Dort. Esc/error retains the input in memory; nothing is saved on exit. Only explicit preview approval persists registration/selection. On success the session switches to that root and Overview shows the transaction ID, or returns to an invoking adoption draft. Other operations retain their transaction ID or explicit no-op result through refresh; Esc clears the result.

Folders start folded; opening/closing a folder updates its subtree immediately, without a list refresh. Large Discover inventories prepare new filters and hierarchies in the background. **Updating list** temporarily replaces the old rows; keep typing to refine the search, open Help or leave the page. Row selection/adoption resumes when the latest list is ready. Marks are retained, including marks hidden by the new display. Quit waits for the owned worker; it does not cancel an approved write.

**Discover/adopt:** `f` opens bounded metadata candidates, with kind/link/eligibility hints, warnings and the heuristic notice. A selected repository protects its containers; an invalid selected root is an error, not unscoped discovery. The table separates name, metadata eligibility and kind. Directories, links, special files and regular files failing ownership/link-count/special-mode checks are not eligible; press Enter for details. Enter inspects any row without changing the retained draft; `h`/`l` folds/unfolds directories. `e` or clicking Adopt selected directly fills editable TARGET/ID/SOURCE suggestions only for an eligible regular file; an ineligible row keeps its explanation and retained draft. Alternatively find Adopt selected in Actions; `o` opens the retained manual draft. Tab/Shift-Tab selects fields or buttons, and Enter on Review requests a fresh read-only preview. SOURCE is repository-relative. Single and batch suggestions preserve the exact HOME-relative path under `files/`: `~/.config/nvim/init.lua` becomes `files/.config/nvim/init.lua`, and `~/.zshrc` becomes `files/.zshrc`. Suggested IDs are sanitized from that relative path (for example, `config-nvim-init-lua`). All fields stay editable; existing mappings and manually chosen source paths are not relocated. The new link mapping belongs to `base`; requested profiles do not change its membership. Choose/Create/attach buttons preserve all adoption input and return after selection/attachment. Esc/errors retain drafts in memory; Discard clears the draft. Core path/kind/sensitivity/collision refusals remain authoritative. Only the selected eligible regular file is automatically read for a private content preview: up to 64 KiB, with unsafe characters escaped. Sensitive-name/content heuristics, binary/non-UTF-8 data, unsafe parents and oversized files withhold content. Heuristics cannot prove secrecy. Wide Discover puts content above details; compact Enter shows them, Esc returns to the list. PgUp/PgDn scrolls content when it is shown; compact list paging stays in the list. Alt+PgUp/PgDn scrolls details; the mouse wheel scrolls its hovered pane. No other candidate is prefetched. Selection/scope/view changes, shrink below 48×14 and exit discard the UI reference, not securely erase memory or terminal capture. Preview-only sessions do not disclose candidate content. Contents never enter reports, logs, errors, receipts or history. Discover page buttons omit redundant Inspect and manual entry; `o`/Actions still supports manual drafts.

**Ignore and undo:** In Discover, select a file or folder and click **Ignore selected** or press `I` (also available in `;` Actions). **Ignore selected** stays visible at 48×14; the visible **Menu** control reaches Files → **Ignored paths**. Review the exact path, press Enter, then activate Confirm. Future CLI and TUI discovery skips that HOME-relative path and all descendants, across repositories, including newly created descendants. It does not change the ignored files, repository mappings or adoption eligibility. Choose Files → **Ignored paths** from the menu, or press `U` in Discover to list preferences; Up/Down or j/k selects, PgUp/PgDn pages, Home/End jumps, Enter previews Unignore, and Esc returns. Unignore also requires a Confirm action. Ancestor rules and built-in safety exclusions still apply. Ignore/Unignore is disabled in preview-only mode and below the approval floor. Success refreshes Discover and clears candidate marks; cancellation writes nothing.

**Tree and batch actions:** Files derives its hierarchy from existing metadata, without extra filesystem traversal. ASCII chevrons and checkboxes work without special fonts; color supplements eligibility/state labels. `h`/`l` folds/unfolds, and Space toggles a file mark. On a group, Space toggles all matching descendants, including folded rows; discovery marks only eligible owned, single-link ordinary files, not directories or links. Ineligible marking shows a prominent refusal; group marking reports omitted ineligible descendants. Filters affect group-marking candidates, never existing marks. The header reports marked and hidden counts. Actions offers Clear marks and Expand/Collapse all. Marks are session-local; managed marks clear on repository change. Refresh does not silently discard missing marked IDs: clear/reselect them if the next preview refuses them.

In Discover, `E` opens retained TARGET/ID/SOURCE drafts for every marked file. The editor fully covers Discover; no underlying list/content or page actions remain visible or active. All TARGET/ID/SOURCE defaults appear in one scrollable table. Esc returns to Discover with marks and edits retained. **Review all** is focused initially, so accepting defaults needs no per-file navigation. j/k or Up/Down selects an item; `e` / **Edit item** opens its separate TARGET/ID/SOURCE form. Enter advances fields to **Done**; Done or Esc returns to the batch list with edits retained, never reviewing/adopting only that item. Tab/Shift-Tab or field clicks are optional. The inline cell/Tab editing path also remains available. Complete selected-item paths stay inline below the batch table and controls. PgUp/PgDn scrolls those paths; Home returns to their top. Activate Review all to request one real combined preview, then **Adopt all** (Enter/click) approves once, without typed yes. Success returns to refreshed Discover with a brief status and receipt details retained. Suggestions can collide; edit them rather than silently dropping files. In Managed Files, uppercase `A`, `K`, `D` preview Apply selected, Keep selected, Delete selected links. Lowercase `a` remains full active-scope Apply. Hidden/folded marks remain included; row selection in the review never narrows the approved set. A successful full inventory refresh drops marks for removed mappings, including after Keep/Delete; valid filtered marks stay selected. An unavailable inventory retains marks until a successful refresh.

A batch contains 1–32 explicitly chosen files, one action and one repository. Every item and cross-item conflict is validated before live changes; one blocker refuses the whole batch. Review all effects; adoption uses one **Adopt all** activation, while other batch writes use one Confirm action for one schema-4 transaction. Delete confirmation displays **every exact mapping ID** and binds them internally; mouse clicks cannot submit those deletion acknowledgements or approval. Shrinking below 48×14 clears the entire acknowledgement sequence. Cancellation/dry-run creates no transaction. Failures during execution may leave partial effects: retain history/backups and use explicit guarded whole-transaction rollback, never an automatic undo or per-file retry queue. Adoption rollback withdraws its new sources into private storage; selected apply/keep/delete and their rollback leave repository sources unchanged. These batch mutation controls are TUI-only; existing CLI commands are unchanged. Recovery remains individually guarded, not bulk.

**Managed inventory:** Files → Managed shows the active mappings with source/destination kinds, drift and blockers in details. Invalid observation is reported as unavailable, never as a healthy empty inventory. `/` filters display only; `a` always previews the complete active-profile scope, even with no displayed matches. Esc leaves search without clearing the filter; Delete or Actions → Clear current display filter removes it. Refresh preserves selected identity where present. Actions → Keep local file: enter exact mapping ID / Delete managed link: enter exact mapping ID accepts a valid ID even outside the current filter. Enter advances the ID field to Review; only the subsequent preview can proceed to approval. Both modes retain repository sources; deletion confirmation displays and binds the exact selected mapping.

**Git:** choose Status, Unstaged or Staged from the Git menu to open metadata inspection. Its root defaults to the current file repository; `g` accepts an independent exact non-bare working-tree root. Relative paths resolve from the launch directory without tilde/environment expansion. This requires no manifest/registration and never changes file scope or configuration. Git → Follow selected repository restores the default. `z` reads Status (branch headers, kind/XY, names and rename originals); `u` reads tracked worktree-vs-index metadata; `s` reads index-vs-HEAD metadata, including unborn repositories. Status tables separate PATH/KIND/XY; diff tables separate PATH/CHANGE/MODES. `/` filters metadata only with a visible escaped query tail and caret; Delete clears the filter, Up/Down selects, and Enter inspects full paths (including rename originals), root, comparison and branch headers. Compact layouts open a separate detail pane; wide layouts show the table and details together. PgUp/PgDn scrolls ordinary metadata details with overlapping steps at compact sizes; Home returns to the start, and Esc returns to the list. Refresh preserves selected raw path where present. Empty comparisons, no display matches and unavailable reads are distinct; unavailable entry counts are unknown, not zero. Missing Git requires installing it separately and restarting Dort; path/read errors retain the root draft. No staging, commit, push or other Git mutation exists.

**Patch disclosure:** from a loaded Unstaged/Staged summary, `w` or Actions → View patch content opens a secrets warning. Activate View patch once to fetch the whole comparison, **not** only the selected/displayed file. Arrow keys/PgUp/PgDn scroll the bounded escaped content, including horizontal scrolling. Esc closes; root/comparison/destination changes, shrinking below 48×14 or exit also discard the UI reference, without promising secure memory erasure. There is no prefetch, automatic patch refresh, clipboard/export or retained patch history. Each new view needs the warning/phrase again; `r` fetches metadata only. Status/summaries, ordinary diagnostics and progress never contain patch content. Terminal capture/scrollback is outside Dort's control. Read-only patch viewing remains available in `--dry-run`, but not below 48×14. See [Git safety](safety.md#git-inspection-boundary).

**Profiles:** `t` opens a checkbox/profile/membership table distinguishing mandatory base/default/platform/local contributors from optional session additions. Up/Down selects; Space/Enter or a checkbox click toggles optional names. A separate Apply additions button validates the new union before changing scope. Inline details show the full manifest, selected profile, contributors, requested additions and effective union; PgUp/PgDn scrolls them and Home returns to their top while the picker controls remain visible. Esc leaves scope unchanged. Mandatory contributors cannot be unchecked. Profile additions affect complete inventory/plans/repository checks and never persist; choosing a repository resets these additions.

**Checks:** `c` checks the current repository manifest. `v` opens an independent standalone path draft; enter a path, then activate Check. Its Temporary profiles button uses separate draft additions, still unioned with local/base/default/platform profiles. Standalone checks require no repository, registration or Git; relative paths resolve from the launch directory. Summary cards separate structural validity from selected file/tool counts; the pane title labels repository or standalone scope. PgUp/PgDn inspects the complete manifest path, schema, effective profile union and its contributors, dependency order and unsupported capabilities; Home returns to the start. Structural validity is not execution readiness: live files and installed tools were not inspected. Errors retain the path/additions; Discard clears them.

**Recovery:** Recovery → All repositories shows retained transactions across **all repositories**, independent of file selection/configuration. `/` searches metadata; Delete or Actions → Clear current display filter clears the display filter. The global write-blocker count is never filtered away, including while scrolling details. The table separates short transaction ID, schema (SCM), phase and integrity/write-block state; terminal history does not imply live-file safety. Compact layouts keep the summary, filter and selected row visible; wide layouts show the table and details together. A failed diagnosis is unavailable, not a healthy empty history. Up/Down selects; Enter shows full repository/transaction IDs, phase, schema, checkpoint count and integrity details without hash transcription. Use PgUp/PgDn for long details and Home to return to the start; Esc returns to the list. Page action buttons show eligible previews only. `b` previews schema-1/2/4 rollback; `s`/`x` previews schema-3 resume/abandon only when the diagnosis offers that action. Preparing offers Abandon, not Resume. Inapplicable operations remain discoverable in Actions with an explanation, also retained in full details. Hints are not permission: the core preview independently checks current paths/artifacts, and completed history is not proof rollback is safe. Schema-1 rollback withdraws the newly adopted source into private transaction storage; schema-2 rollback leaves repository sources unchanged. Backups/private artifacts are retained, not garbage-collected. Preparing bootstraps cannot resume: explicitly abandon, then start a new Create/attach transaction. Terminal or uncertain bootstrap histories cannot resume. Abandon retains published repositories, registration and private artifacts; it does not undo publication. Review the real preview and activate Confirm only when its effects are intended. No automatic recovery or repair occurs.

**Diagnostics/About:** in Checks, `j` reads host/architecture, HOME/config/state paths, registered count/active ID, session root, Git availability and global write blockers. Registry, history and blocker summary cards stay visible while PgUp/PgDn inspects full host/path and capability sections; Home returns to the start. Configuration and history availability are reported independently as unknown when unavailable, never zero; no local variable values or file contents appear. Diagnostics never installs tools or repairs configuration. `r` refreshes this report; `m` returns to the retained manifest result. Session → About shows Dort's build version, safeguards and unsupported capabilities; Esc returns to the prior destination.

**Activity/results:** Reading and Approved execution panels show the task's fixed scope, even if you change destination while it runs. Progress reports only received transaction boundaries/sequences, with no estimated percentage or Cancel control. q/Ctrl-C waits for completion. Result receipts retain the operation and full transaction ID through refresh, or explicitly say no changes were needed and no transaction was created. Use PgUp/PgDn to scroll the activity or receipt; Home returns to its top. Click Refresh (`r`), Recovery (`h`) or Back (Esc); direct destination/action keys also return to the workbench. There is no timed dismissal. Completed results remain inspectable in Overview and Files details after Back.

**Errors/retry:** The error panel separates the cause from retained task scope, with clickable controls. Esc/Back returns to retained input. `r` retries read/preview intent against current state, never replays a consumed approval or disclosed patch. After a failed write, `r` only refreshes observations; `h` opens all-repository Recovery. Possible partial effects stays visible while scrolling, and an earlier-write warning remains for the session after Back or later reads. The warning is not proof of live drift, nor does dismissal repair anything. Retain artifacts and inspect Recovery. Choose an action again for a new preview and a new confirmation; do not assume a failed call made no changes.

Actions includes a session preview-only switch; it cannot change mode during a preview/worker. An entry `--dry-run` locks the entire session to preview-only and cannot be switched off.

Esc does **not** interrupt an approved write. q/Ctrl-C requests exit after it finishes, including on a small screen. Input/render errors restore the terminal then wait for the worker; no write is detached or automatically rolled back. A failed write may leave durable partial effects: dismiss the error, refresh Recovery, and explicitly preview the appropriate recovery action. Panics restore the terminal where cleanup is possible; forced termination cannot guarantee this. See [terminal/interruption safety](safety.md#terminal-and-interruption).

## Tool declarations

A declaration records what you want in `dort.toml`; it does **not** install a package or adopt an existing binary. Create or attach the repository first.

**TUI:** select the repository through Session, then Tools → **Add tool declaration** (`a`). Fill in the tool ID, bare binary name and at least one platform's provider/package. Profiles default to `base`; prerequisites refer to existing tool IDs. Leave a platform's provider and other fields blank when it is unsupported. Versions are optional; npm definitions also need a declared runtime tool ID. Tab/Shift-Tab moves between fields, Enter advances, and **Review declaration** opens the preview. Activate **Confirm** with Enter or a click to save. Esc returns to the retained draft.

**CLI:** replace `/absolute/path/to/environment` below with your registered repository root. The JSON is one Tool declaration, using the [manifest fields and provider rules](manifest.md).

```sh
dort --repo /absolute/path/to/environment tools declare --dry-run \
  --declaration '{"id":"ripgrep","binary":"rg","install":{"macos":{"provider":"brew","package":"ripgrep"},"ubuntu":{"provider":"apt","package":"ripgrep"}}}'
```

Remove `--dry-run` and confirm `y/N` to save, or use `--yes` for automation. `--json --dry-run` emits the preview; `--json --yes` emits the completion report. Repeated `--profile NAME` adds request profile scope to persistent additions; the declaration's own profiles belong in its JSON.

An identical typed declaration is a no-op with no transaction. A conflicting tool ID is refused; updates and removals are not supported. Other declarations and TOML comments are preserved. A changing declaration creates one manifest-only schema-5 transaction. Recovery → **Preview rollback** (`b`) or CLI `recovery rollback` can restore its unchanged result; later manifest edits can prevent rollback. Existing npm snapshots need fresh inspection after a manifest change.

## Provider inspection

Tools inspection runs a trusted local provider only after approval. Configuration/plugins may execute code; caches/logs may change. Offline/no-update settings are hardening, **not a sandbox or a network guarantee**. No package installation, provider rollback or ownership verification is performed. Startup, navigation, checks, plans and status never query providers automatically.

In the TUI, open **Tools → Inspect installed tools** (`c`), or enter Tools with no retained results. Dort finds available Homebrew, apt (dpkg-query), mise and npm defaults without running commands. Available inventories are selected for you. Review them, then press Enter or click **Inspect installed tools** once. Each selected inventory becomes its own session snapshot; nothing is installed or adopted. Up/Down chooses a manager and Space toggles selection. `d` shows the full executable, arguments, home and search scope; `r` refreshes suggestions without querying providers.

**Advanced** (`e`) lets you change paths for the highlighted manager; Enter on an unavailable row opens its missing details. Detected values and your overrides are retained. npm defaults need a selected repository declaring exactly one Node runtime and a matching Node/npm CLI installation layout. Its displayed installation root is the proposed global prefix: change it in Advanced if you use a different prefix. Otherwise supply only the missing manifest, runtime ID, Node, npm CLI or prefix details. Node runs the selected CLI script directly; Dort never runs an npm launcher or config query to guess a binding.

In Advanced, Tab/Shift-Tab moves fields and Enter advances; Space on Provider cycles managers. Activate **Review exact query**, inspect the exact executable/arguments/home/runtime scope and warning (PgUp/PgDn scroll), then activate Run query to execute once. Esc returns to retained input. Preview-only and layouts below 48×14 cannot execute; shrinking dismisses the query review. Quit/Ctrl-C during a query waits for the bounded worker. Nothing is automatically undone.

Results are provider-reported installed package/version metadata, including inactive mise versions, not proof of which binary PATH selects or what Dort owns. `/` searches the display, Enter/`d` shows full selected metadata and query scope, and `r` prepares a **fresh review**, never replays approval. Unavailable/malformed/incomplete observations are not healthy empty catalogs. No automatic refresh occurs.

CLI equivalents (replace these examples with paths you trust for your installation):

```sh
# Review only; no provider process, even with --yes.
dort tools inspect brew --executable /opt/homebrew/bin/brew \
  --search-dir /opt/homebrew/bin --search-dir /usr/bin --search-dir /bin --dry-run

# Ubuntu package database inspection; requires explicit approval in automation.
dort tools inspect apt --executable /usr/bin/dpkg-query \
  --search-dir /usr/bin --search-dir /bin --json --yes

# Independent mise installation catalog, not a repository/profile activation.
dort tools inspect mise --executable /trusted/bin/mise \
  --search-dir /trusted/bin --search-dir /usr/bin --search-dir /bin --dry-run

# npm requires all binding fields; runtime ID node must be declared in this manifest.
dort tools inspect npm --executable /trusted/node/bin/node \
  --search-dir /trusted/node/bin --search-dir /usr/bin --search-dir /bin \
  --manifest /absolute/path/dort.toml --runtime-tool node \
  --npm-cli /trusted/node/lib/node_modules/npm/bin/npm-cli.js \
  --prefix /trusted/npm-global --dry-run
```

Remove `--dry-run` to execute after terminal confirmation (`inspect`), or use explicit `--yes` for JSON/nonterminal execution. `--repo` and `--confirm-remove` do not apply; npm-only fields are rejected for other providers. Reports include the exact query scope; stdout/stderr payloads, configuration, resolved URLs, scripts and parser excerpts are withheld. Exit 0 means a completed observation (possibly empty), 1 means an unavailable provider observation, and 2 means invalid input/approval/manifest. The runtime binding records your selection, not proof that the declared runtime owns that Node or prefix.

## Snapshot reconciliation

Tools can retain **multiple independently approved snapshots** for the session, then reconcile declarations against your explicit choices. Snapshots are provider claims, not ownership, trust or proof that a binary is ready. They are not saved across sessions. Reconciliation alone performs no package operation. Separate opt-in mise/npm/Homebrew/APT install reviews are available for eligible missing rows; provider configuration/code risks still apply.

### Interactive workflow

1. In Tools, review detected defaults and activate Inspect installed tools once for the selected inventories, or use Advanced and Run query for a manual scope. Each result gets a session ID (`s1`, `s2`, …). Repeated queries create new snapshots, not replacements or merged inventories.
2. Press `s` (or **Tools → Session snapshots**) to browse snapshots. Up/Down and Enter select one; `d` on its report shows full scope. `x` **in the snapshot browser** forgets only that metadata, not a package or earlier provider effects. Session limits are 16 snapshots and 65,536 installation records; a limit refusal never replays a query.
3. Press `p` (or **Tools → Reconcile snapshots**) to enter an absolute manifest, **separate tool-search directories** (colon-separated) and optional temporary profiles (comma-separated). These tool directories are not the provider's child PATH and are never discovered from the process environment. Load declarations with Enter. This step does not query providers.
4. Each selected tool initially says **Not observed**. Space explicitly chooses a matching-provider snapshot ID; `d` inspects that chosen scope. Nothing is assigned automatically, even when only one snapshot exists. Tab moves controls. Activate **Reconcile metadata** after reviewing the choices.
5. Up/Down selects a result; Enter/`d` shows full package, scope, blocker and binary-probe metadata. PgUp/PgDn and Home/End inspect the full detail. Esc returns to retained inputs. Reloading/recalculating uses the chosen snapshots as recorded and never queries a provider automatically.

Use existing physical tool-search directories. Symlinked parents/directories (for example `/bin` on some systems), missing/unreadable directories and raced observations are **unavailable**, not healthy empty. Candidate symlink targets are shown but never followed. Lookup reads only metadata for declared bare names: candidates include nonexecutable files, directories, special files and dangling links, and are always unattributed. No candidate is run to obtain a version. Absence means only that the explicit scope was completely inspected and no matching name was found.

A package **no-op is not tool readiness**; binaries may still be missing/unverified and dependencies may be blocked. A missing-package **review proposal is not execution approval**. Default/latest policies do not upgrade existing matching claims. Other providers, unbound tools, version conflicts and ambiguous tap/architecture/backend names remain explicit rather than guessed or silently merged. npm snapshots must come from the same unchanged manifest; changed/different declarations require a fresh, separately approved inspection, never a silent runtime-ID reassociation.

### Install-command descriptions — preview only

For an eligible **Review only** missing-package row, full TUI details and CLI `reconciliation.plan.rows[].install_preview` show proposed executable/argv, cleared-environment controls, version selection and side-effect warnings. **Dort cannot run these descriptions.** The separate mise/npm/Homebrew/APT workflows below reconstruct their own live previews; they never execute these descriptions. Regular reconciliation with `--yes` approves only inspection queries, not installation. No native install or install `--dry-run` operation is invoked to construct this metadata.

Satisfied, ambiguous and blocked tools have no install command. APT command descriptions stay withheld here: a dpkg-query snapshot never authorizes inventing apt-get/sudo. The separate [APT install review](#bounded-apt-installation) requires explicit trusted programs and root-risk approval. Version defaults, prefixes and latest requests are unresolved native selections, not a reproducibility or availability guarantee. A bare mise default can select multiple globally configured versions; the description does not request activation and executes nothing. Homebrew controls do not guarantee that dependencies remain unchanged or unupgraded. npm descriptions use the selected Node/CLI/prefix and disable lifecycle scripts, which can leave packages unusable; provider code/configuration risks still apply. All native effects can include downloads, shared links/caches and partial failures without rollback.

These descriptions require a computed eligible row; reconciliation `--dry-run` still previews only queries/analysis scope and does not invent missing-package facts. Descriptions exceeding metadata limits are explicitly deferred rather than partially emitted. They are not execution approvals, tested provider capabilities, verified tool readiness or a supported manual-install handoff.

### CLI collection and reconciliation

`tools reconcile` combines **explicitly approved collection** with metadata-only analysis. Supply one strict JSON `--scope` per requested inspection and explicit `--bind TOOL=SNAPSHOT` choices. Scope fields are `id`, `provider`, `executable`, `search_dirs`, with `runtime_tool`, `npm_cli` and `prefix` only for npm. There are no arbitrary command/argument/environment fields. Each descriptor constructs a fresh real core preview; JSON is not execution authorization.

```sh
# Replace paths with your trusted executables and existing physical directories.
# This only previews the two queries and analysis scope: no provider/binary lookup.
dort tools reconcile --manifest /absolute/path/dort.toml \
  --tool-search-dir /trusted/node/bin --tool-search-dir /usr/bin \
  --scope '{"id":"mise","provider":"mise","executable":"/trusted/bin/mise","search_dirs":["/trusted/bin","/usr/bin"]}' \
  --scope '{"id":"npm","provider":"npm","executable":"/trusted/node/bin/node","search_dirs":["/trusted/node/bin","/usr/bin"],"runtime_tool":"node","npm_cli":"/trusted/node/lib/node_modules/npm/bin/npm-cli.js","prefix":"/trusted/npm-global"}' \
  --bind node=mise --bind pi=npm --dry-run
```

The manifest must declare the bound tools and npm runtime ID. Add `--profile NAME` for temporary profile additions. Other selected tools stay not observed; they can therefore block the package plan. Duplicate/unknown bindings or snapshot IDs are rejected before query execution.

Remove `--dry-run` to review and confirm `y/N` **separately for each identified scope**, or use explicit `--yes --json` for automation. `--yes` does not bypass validation. If you decline a later scope or analysis fails, earlier approved inspections may already have executed configuration/plugins or changed caches/logs; nothing is automatically undone. Metadata reconciliation itself never executes/refetches a provider, writes state or grants installation authority.

Final JSON includes `snapshots` and `reconciliation` (scope, bindings, binary observations and package plan). Exit 1 means an unavailable requested provider/binary observation, 4 means a blocked package plan, 2 means invalid input/approval/changed declarations, and 0 means a completed package analysis—not verified tool readiness. Reports and snapshots are not reusable execution approvals or persisted installation receipts.

## Exact mise installation

Dort can install **one observed missing exact mise version** for a registered repository. The manifest must specify an exact version such as `1.2.3`, with clear prerequisites and no existing/unattributed binary in the separate lookup scope. Default/latest/prefix mise policies, conflicting or unknown inventories, unmanaged binaries and blocked prerequisites are not installable. Homebrew and Ubuntu APT have separate provider-current workflows below. npm has a separate [exact-package workflow](#exact-npm-installation). No activation, force/reinstall, upgrade, uninstall or rollback is provided.

**TUI:**

1. Select the registered repository and profiles through Session. Use Tools → Inspect installed tools to collect a mise inventory.
2. Press `p`, load that repository's `dort.toml`, existing physical tool-search directories and matching profile additions, then explicitly bind the target and any satisfied prerequisites to the chosen mise snapshot. Reconcile.
3. On an eligible missing exact mise row, press `i` or click **Review exact mise install**. This review runs no queries. `d` shows full scope/arguments; return with `d` or Esc.
4. Press Enter or click **Install exact version** once. Dort freshly inspects the scope under the writer lock, installs only if still eligible, and inspects afterward. A newly satisfied exact package becomes a no-op. Results distinguish process outcome, package claim and binary metadata; inactive mise versions are normal, not readiness verification.

The installer has a **10-minute deadline**. Provider/backend/package code and downloads can change data and caches; timeout or failure may leave partial effects. Quit/Ctrl-C waits for the worker and terminal restoration. Installer output is discarded, not shown or logged. Historical snapshots are not refreshed: collect a fresh inventory and explicitly bind it before another install review. Recomputing with old snapshots remains read-only; **Collect fresh inventory** (`c` in that comparison) starts a new default review without running commands.

**CLI:** add `--install-tool TOOL` to explicit collection/reconciliation. A registered manifest is required; persistent profiles are included with your explicit additions. Each collection query is approved separately, followed by one installation review/confirmation. `--yes` explicitly approves the selected workflow in automation; JSON/nonterminal execution requires it. Without `--install-tool`, reconciliation remains metadata-only.

```sh
# Preview requested scope only: no queries, installs, state, locks or prompts.
# Replace paths and IDs with your registered manifest and trusted physical paths.
dort --dry-run tools reconcile --manifest /home/alex/dotfiles/dort.toml \
  --tool-search-dir /home/alex/bin \
  --scope '{"id":"mise-global","provider":"mise","executable":"/usr/local/bin/mise","search_dirs":["/usr/bin"]}' \
  --bind demo=mise-global --install-tool demo
```

Omit `--dry-run` for interactive collection and review. A dry-run cannot invent an installed inventory or prove eligibility; it previews the request, not an executable install approval. Successful exact package observation (or a newly satisfied no-op) exits 0; failed/unverified outcomes exit 1, invalid/stale/refused requests exit 2, and lock contention exits 3. A successful exit does not establish binary readiness. Error exits can still follow partial effects or failed outcome persistence; inspect installation attempts before retrying.

## Exact npm installation

Dort can install **one observed missing exact npm registry package** with lifecycle scripts disabled. The registered manifest must declare its `runtime_tool`, an exact package version, and clear dependencies. The selected Node must be the sole runtime candidate in your separate tool-search directories, with a satisfied package claim in an explicitly selected runtime inventory. Existing/conflicting/unmanaged/unknown packages and default/latest/prefix requests cannot be installed.

Use a **physical, owned prefix strictly inside HOME**, separate from the selected runtime. Existing required parents, including a scoped package's `@scope` directory, cannot be symlinks or group/world writable; absent descendants are created only after approval and durable intent. npm's write roots are `PREFIX/lib/node_modules`, `PREFIX/bin` and `PREFIX/share/man`: selected Node and npm CLI must be outside them. A typical co-located Node/npm default prefix may therefore be unsuitable for installation. Change the prefix deliberately in Advanced, collect that new scope, and bind it explicitly; Dort never silently substitutes another prefix or runtime.

**Important:** native dependencies and bin/man links can expose commands immediately if PREFIX/bin is already on PATH. Dort does not edit PATH or activate/configure Node. Script-disabled packages may not be usable. Effects are not atomic or isolated from other prefix entries; failure/timeout can leave partial changes. Trusted npm/builtin configuration and dependencies may read HOME or contact additional endpoints. No private-auth setup, force, intentional other-top-level upgrade, uninstall or rollback is provided.

**TUI:**

1. Select the registered repository and profiles. Collect a runtime inventory and any prerequisite inventories through **Tools → Inspect installed tools**. These are provider claims, not runtime ownership or readiness proof.
2. In the npm inventory's **Advanced** form, select the declared runtime ID, physical Node executable, npm CLI script and explicit global prefix. Node runs that CLI directly; no npm launcher, version probe or config query is used. Confirm collection once. Keep the manifest unchanged; changed declarations require fresh collection.
3. Press `p`, load `dort.toml`, existing physical tool-search directories and matching profiles. Explicitly bind the target to its npm snapshot and runtime/prerequisites to their own chosen snapshots, then reconcile.
4. Select the eligible target and press `i` or click **Review exact npm install**. Review is process-free; `d` shows the full prefix, trusted programs, fresh query scopes, fixed install arguments and risks.
5. Enter/click **Install exact version** once. Under the writer lock, Dort revalidates every selected necessary catalog and the runtime candidate, installs only if still missing, and observes afterward. A newly satisfied exact target is a no-op. Quit/Ctrl-C waits; the installer deadline is ten minutes. Results do not verify readiness. Before another install, collect fresh inventories and explicitly rebind all required scopes; old snapshots are retained but cannot authorize another install review.

**CLI:** use the same `--install-tool` opt-in, confirmation and exit codes as mise. This example assumes declared `node` and `prettier` tools, a satisfied runtime, and an exact npm version for `prettier`; replace every path with your physical trusted paths:

```sh
dort --dry-run tools reconcile --manifest /home/alex/dotfiles/dort.toml \
  --tool-search-dir /home/alex/runtimes/node/bin \
  --tool-search-dir /home/alex/bin \
  --scope '{"id":"runtime","provider":"mise","executable":"/home/alex/bin/mise","search_dirs":["/usr/bin"]}' \
  --scope '{"id":"npm","provider":"npm","executable":"/home/alex/runtimes/node/bin/node","search_dirs":["/usr/bin"],"runtime_tool":"node","npm_cli":"/home/alex/runtimes/node/lib/node_modules/npm/bin/npm-cli.js","prefix":"/home/alex/.local/npm-global"}' \
  --bind node=runtime --bind prettier=npm --install-tool prettier
```

Omit `--dry-run` for interactive collection and review, or explicitly add `--yes` for automation. JSON/nonterminal execution never prompts. The dry-run does not query, create parents/state/locks or establish eligibility. npm uses fixed public-registry, ignore-scripts, audit/fund/notifier/log controls and ignored user/global config; unsupported controls fail without a weaker fallback. Installer output is discarded.

## Homebrew core installation

On macOS, Dort can install **one observed missing canonical Homebrew core formula** with an omitted or `latest` version policy. Use its unqualified canonical name, for example `jq`, not a tap-qualified name or alias. Versioned formula names are identities, not exact patch pins. Casks, aliases/renames, disabled/unknown formulae, HEAD, arbitrary options/URLs/files and privileged layouts are unsupported. Already matching installed claims are no-ops, never upgrade requests.

**Native effects:** formula/build/resource/dependency code **can execute**. Dependencies can install or upgrade, and native links can expose commands on PATH. Skip-post-install skips post-install steps only, not all build code. This is not bottle-only, exact-pinned, atomic, a sandbox, a usability guarantee or isolation of other prefix entries. There is no rollback, PATH/runtime configuration, service operation, manager installation, explicit link/upgrade/cleanup or sudo fallback. Trusted Homebrew may contact endpoints and maintain its native caches/metadata.

Select a physical trusted `REPOSITORY/bin/brew` and the expected native prefix. Supported layouts put the repository at `PREFIX` or `PREFIX/Homebrew`, and Cellar at `PREFIX/Cellar`. Required native write roots must already exist as safe owned directories; a non-writable root-owned `/usr/local` parent is acceptable only when its required write roots are already owned/writable by you. Unsafe, missing or mismatched layouts refuse; do not bypass them with sudo or permission changes. Dort does not create Homebrew layouts for you.

**TUI:** collect the Homebrew inventory and any declared prerequisite inventories, load the registered manifest/profile scope and separate physical tool-search directories, and explicitly bind every necessary tool. On an eligible row, `i` / **Review core formula install** opens a process-free review. `a` edits the retained **Advanced prefix**; `d` shows full scope, fixed probes and arguments. Enter/click **Install core formula** once approves fresh help/layout/canonical-formula probes, necessary installed catalogs, conditional installation and post-observation. Unsupported advertised controls refuse; there is no weaker fallback. The ten-minute installer deadline can leave partial effects; q/Ctrl-C waits. Results report provider-current presence and observed versions, never exact presence or verified readiness. Before another install collect fresh catalogs and explicitly rebind them.

**CLI:** this assumes a registered macOS manifest declaring `jq` with `provider='brew', package='jq'`, without blocked prerequisites. Replace the paths with your physical trusted paths. `--brew-prefix` deliberately overrides the metadata-only layout hint; it never changes an observed mismatch.

```sh
dort --dry-run tools reconcile --manifest /Users/alex/dotfiles/dort.toml \
  --tool-search-dir /Users/alex/bin \
  --scope '{"id":"brew","provider":"brew","executable":"/opt/homebrew/bin/brew","search_dirs":["/usr/bin","/bin"]}' \
  --bind jq=brew --install-tool jq --brew-prefix /opt/homebrew
```

Dry-run runs no queries, creates no state and cannot invent absence. Remove `--dry-run` for separate inventory confirmation followed by the single install review, or use explicit `--yes` for automation. A query-only reconciliation never installs. Native installer arguments are fixed: `brew install --formula --no-ask --skip-post-install homebrew/core/FORMULA`. Fixed no-update/no-analytics/no-target-upgrade/no-dependent-check/no-cleanup controls are required; help and layout probes run only as part of this explicit installation approval, not normal inspection collection.

## Bounded APT installation

On Ubuntu, one observed missing literal package with omitted or `latest` policy can be installed at the provider-current version, **not an exact Debian pin**. The repository must be registered; explicitly bind the target and all necessary prerequisites to approved inventories. Complete dpkg inventory is required: any repair-required package blocks installation. Names with periods/regex characters, architecture/version/release qualifiers, paths/options or trailing `+`/`-` refuse. There are no apt update, upgrade, remove, repository-edit or provider-setup workflows.

**Root risk:** root hooks, maintainer scripts, triggers and dependency code can execute. Dependencies can install/upgrade, services can start/restart, and packages, configuration, live links, databases, caches and data outside HOME can change. `--no-upgrade` protects only the named target; `--no-remove` forbids removals, not every other change. Native root configuration/endpoints and sudo policy/PAM remain trusted. Clearing the caller environment is not privileged isolation. There is no sandbox, exact-pin, atomicity, package-isolation, usability/readiness or rollback guarantee.

In Tools comparison choose the eligible row, then `i` / **Review root-risk APT install**. Preparation runs no commands. `a` opens retained **Advanced** fields for physical absolute apt-cache, apt-get and sudo programs; co-located metadata hints are inspectable, never execution or authentication probes. Missing paths require explicit input. Tab/Enter moves fields; printable shortcuts remain text. `d` shows the full scope, fixed argv and root-risk warning. One Enter/click **Install with root risk** confirms fresh necessary catalogs, bounded `apt-cache policy PACKAGE`, conditional privileged installation and post-dpkg observation. This confirmation is separate from inventory approval. Preview-only/below-floor layouts cannot execute; leaving/shrinking drops approval.

The fixed privileged request uses selected sudo with `-n -T 600 -u root`, `DEBIAN_FRONTEND=noninteractive`, selected absolute apt-get, `--assume-yes --no-remove --no-upgrade --no-install-recommends`, disabled dpkg PTY and default/old conffile handling. Sudo refuses prompts; user-specified command timeouts require explicit permission in existing sudo policy. If environment/timeout permission is denied or unsupported, the attempt fails noninteractively. Dort never collects passwords, validates credentials, manipulates the credential cache, changes policy, configures a helper or retries without the timeout. Ask your administrator about policy outside Dort; no policy-changing command is supplied here.

Only trusted sudo owns privileged timeout/termination. At the requested ten-minute deadline, an overdue supervisor receives catchable TERM for sudo's relay and Dort **continues waiting joined**, with an overdue status. A user cannot reliably kill root descendants; no strict OS deadline against a broken supervisor or proof every privileged descendant/service terminated is promised. q/Ctrl-C waits, with no detached work or automatic rollback. Failure/deadline may leave partial or ongoing native effects; inspect the reported outcome and system separately, never assume safe automatic retry. Historical closure proves neither root termination nor repair.

Example process-free CLI preview (replace trusted physical paths as appropriate):

```sh
dort --dry-run tools reconcile --manifest /physical/dotfiles/dort.toml \
  --tool-search-dir /usr/bin --install-tool rg \
  --apt-cache /usr/bin/apt-cache --apt-get /usr/bin/apt-get --sudo /usr/bin/sudo \
  --scope '{"id":"packages","provider":"apt","executable":"/usr/bin/dpkg-query","search_dirs":["/usr/bin"]}' \
  --bind rg=packages
```

Remove `--dry-run` for separate inventory confirmation followed by the root-risk install review; JSON/nonterminal execution needs explicit `--yes`. A newly matching installed target is a no-op, never an upgrade request. Results distinguish process outcome, provider-current package/version claims and binary metadata, not readiness. Inventories remain historical; collect fresh ones and explicitly rebind before another install. No supported step requires a CLI handoff from the TUI.

## Installation attempts and explicit closure

Tools → **Installation attempts** (`i` outside comparison) lists private, all-repository attempt metadata. Enter/`d` shows full IDs, scope and observed outcomes. An unresolved attempt blocks later Tools installs, not unrelated file workflows. Nothing is replayed, cleaned up or rolled back at startup.

Select an unresolved row and press `c` or click **Review closure**. The preview uses the original trusted provider scope; missing/damaged scope opens retained provider/executable/child-PATH fields, plus Node/npm CLI/prefix for npm or the original prefix for Homebrew. Use the Provider control to cycle mise, npm, Homebrew and APT; each provider's draft is retained. Known attempts retain their original scope, without needing an unchanged current manifest. Review is process-free. One Enter/click approves a **fresh inventory query for that provider scope and closure of unchanged metadata only**. Homebrew also verifies a fixed fresh prefix query against the displayed original prefix. An unavailable query cannot close the attempt. APT closure queries only complete dpkg inventory, without sudo/root authentication or current manifest requirements. Closure is not proof of root termination, package repair, retry, readiness verification or a successful install. Changed records require a fresh review.

```sh
dort tools attempts
dort tools attempts --id ATTEMPT_ID
dort --dry-run tools close-attempt ATTEMPT_ID
# Interactive fresh query and metadata closure:
dort tools close-attempt ATTEMPT_ID
# For missing/damaged original scope, also supply trusted paths:
dort tools close-attempt ATTEMPT_ID --executable /usr/local/bin/mise --search-dir /usr/bin
# Missing/damaged npm scope (no current manifest or runtime claim required):
dort tools close-attempt ATTEMPT_ID --provider npm \
  --executable /home/alex/runtimes/node/bin/node --search-dir /usr/bin \
  --npm-cli /home/alex/runtimes/node/lib/node_modules/npm/bin/npm-cli.js \
  --prefix /home/alex/.local/npm-global
# Missing/damaged APT scope (dpkg only; no sudo/authentication):
dort tools close-attempt ATTEMPT_ID --provider apt \
  --executable /usr/bin/dpkg-query --search-dir /usr/bin

# Missing/damaged Homebrew scope:
dort tools close-attempt ATTEMPT_ID --provider brew \
  --executable /usr/local/Homebrew/bin/brew --search-dir /usr/bin \
  --prefix /usr/local
```

No literal confirmation phrase or ID must be retyped at confirmation. If attempt storage is unsafe or exhausted, Dort refuses rather than deleting records or silently treating unavailable metadata as empty.

## Initialization and approval

```sh
dort init /absolute/path/to/dotfiles --dry-run
dort init /absolute/path/to/dotfiles --yes
```

Init requires its explicit positional path (omit `--repo`). Relative paths resolve from the current directory; use physical absolute paths to avoid symlink aliases. It either creates a new minimal local Git repository and manifest at an **absent** destination, or registers/selects an already-valid non-bare Git root containing a host-valid `dort.toml`. Arbitrary existing/empty directories are refused. The destination's parent must already exist, be owned and not group/world writable. HOME/root and Dort metadata overlaps are forbidden.

Creation initializes only a private staged repository with templates disabled, branch `main`, SHA-1 objects and minimal `schema = 1` manifest. It journals intent before no-replace publication and then registration. No cloning/network/hooks/staging/commit occurs. State/configuration/repository must share a filesystem supporting atomic renames. Interrupted bootstrap blocks new writes; use explicit recovery below. Approved init/attach needs an installed trusted Git, with the same environment/configuration hardening as inspection. Attachment refuses filters/promisor configuration.

Human terminal mode (stdin/stdout/stderr all terminals, no `--json`) presents a preview then asks whether to proceed (`y/N`). Removal uses the exact mapping ID already selected by the request; no extra typing challenge is required. `--yes` bypasses prompts; an optional `--confirm-remove` guard is still checked when supplied. JSON/nonterminal writes always require explicit flags and never prompt. Dry runs skip all approvals and mutations. Saved preview JSON is not execution authorization.

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

`discover` needs no repository unless one is selected explicitly or locally. It examines hidden HOME entries and traverses every nonexcluded hidden directory to depth 8 (HOME is depth 0), with a 100,000-name total budget and a 100,000-name per-directory limit. Nonhidden HOME directories are not traversed. Protected paths and known sensitive/generated locations are excluded. It does not follow directory symlinks or read candidate contents.

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
# dort files remove editor --mode remove --yes
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
- `--patch`: the **only CLI content-disclosure option**, accepted only by `git diff`. Adds a separate bounded display patch. All non-ASCII/control bytes and backslashes are escaped, except LF line breaks. This is not a directly applicable patch file. Binary changes produce markers, not binary payloads. It can expose secrets; there is no secret-proof filtering.

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

`recovery rollback` is an alias route for top-level `rollback`, handling schemas 1/2/4/5. Its preview checks affected guards and conservatively lists inverse paths without mutation. Approved execution independently revalidates the journal head/layout under the lock, then performs restartable guarded inverses. Completed transactions may be rolled back only while guards still match; later transactions/edits can invalidate them. An already-rolled-back transaction has no inverse paths.

For schema 3, remove `--dry-run` and confirm (or add `--yes`) to resume or abandon. Resume recognizes only exact prepared/published identities/content and exact before/after registration; edits/substitutions are refused. Private incomplete preparation can only be abandoned and retried. **Abandon retains published repositories, registration and private artifacts**, merely marking terminal; it never deletes or silently undoes them. If registration never published, a retained repository can be attached later via init.

There is no automatic startup replay/rollback. Malformed chains, uncertain layouts, edited sources/destinations/parents or damaged backups require investigation, not a force flag. Do not discard damaged checkpoint tails. Recovery flags approve only a newly observed preview, not a saved JSON document.

## Output and exit codes

The table below describes explicit CLI commands. A clean TUI exit returns 0 even if it displayed service/preview errors or write blockers; those stay in the interface. Fatal terminal failures return 1 (I/O), and a worker panic returns 2 after cleanup. If a worker fails while an exit request is waiting, the failure is instead emitted on stderr after restoration with its normal error code (1/2/3), not silently discarded. Do not use the TUI exit code as an automation success report.

| Code | Meaning |
| --- | --- |
| 0 | Read succeeded or explicitly approved operation completed; not a production-safety guarantee. |
| 1 | Unavailable provider/binary observation or I/O failure. |
| 2 | Invalid input/configuration/approval, stale or unsafe guards, malformed history, unsupported host/arguments, unavailable or failed/refused/bounded Git operation. |
| 3 | Writer lock contention; dry runs/read-only commands never acquire it. |
| 4 | A file/package plan is blocked, or recovery diagnosis reports write blockers. |

Blocked plans/diagnosis still emit their full report on **stdout**. Other runtime errors go to **stderr**, as JSON when `--json` is selected. Clap help/argument-parsing output has its own normal formatting; `--json` does not turn invalid CLI syntax into a JSON report.

Successful mutation JSON has `schema: 1`, `operation`, `completed: true`, and nullable `transaction_id`; init additionally reports `repository`. Human mutation mode presents its preview before confirmation and then the same structured completion report. JSON mode emits a single preview (dry-run/blocked) or completion report, never prompts. Runtime errors use `error`/`kind` on stderr; an error can follow partial durable effects, so inspect history. Exit codes alone do not mean no effects occurred.

Ordinary reports expose paths/metadata/hashes but not file bodies or local variable values. Explicit `git diff --patch` additionally discloses bounded escaped content, which may contain secrets. Even metadata reports can reveal personal paths or configuration names; review before sharing.
