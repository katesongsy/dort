# Command reference

[User guide](README.md) · [Getting started](getting-started.md)

Manifest checks, discovery, file inventory, plans, status, history and Git inspection are read-only and never run providers. Separate Tools inspection requires approval: provider configuration/plugins may execute code and caches/logs may change. Explicit mutation commands are experimental and require approval. Only the explicit npm/Homebrew workflows below install packages. npm/Homebrew native links may expose commands on PATH; Homebrew build/dependency code can execute and dependencies may upgrade. No command configures runtime activation, intentionally upgrades other top-level tools, clones/syncs, stages, commits or pushes. Use `dort --help` and `dort <command> --help` for syntax.

## Global options

- `--json`: structured reports on stdout; structured errors on stderr.
- `--repo PATH`: override the selected environment directory for repository commands. It does not register the directory.
- `--dry-run`: preview writes or provider inspection without execution; never creates state/directories/locks/backups/journals or prompts. Read-only commands remain read-only regardless.
- `--yes`: explicit CLI write or provider-inspection approval without prompting. Required for JSON/nonterminal writes or provider queries; does not bypass guards. Not accepted by the TUI.
- `--confirm-remove ID`: optional extra ID guard for `files remove --mode remove`; when supplied, it must match the selected mapping.
- `--help`, `--version`: available without a configured HOME.

Other commands require a supported host and HOME. Terminal invocation without a subcommand opens the TUI; nonterminal invocation without a subcommand fails promptly with exit 2. Do not rely on the working directory being auto-discovered as a repository; use the selected repository or supply `--repo`.

## Interactive interface

The interface provides Overview/Welcome, guided local create/attach, session repository/profile selection, Files discovery/adoption/apply/unmanage, structural Checks, all-repository Recovery, explicit Tools provider inspection and read-only Git inspection. The workbench uses one location header, a wide-layout destination rail, section/destination Menu at every width and one foreground action strip. Neutral inverse selection is independent of fact colors and marked membership. Text fields have a caret; choice controls do not. Menu or Ctrl-G opens sections and their destinations; Ctrl-K opens the global searchable task launcher; F2 opens contextual More when additional actions do not fit; F3 opens full metadata where offered. F1 or `?` outside text opens Help, and Session → About shows the build version and capability limits.

```sh
dort                       # terminal only; same as dort tui
dort --repo /absolute/path/to/dotfiles tui
dort --dry-run tui         # inspection/previews only; confirmation disabled
```

All three standard streams must be terminals. TUI entry rejects `--json`, `--yes` and `--confirm-remove`; no flags silently approve TUI writes. Use explicit CLI subcommands for automation. Startup observes only: it does not create missing configuration/state, acquire a lock or replay recovery. Without a repository, Welcome offers Choose repository and Create/attach; Recovery still diagnoses all retained local transactions. Discovery and standalone manifest checks work without a repository. Git also accepts an independent session-only working-tree root without a manifest or registration.

**Pages and dialogs:** forms, reviews, results, history and ignored-path management are pages with a breadcrumb and Back. The Menu mouse control remains available while typing; Ctrl-K opens searchable tasks and destinations. Opening/dismissing a menu keeps drafts, focus and reviews; choosing an available destination discards pending approval, even for the same tab. Short repository/profile choices, Actions, Help/About and sensitive-content disclosure gates are dimmed framed dialogs. Dialogs and running work visibly pause navigation; outside clicks do not cancel, approve or disclose. Use Esc or explicit Close/Cancel to leave a dialog. q/Ctrl-C during work waits for completion.

**Terminal appearance:** the canvas and essential neutral labels use terminal defaults. Nonempty `NO_COLOR` requests monochrome (including `NO_COLOR=0`); selection keeps reverse/bold and states remain textual. It does not disable Unicode. The first nonempty `LC_ALL`, `LC_CTYPE`, then `LANG` determines character support: UTF-8/UTF8 locales use Unicode borders, menu chevrons and content gutters, others (including missing locale) use ASCII. `TERM=vt100`, `vt102`, `ansi` or `dumb` also selects ASCII; vt100/vt102/dumb/xterm-mono selects monochrome. For example, `LC_ALL=C NO_COLOR=1 dort tui` requests both. No terminal background is guessed or setting saved. Cursor addressing is still required: setting `TERM=dumb` is not a line-mode backend. These fallbacks do not change keys, mouse targets or approval requirements.

**Motion:** active task headings show a bounded liveness indicator, not measured child-process progress or an estimated installation percentage. Set `DORT_REDUCED_MOTION=1` before starting Dort for a static indicator, for example `DORT_REDUCED_MOTION=1 dort tui`. This is independent of `NO_COLOR` and locale/ASCII support; no preference is persisted. Quit during owned work still waits for completion.

**Forms:** forms start outside field editing. Press Tab to enter the first field. Esc stops editing without losing the draft; Tab enters it again. Esc outside editing goes back one page. Enter invokes the page's primary review/load action. Menus preserve the draft and focus.

**Reading reviews:** operation previews emphasize section headings, paths, ordered effects and blockers. Pane titles identify their content. Paging controls say **Scroll**; Help explains which visible content they scroll. File operations pair a summary table with numbered change details; F3 opens the full report. Selected-file content has source line numbers and wrapped continuation gutters, with configuration keys emphasized. Disclosed Git patches emphasize file headers and hunks; `+` marks additions and `-` marks removals, alongside color where supported. Content disclosure and confirmation requirements remain in their dedicated screens.

| Key | Action |
| --- | --- |
| `o` / `f` / `t` | Open Overview / Files / Tools directly, outside text entry. |
| `r` / `c` / `g` | Open Recovery / Checks / Git directly, outside text entry. |
| 1 / 2 / 3 / 4 | Open a numbered local destination in the current section, outside fields/filters. In Menu, use the selected section. |
| Ctrl-G / Menu | Open navigation: Up/Down selects a section, Left/Right a destination, Enter opens it. Esc or Ctrl-G closes without navigating. |
| Left / Right | In Menu, select a local destination without opening it; elsewhere operate the focused choice or scroll long lines where offered. |
| Tab / Shift-Tab | Move among form controls only; action-strip buttons are never traversal stops. Use Enter to inspect a list row and Esc to return. |
| Ctrl-K, `;`, `/` | Ctrl-K opens the global Actions launcher with search focused: type a label immediately. Up/Down chooses results; `/` refocuses search only from results. Esc closes Actions directly; F3 opens full action metadata. In Files/Git/Recovery content, `/` searches displayed metadata; Esc leaves that search retaining its filter, and Delete outside text clears it. |
| F1 | Contextual Help for the current view, pane, Actions, form, batch, preview, confirmation or task/error. Esc returns to the retained screen and draft. `?` remains literal in text/search. About/version is in the Session menu. |
| `p`, Ctrl-N | Choose repository (session only), create/attach local repository. |
| Up / Down | Select a list row or scroll the open reader, without changing destination. PgUp/PgDn pages the current list or reader. |
| PgUp / PgDn; Home | Scroll view/form details; Home returns to the top of the current list or detail/report/help/error viewport outside text entry. In previews, Home resets both axes. |
| Enter (Files) | Inspect the selected file or directory on a reading page at any width; Esc returns to the list. |
| `R` | Refresh observations/current check/Diagnostics, independent all-repository history in Recovery, or Git metadata (never a patch). In an error, retry the failed read/fresh preview; after a failed write, refresh only. |
| `P`, `z`, `u`, `s` (Git content) | Choose exact Git-only root; Status; Unstaged; Staged. |
| Enter (Tools lists) | Open the selected tool, package or installation record; never Check or Install. |
| `C` / `i` (declared tools/details) | Review Check installed / Install missing. |
| `v` / `h` (Tools) | Installed tools / Installation history. |
| `a` (Tools content) | Add a declaration; an installed package prefills known fields. |
| `s` (declared tool details) | Necessary session settings for this tool. |
| `e` / `d` (declared tools/details) | Edit / remove the selected declaration; installed packages stay untouched. |
| `w` (Git content) | View patch content: warning then a View patch action for one whole-comparison read. |
| Enter (Git metadata) | Inspect selected metadata details; Esc returns to list. |
| `F`, `O` | Discover file metadata; open retained manual adoption form. Press `e` or click Adopt selected for an eligible regular file; ineligible entries remain inspectable but cannot open a candidate draft. |
| Space, `h` / `l` (Files) | Toggle marks; fold / unfold selected directory. All folders start folded. A file's `h` folds its parent. Click chevrons/checkboxes for the same controls; double-click a row to toggle its mark. |
| `j` / `k`, Ctrl-F / Ctrl-B, Ctrl-D / Ctrl-U, Home / `G` (Files list) | Next/previous row, full page, half page, first/last row. Text fields and dialogs retain their own keys. |
| `I` / `U` (Discover) | Ignore selected file/folder / list Ignored paths for confirmed Unignore. Ignore selected is in More; Files → Ignored paths is in Menu. |
| `u` (Managed Files) | Review Keep for the selected mapping, retaining an ordinary local file and its repository source. `j`/`k` still navigates rows. |
| `E` (Discover); `A` / `K` / `D` (Managed Files) | Edit marked adoption drafts; preview selected apply / keep / delete links. |
| `T` | Temporary repository profile additions; cannot remove mandatory contributors. |
| `C`, `v` (Checks) | Check current repository manifest; open standalone manifest check form. |
| `j`, `m` (Checks) | Read host/registry Diagnostics; return to the manifest result. |
| Enter (Recovery content) | Full transaction IDs, schema/phase, integrity, checkpoint count and contextual action hints. Esc returns to the list. |
| `h` (error) | Open and refresh independent all-repository Recovery; no repair or replay. |
| `a` | Preview apply (Overview / Files). |
| Actions → Keep local file / Delete managed link | Preview the selected file's unmanage operation. Both retain its repository source. |
| `b` | Preview guarded schema-1/2/4 rollback of the selected transaction. |
| `s` / `x` (Recovery) | Preview schema-3 bootstrap resume / abandon. Abandon retains published data/artifacts. |
| Up / Down (file review); `i` | Select a mapping in the summary; toggle the complete report. Selection never reduces scope. PgUp/PgDn scroll selected effects. |
| Left / Right (full preview report) | Scroll long report lines horizontally. |
| Enter (preview) | Approves the reviewed changes once; no additional dialog. Adoption uses **Adopt all**. Blocked/dry-run/below-48×14 previews cannot approve. |
| Esc | Confirmation → preview → retained form → invoking destination; errors/help return to their prior scope. No execution is started. |
| Ctrl-U (text field) | Clear current field. Backspace edits; Tab/Shift-Tab moves between fields only. Enter requests the primary Review/Check/Select action from any field. |
| `q` / Ctrl-C | Quit outside text entry / anywhere; an active worker finishes first. Printable `q`, `?`, `/` are text in forms/search. |

Menu exposes every destination at all widths, and Welcome leads with create/choose/discover/check guidance. Preview scrolling is bounded to the escaped report and current viewport; Home returns to its origin. File reviews show typed change/unchanged/blocked counts, a mapping table and selected effects. F3 opens the full report for complete permissions, warnings and guards. Review the full effects before Enter. Adoption uses **Adopt all** (Enter/click), without typed yes; other writes use Confirm (Enter/click). Deletion confirmation displays the exact selected IDs and binds them internally; typing yes alone cannot delete a link. Execution independently revalidates under the same application lock as the CLI. Read-only inspection remains available while another process holds that lock. During work, attempted navigation explains that it is paused; Help, scrolling, redraw/resize and joined exit requests stay responsive. Scope changes and new jobs are disabled. Only one service worker runs at a time. Forms/pickers keep the focused control visible at compact sizes; long field values show an escaped tail with full input available through F3 metadata and PgDn. Confirmation PgUp/PgDn scrolls the prompt while keeping the explicit Confirm action available and long exact IDs inspectable. Shrinking below 48×14 clears pending confirmation and the deletion acknowledgement; growing requires fresh confirmation.

**Metadata navigation:** wide lists may show a passive inspector. Enter opens a full reading page for Files, Recovery or Git at every width; arrows and PgUp/PgDn then scroll only that reader, never hidden rows. Home returns to its top and Esc returns to the retained list. Discover's reader preserves the selected-content and metadata panes; Alt+PgUp/PgDn scrolls metadata and the mouse wheel targets its hovered pane. Inspection never requests a mutation preview or Git patch; private patch disclosure/input remains separate.

**Mouse:** click destination tabs, metadata rows, filters and labeled action buttons. A row click selects only; Inspect opens compact details and Adopt selected remains a separate action. Wheel over a list selects rows; over details, previews or a dialog it scrolls that area. Click form fields to edit or buttons to request their action. In repository selection, click a row then Enter; profile checkboxes toggle on click and Apply remains separate. Mouse can activate **Adopt all** against a real unblocked adoption preview. Labeled Confirm, Run query and View patch controls can be activated by click or Enter after review; no typing challenge is required. Busy work cannot start a second action. Mouse capture is restored on normal exit, errors and unwinding where terminal cleanup is possible.

**Menus:** Click Menu or press Ctrl-G to see sections and their destinations at any width, or use Ctrl-K to search destinations and tasks. Up/Down selects a section, Left/Right a destination, Enter opens it, and Esc returns without navigating. Clicking a section selects its row; clicking a destination opens it directly. Each item also shows a working menu-local shortcut; for example, `2` opens Diagnostics from the Checks menu. Compact rows scroll horizontally to keep the selected destination visible; its label and shortcut are repeated below the menu. Stable section pages show clickable numbered local destinations with the current one marked. Files uses 1 Managed, 2 Discover, 3 Ignored paths; Checks uses 1 Manifest checks, 2 Diagnostics; Git uses 1 Status, 2 Unstaged, 3 Staged; Tools uses 1 Inventory, 2 Snapshots, 3 Reconcile, 4 Attempts. Digits stay text in fields/filters and do not switch hidden destinations during focused workflows. Plain section-letter shortcuts are shown in Menu and the wide navigation rail. Both use Overview, Files, Tools, Recovery, Checks, Git in that order, with Session last in Menu. Lowercase section letters are reserved outside text entry; contextual uppercase O/F/T/R/C retains adoption/discovery/profiles/refresh/inspection actions, and P chooses Git scope. All letters remain literal in fields and filters. Local More opens the same Menu if labels do not fit; focused workflow forms/reviews/results keep their title and Back path instead. Files offers Managed, Discover and Ignored paths; Checks offers Manifest checks and Diagnostics; Git offers Status, Unstaged and Staged plus Git-only scope selection. Session contains repository selection, create/attach, preview-only mode, Help, About and Quit. The current destination/subview stays visible. F1 explains menus and restores the exact menu selection on return. Unavailable entries retain reasons; F1 includes the full selected reason. Forms, previews and error/result pages retain Menu access; dialogs, confirmation/disclosure gates and running work explain paused navigation. Contextual action shortcuts retain their input and approval guards.

**Actions:** Ctrl-K opens the global searchable task launcher, including destinations and session tasks. F2 More contains actions for the current foreground; neither menu grants approval. Menu provides destination-first navigation without searching. Search matches labels case-insensitively. Up/Down chooses a result and Enter requests it; filtering/highlighting never changes scope or starts work. Scope and effect are independent labels: editable drafts, mutation previews and content warnings are never approvals. An `x` marks an unavailable action; Enter leaves Actions open with its prerequisite reason and no task. F3 opens the complete selected label, query, reason and limits; wide dialogs also show inline details. PgUp/PgDn scrolls metadata and Home returns to its top; Esc returns to choices. Use Up/Down for results and `/` to edit search; Tab does not change workbench focus. Esc closes Actions directly from search or choices and restores the invoking destination. Printable `q`, `?`, `/` stay literal while search is focused; Ctrl-U clears it. Use page buttons or direct keys for ordinary work without opening Actions. The foreground strip keeps the current primary action, More and Help available at compact sizes; secondary actions are in More.

**Contextual Help:** F1 works even while editing text or reviewing/confirming an operation. The bottom hints describe the foreground only: Help offers Back/scrolling, forms show field/review actions, and metadata views do not advertise hidden row actions. `?` opens the same Help only outside text entry. Help is an exclusive screen: Enter, typed `yes` and clicks never submit or authorize the underlying screen. Esc restores retained input, selection and focus; PgUp/PgDn scrolls Help independently and Home returns to its start. Active work continues, and exit still waits for the worker. Shrinking below 48×14 clears the full approval/acknowledgement sequence even with Help open. Discover content is withheld while Help is open and can be reread on return. A still-live Git patch stays hidden behind Help; shrink/exit still drops it, with no automatic redisclosure.

**Overview:** summary cards show registration, active file count and global history blockers. Short layouts use a compact summary strip. PgUp/PgDn inspects the full root, selection origin, profile union, linked/needs-apply/blocked counts, observation errors and retained operation result; Home returns to the start. Unknown counts mean unavailable observations, not a healthy zero. A zero history-blocker count does not establish live-file safety or authorize a write.

**Choose repository:** the table separates exact-path entry, the registered active root and known roots; a separate Create/attach button opens its retained draft. Up/Down or Tab selects rows only; Enter selects the row. Ctrl-N or Create/attach opens its retained page. The registered-active choice resolves configuration, not the current explicit root. F3 opens full selected/current paths, registry availability and selection metadata; wide dialogs also show inline details. PgUp/PgDn scrolls metadata, Home returns to its top, and Esc returns to choices. Selection is session-only: no registration/configuration write or upward repository search occurs. The compact header identifies the scope; Overview shows the full root, selection origin, profiles and registration or observation errors. An unregistered root can be inspected but requires approved attachment before writing. History always includes all repositories.

**Create/attach:** enter a destination; Enter requests Review read-only preview directly from the path field. F3 opens full draft/resolved-path metadata; wide pages also show inline details. PgUp/PgDn scrolls metadata and Home returns to its top; Esc returns to the retained field and draft. Relative input resolves from the directory where Dort was launched; no tilde/environment expansion occurs. An absent destination creates a minimal repository; an existing valid Git root with `dort.toml` is attached. The existing parent must be safe; arbitrary existing directories are refused. Git is resolved at startup; if unavailable, install it separately and restart Dort. Esc/error retains the input in memory; nothing is saved on exit. Only explicit preview approval persists registration/selection. On success the session switches to that root and Overview shows the transaction ID, or returns to an invoking adoption draft. Other operations retain their transaction ID or explicit no-op result through refresh; Esc clears the result.

Folders start folded; opening/closing a folder updates its subtree immediately, without a list refresh. Large Discover inventories prepare new filters and hierarchies in the background. **Updating list** temporarily replaces the old rows; keep typing to refine the search, open Help or leave the page. Row selection/adoption resumes when the latest list is ready. Marks are retained, including marks hidden by the new display. Quit waits for the owned worker; it does not cancel an approved write.

**Discover/adopt:** `F` opens bounded metadata candidates, with kind/link/eligibility hints, warnings and the heuristic notice. A selected repository protects its containers; an invalid selected root is an error, not unscoped discovery. The table separates name, metadata eligibility and kind. Directories, links, special files and regular files failing ownership/link-count/special-mode checks are not eligible; press Enter for details. Enter inspects any row without changing the retained draft; `h`/`l` folds/unfolds directories. `e` or clicking Adopt selected directly fills editable TARGET/ID/SOURCE suggestions only for an eligible regular file; an ineligible row keeps its explanation and retained draft. Alternatively find Adopt selected in Actions; `O` opens the retained manual draft. Tab/Shift-Tab selects fields only, and Enter from any field requests a fresh read-only preview. Ctrl-R chooses a repository, Ctrl-N opens create/attach, Ctrl-D resets the draft, and F3 opens full metadata. SOURCE is repository-relative. Single and batch suggestions preserve the exact HOME-relative path under `files/`: `~/.config/nvim/init.lua` becomes `files/.config/nvim/init.lua`, and `~/.zshrc` becomes `files/.zshrc`. Suggested IDs are sanitized from that relative path (for example, `config-nvim-init-lua`). All fields stay editable; existing mappings and manually chosen source paths are not relocated. The new link mapping belongs to `base`; requested profiles do not change its membership. Choose/Create/attach buttons preserve all adoption input and return after selection/attachment. Esc/errors retain drafts in memory; Discard clears the draft. Core path/kind/sensitivity/collision refusals remain authoritative. Only the selected eligible regular file is automatically read for a private content preview: up to 64 KiB, with unsafe characters escaped. Sensitive-name/content heuristics, binary/non-UTF-8 data, unsafe parents and oversized files withhold content. Heuristics cannot prove secrecy. Wide Discover puts content above details; compact Enter shows them, Esc returns to the list. PgUp/PgDn scrolls content when it is shown; compact list paging stays in the list. Alt+PgUp/PgDn scrolls details; the mouse wheel scrolls its hovered pane. No other candidate is prefetched. Selection/scope/view changes, shrink below 48×14 and exit discard the UI reference, not securely erase memory or terminal capture. Preview-only sessions do not disclose candidate content. Contents never enter reports, logs, errors, receipts or history. Discover page buttons omit redundant Inspect and manual entry; `O`/Actions still supports manual drafts.

**Ignore and undo:** In Discover, select a file or folder and choose More → **Ignore selected**, or press `I` (also available in Ctrl-K Actions). More keeps **Ignore selected** reachable at 48×14; **Menu** reaches Files → **Ignored paths**. Review the exact path, press Enter, then activate Confirm. Future CLI and TUI discovery skips that HOME-relative path and all descendants, across repositories, including newly created descendants. It does not change the ignored files, repository mappings or adoption eligibility. Choose Files → **Ignored paths** from the menu, or press `U` in Discover to list preferences; Up/Down or j/k selects, PgUp/PgDn pages, Home/End jumps, Enter previews Unignore, and Esc returns. Unignore also requires a Confirm action. Ancestor rules and built-in safety exclusions still apply. Ignore/Unignore is disabled in preview-only mode and below the approval floor. Success refreshes Discover and clears candidate marks; cancellation writes nothing.

**Tree and batch actions:** Files derives its hierarchy from existing metadata, without extra filesystem traversal. ASCII chevrons and checkboxes work without special fonts; color supplements eligibility/state labels. `h`/`l` folds/unfolds, and Space toggles a file mark. On a group, Space toggles all matching descendants, including folded rows; discovery marks only eligible owned, single-link ordinary files, not directories or links. Ineligible marking shows a prominent refusal; group marking reports omitted ineligible descendants. Filters affect group-marking candidates, never existing marks. The header reports marked and hidden counts. Actions offers Clear marks and Expand/Collapse all. Marks are session-local; managed marks clear on repository change. Refresh does not silently discard missing marked IDs: clear/reselect them if the next preview refuses them.

In Discover, `E` opens retained TARGET/ID/SOURCE drafts for every marked file. The editor replaces Discover content; the parent breadcrumb and page menus remain, but the underlying list/content and adoption actions are withheld. All TARGET/ID/SOURCE defaults appear in one scrollable table. Esc returns to Discover with marks and edits retained. Enter on any row requests **Review all**, so accepting defaults needs no per-file navigation. j/k or Up/Down selects an item; `e` / **Edit item** opens its separate TARGET/ID/SOURCE form. Tab/Shift-Tab moves fields only; Enter/**Done** or Esc returns to the batch list with edits retained, never reviewing/adopting only that item. Tab/Shift-Tab or field clicks are optional. List rows do not double as editable cells or footer focus stops. F3 opens complete selected-item paths. PgUp/PgDn scrolls them; Home returns to their top and Esc returns to the list/item form. Activate Review all to request one real combined preview, then **Adopt all** (Enter/click) approves once, without typed yes. Success returns to refreshed Discover with a brief status and receipt details retained. Suggestions can collide; edit them rather than silently dropping files. In Managed Files, uppercase `A`, `K`, `D` preview Apply selected, Keep selected, Delete selected links. Lowercase `a` remains full active-scope Apply. Hidden/folded marks remain included; row selection in the review never narrows the approved set. A successful full inventory refresh drops marks for removed mappings, including after Keep/Delete; valid filtered marks stay selected. An unavailable inventory retains marks until a successful refresh.

A batch contains 1–32 explicitly chosen files, one action and one repository. Every item and cross-item conflict is validated before live changes; one blocker refuses the whole batch. Review all effects; adoption uses one **Adopt all** activation, while other batch writes use one Confirm action for one schema-4 transaction. Delete confirmation displays **every exact mapping ID** and binds them internally; mouse clicks cannot submit those deletion acknowledgements or approval. Shrinking below 48×14 clears the entire acknowledgement sequence. Cancellation/dry-run creates no transaction. Failures during execution may leave partial effects: retain history/backups and use explicit guarded whole-transaction rollback, never an automatic undo or per-file retry queue. Adoption rollback withdraws its new sources into private storage; selected apply/keep/delete and their rollback leave repository sources unchanged. These batch mutation controls are TUI-only; existing CLI commands are unchanged. Recovery remains individually guarded, not bulk.

**Managed inventory:** Files → Managed shows the active mappings with source/destination kinds, drift and blockers in details. Invalid observation is reported as unavailable, never as a healthy empty inventory. `/` filters display only; `a` always previews the complete active-profile scope, even with no displayed matches. Esc leaves search without clearing the filter; Delete or Actions → Clear current display filter removes it. Refresh preserves selected identity where present. Actions → Keep local file: enter exact mapping ID / Delete managed link: enter exact mapping ID accepts a valid ID even outside the current filter. Enter requests Review directly from the ID field; only the subsequent preview can proceed to approval. Both modes retain repository sources; deletion confirmation displays and binds the exact selected mapping.

**Git:** choose Status, Unstaged or Staged from the Git menu to open metadata inspection. Its root defaults to the current file repository; `P` accepts an independent exact non-bare working-tree root. Relative paths resolve from the launch directory without tilde/environment expansion. This requires no manifest/registration and never changes file scope or configuration. Git → Follow selected repository restores the default. `z` reads Status (branch headers, kind/XY, names and rename originals); `u` reads tracked worktree-vs-index metadata; `s` reads index-vs-HEAD metadata, including unborn repositories. Status tables separate PATH/KIND/XY; diff tables separate PATH/CHANGE/MODES. `/` filters metadata only with a visible escaped query tail and caret; Delete clears the filter, Up/Down selects, and Enter inspects full paths (including rename originals), root, comparison and branch headers. Compact layouts open a separate detail pane; wide layouts show the table and details together. PgUp/PgDn scrolls ordinary metadata details with overlapping steps at compact sizes; Home returns to the start, and Esc returns to the list. Refresh preserves selected raw path where present. Empty comparisons, no display matches and unavailable reads are distinct; unavailable entry counts are unknown, not zero. Missing Git requires installing it separately and restarting Dort; path/read errors retain the root draft. No staging, commit, push or other Git mutation exists.

**Patch disclosure:** from a loaded Unstaged/Staged summary, `w` or Actions → View patch content opens a secrets warning. Activate View patch once to fetch the whole comparison, **not** only the selected/displayed file. Arrow keys/PgUp/PgDn scroll the bounded escaped content, including horizontal scrolling. Esc closes; root/comparison/destination changes, shrinking below 48×14 or exit also discard the UI reference, without promising secure memory erasure. There is no prefetch, automatic patch refresh, clipboard/export or retained patch history. Each new view needs the warning/phrase again; `R` fetches metadata only. Status/summaries, ordinary diagnostics and progress never contain patch content. Terminal capture/scrollback is outside Dort's control. Read-only patch viewing remains available in `--dry-run`, but not below 48×14. See [Git safety](safety.md#git-inspection-boundary).

**Profiles:** `T` opens a checkbox/profile/membership table distinguishing mandatory base/default/platform/local contributors from optional session additions. Up/Down or Tab selects rows; Space or a checkbox click toggles optional names. Enter on any row or **Apply additions** validates the new union before changing scope. F3 opens full metadata for the manifest, selected profile, contributors, requested additions and effective union; wide dialogs also show inline details. PgUp/PgDn scrolls metadata, Home returns to its top, and Esc returns to choices. Esc leaves scope unchanged. Mandatory contributors cannot be unchecked. Profile additions affect complete inventory/plans/repository checks and never persist; choosing a repository resets these additions.

**Checks:** `C` checks the current repository manifest. `v` opens an independent standalone path draft; enter a path, then activate Check. Its Temporary profiles button uses separate draft additions, still unioned with local/base/default/platform profiles. Standalone checks require no repository, registration or Git; relative paths resolve from the launch directory. Summary cards separate structural validity from selected file/tool counts; the pane title labels repository or standalone scope. PgUp/PgDn inspects the complete manifest path, schema, effective profile union and its contributors, dependency order and unsupported capabilities; Home returns to the start. Structural validity is not execution readiness: live files and installed tools were not inspected. Errors retain the path/additions; Discard clears them.

**Recovery:** Recovery → All repositories shows retained transactions across **all repositories**, independent of file selection/configuration. `/` searches metadata; Delete or Actions → Clear current display filter clears the display filter. The global write-blocker count is never filtered away, including while scrolling details. The table separates short transaction ID, schema (SCM), phase and integrity/write-block state; terminal history does not imply live-file safety. Compact layouts keep the summary, filter and selected row visible; wide layouts show the table and details together. A failed diagnosis is unavailable, not a healthy empty history. Up/Down selects; Enter shows full repository/transaction IDs, phase, schema, checkpoint count and integrity details without hash transcription. Use PgUp/PgDn for long details and Home to return to the start; Esc returns to the list. Page action buttons show eligible previews only. `b` previews schema-1/2/4 rollback; `s`/`x` previews schema-3 resume/abandon only when the diagnosis offers that action. Preparing offers Abandon, not Resume. Inapplicable operations remain discoverable in Actions with an explanation, also retained in full details. Hints are not permission: the core preview independently checks current paths/artifacts, and completed history is not proof rollback is safe. Schema-1 rollback withdraws the newly adopted source into private transaction storage; schema-2 rollback leaves repository sources unchanged. Backups/private artifacts are retained, not garbage-collected. Preparing bootstraps cannot resume: explicitly abandon, then start a new Create/attach transaction. Terminal or uncertain bootstrap histories cannot resume. Abandon retains published repositories, registration and private artifacts; it does not undo publication. Review the real preview and activate Confirm only when its effects are intended. No automatic recovery or repair occurs.

**Diagnostics/About:** in Checks, `j` reads host/architecture, HOME/config/state paths, registered count/active ID, session root, Git availability and global write blockers. Registry, history and blocker summary cards stay visible while PgUp/PgDn inspects full host/path and capability sections; Home returns to the start. Configuration and history availability are reported independently as unknown when unavailable, never zero; no local variable values or file contents appear. Diagnostics never installs tools or repairs configuration. `R` refreshes this report; `m` returns to the retained manifest result. Session → About shows Dort's build version, safeguards and unsupported capabilities; Esc returns to the prior destination.

**Activity/results:** Reading and Approved execution panels show the task's fixed scope, even if you change destination while it runs. Progress reports only received transaction boundaries/sequences, with no estimated percentage or Cancel control. q/Ctrl-C waits for completion. Result receipts retain the operation and full transaction ID through refresh, or explicitly say no changes were needed and no transaction was created. Use PgUp/PgDn to scroll the activity or receipt; Home returns to its top. Click Refresh (`R`), Recovery (`h`) or Back (Esc); direct destination/action keys also return to the workbench. There is no timed dismissal. Completed results remain inspectable in Overview and Files details after Back.

**Errors/retry:** The error panel separates the cause from retained task scope, with clickable controls. Esc/Back returns to retained input. `R` retries read/preview intent against current state, never replays a consumed approval or disclosed patch. After a failed write, `R` only refreshes observations; `h` opens all-repository Recovery. A blocked earlier operation has a separate Changes blocked panel naming its repository, operation and ID. It explains whether the operation is unfinished or its recovery record cannot be verified; Enter opens Recovery. This refusal does not receive a new partial-effects warning. Possible partial effects stays visible while scrolling, and an earlier-write warning remains for the session after Back or later reads. The warning is not proof of live drift, nor does dismissal repair anything. Retain artifacts and inspect Recovery. Choose an action again for a new preview and a new confirmation; do not assume a failed call made no changes.

Actions includes a session preview-only switch; it cannot change mode during a preview/worker. An entry `--dry-run` locks the entire session to preview-only and cannot be switched off.

Esc does **not** interrupt an approved write. q/Ctrl-C requests exit after it finishes, including on a small screen. Input/render errors restore the terminal then wait for the worker; no write is detached or automatically rolled back. A failed write may leave durable partial effects: dismiss the error, refresh Recovery, and explicitly preview the appropriate recovery action. Panics restore the terminal where cleanup is possible; forced termination cannot guarantee this. See [terminal/interruption safety](safety.md#terminal-and-interruption).

## Declared tool workflow

Tools opens the selected repository's declarations. The top tabs are **1 Declared**, **2 Installed**, **3 History**, matching Menu; press the number or click a tab on a collection page. Digits remain text in fields and filters. **Enter always opens the selected tool's details**; Esc returns to the list. Opening details runs no package-manager commands.

Use **Check installed** (`C`) to review detected package-manager checks, then activate **Run checks** once. Dort derives the platform, profiles, programs and command search directories. Results distinguish Installed, Missing, Version differs, Couldn't check and Needs attention. Installed means a matching package-manager claim, not verified command readiness. Unavailable observations never become Missing.

Use **Install missing** (`i`) to review the exact eligible set, then approve it once. Installations run serially and stop on failure; results identify completed attempts and tools not started. Earlier effects remain, with no automatic rollback or retry. Check again before another install review.

npm installation needs an exact version. Homebrew core uses the provider's current version on macOS, Linux and WSL. npm also needs a verified declared Node runtime and a safe global destination. Missing prerequisites and unsuitable scopes are explained with a next step. A review contains at most 32 installs; no-ready-item reviews offer details or declaration editing instead of an install action.

If detection needs help, open the affected tool's details and choose **Tool settings** (`s`). Enter only the necessary program/search directories, or Node/npm CLI/global prefix. Save changes session settings without running a provider; check again to review the new scope. Unsaved drafts do not affect checks.

```sh
dort tools check --dry-run                 # review without running providers
dort tools check                           # approve checks and show status
dort tools install-missing                 # approve checks, then one install set
dort --yes --json tools install-missing     # explicit automation approval

# Optional settings are keyed by your declared tool name.
dort tools check --program formatter=/absolute/path/to/node \
  --npm-cli formatter=/absolute/path/to/npm-cli.js \
  --npm-prefix formatter=/absolute/path/to/global-packages
```

Check/install-missing default to the selected repository and accept `--manifest PATH`, `--repo ROOT` and repeated `--profile NAME`. Persistent registered-repository profile additions are included. Installation requires registration. Nonterminal execution needs `--yes`; `--json` selects machine output. Check exits 0 for matching package claims, 4 for missing/version/attention results and 1 for unavailable checks.

## Installed tool discovery

Open **Installed tools** (`v`) to review detected package-manager scopes, then activate **Run inspection**. This can run package-manager configuration/plugins and change caches, but installs nothing. It needs no declarations or selected repository. npm discovered on PATH first reports its configured global prefix, then lists that prefix's packages; both commands are reviewed together. Declared npm checks and installation still require a Node runtime and accepted Node/npm CLI/destination settings.

The list shows only the current inspection result. Enter opens a package's details; `/` filters displayed packages. **Inspect again** (`R`) prepares a fresh approval. Opening, filtering or reading an existing result never queries a provider. Results are point-in-time observations, not a live inventory. Failed checks are unavailable, not healthy empty package lists. There is no inspection-history browser or binding step. Installation history is a separate page.

Select a package and choose **Add declaration** (`a`). Dort prefills its provider and package, an observed exact version for npm, and a known desired npm runtime when available. The install specification is platform-independent. Supply the desired tool ID and bare command name; inspection cannot infer those reliably. Review and confirm the manifest change separately. Saving returns to the installed results without rerunning inspection or installing the package.

```sh
dort tools inspect --dry-run
dort tools inspect                         # approve detected package-manager queries
dort --yes --json tools inspect            # current metadata for automation
dort tools inspect --manifest /absolute/path/to/dort.toml
```

Inspection accepts the same optional named program/npm settings when a manifest is available. Ordinary output is readable text; `--json` emits structured metadata. Raw provider output and file contents are withheld.

## Tool declarations

A declaration records desired metadata in `dort.toml`. Create or attach the repository first. **Add declaration** (`a`), **Edit declaration** (`e`) and **Remove declaration** (`d`) leave installed packages untouched.

The form asks for a tool ID, bare command name, provider and package. Choose Homebrew or npm; the same declaration applies on macOS, Linux and WSL. Profiles default to `base`; prerequisites reference existing tool IDs. Homebrew accepts an omitted or `latest` version. npm accepts default/latest or exact metadata, but installation needs an exact version and a declared Node runtime. Tab enters editing and traverses visible fields; Esc stops editing before going back. Space chooses a provider and F4 reveals optional dependencies. Provider-specific drafts are retained. Enter requests **Review declaration**; the exact preview has one **Confirm changes** action.

Editing prefills the selected declaration and keeps its ID fixed. Removal is refused while tool prerequisites, file prerequisites or npm runtimes reference it. It never cascades.

```sh
dort --repo /absolute/path/to/environment tools declare --dry-run \
  --declaration '{"id":"ripgrep","binary":"rg","install":{"provider":"brew","package":"ripgrep"}}'

dort tools edit --declaration '{"id":"ripgrep","binary":"rg","install":{"provider":"brew","package":"ripgrep"}}'
dort tools remove ripgrep --dry-run
```

Remove `--dry-run` and confirm to save, or use `--yes` for automation. The JSON describes one complete Tool using the [manifest fields](manifest.md); it is not an approval. `--json` selects structured preview/completion output.

Identical additions/edits are no-ops with no transaction. Conflicting additions, unknown edit/remove IDs and invalid references are refused. Unrelated declarations, formatting and comments are preserved. Additions retain schema-5 transactions; edits/removals use schema 6. Recovery can restore an unchanged result; later manifest edits can prevent rollback. Manifest changes require fresh tool checks.

## Installation safety

All installations use reviewed exact scopes and fresh independent checks under the application lock. No discovered command is executed to test readiness.

- **npm:** exact package/version with the declared Node/npm CLI/prefix. Global writes must stay in an accepted HOME destination, separate from the selected Node and npm CLI. Choose another explicit destination in Tool settings if needed; Dort does not silently move it.
- **Homebrew:** current `homebrew/core` formula with an accepted physical prefix. Native build/dependency code may change prefix contents and external state.

There is no sandbox, atomic package transaction or automatic rollback. Quit waits for the whole active approved batch. See [installation contracts](../tool-installation.md) and [safety](safety.md#bounded-tools-installs-and-unresolved-attempts).

## Installation history and record closure

**Installation history** (`h`) shows durable attempts, including partial effects. Enter opens the selected record's details without querying a provider. Refresh (`R`) rereads records only. This is installation history, not inspection-result history.

Unresolved records block later tool installations. On an unresolved record choose **Review record closure** (`x`). A known scope is retained; a damaged record asks only for the necessary package manager and trusted program/search/prefix paths. Confirming the exact review runs a fresh query and closes only the unchanged record. It does not retry, repair, undo or prove the installation succeeded.

```sh
dort tools attempts
dort tools attempts ATTEMPT_ID
dort tools close-attempt ATTEMPT_ID --dry-run
dort tools close-attempt ATTEMPT_ID

# For a damaged Homebrew record without a retained scope:
dort tools close-attempt ATTEMPT_ID --provider brew \
  --executable /absolute/PREFIX/Homebrew/bin/brew \
  --search-dir /absolute/physical/bin --brew-prefix /absolute/PREFIX
```

Damaged npm records also need `--provider npm --node PATH --npm-cli PATH --prefix PATH`; damaged Homebrew records need `--provider brew --executable PATH --brew-prefix PATH`. No current manifest or user-entered runtime ID is required for historical closure. `--yes` is the CLI automation approval alternative.

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
dort --repo /path/to/dotfiles manifest check --profile dev --profile linux
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

`recovery rollback` is an alias route for top-level `rollback`, handling schemas 1/2/4/5/6. Its preview checks affected guards and conservatively lists inverse paths without mutation. Approved execution independently revalidates the journal head/layout under the lock, then performs restartable guarded inverses. Completed transactions may be rolled back only while guards still match; later transactions/edits can invalidate them. An already-rolled-back transaction has no inverse paths.

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
