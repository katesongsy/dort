# Workbench interaction contracts

The [design system](design-system.md) is the mandatory visual/component standard for new or changed surfaces; it is not a claim that every legacy screen already conforms. The workbench separates navigation surfaces from approval ownership. End-user keys and workflows are documented in the [command reference](user/commands.md#interactive-interface).

## Simplicity and predictable actions

These rules apply to new or changed workflows; the [command reference](user/commands.md#interactive-interface) records the currently implemented controls.

- Navigation contains places and collections. Check, install, refresh and approve are actions, not numbered destinations.
- A list's Enter opens the selected object's details. Keep named task bindings stable; changing status must not silently change Enter from Check to Install. Reviews expose their single approval action explicitly, and readers never approve underlying content.
- Expose one workflow per user goal. Provider inspection and comparison are internal stages of Check installed, not separate Advanced workflows to learn. Settings appear for the affected tool when configuration is necessary.
- Derive internal IDs, associations and routine defaults from the declaration and reviewed context. Ask for a value only when it cannot be resolved safely. Exact scope remains inspectable before execution.
- Retain check observations and provenance internally. Multiple historical snapshots are not a user selection workflow; stale or unavailable results call for a fresh approved check, not manual snapshot reconciliation. Writes still independently revalidate.
- The bottom strip follows controller-supplied relevance order: primary action, next useful step/selected-item actions, then secondary operations. Back and Help occupy reserved trailing positions; More precedes them when needed. Overflow removes the least relevant actions first without changing the order of the remainder. Rendering, mouse hits, keyboard activation and More share this model.
- Tests must cover stable key meanings across status changes, absence of action destinations/internal-ID prompts, relevant action order and compact overflow, alongside approval/freshness guards. Existing screenshots or tests do not justify retaining an unnecessarily complicated workflow.

## Local destination tabs

Sections with multiple stable collections show numbered tabs at the top using the shared navigation component and the same catalog/order as Menu. Tools has **1 Declared**, **2 Installed**, **3 History**; Check, Install, Add and Edit remain actions. Tabs identify the current collection and share keyboard/click navigation guards and compact overflow with Files and Git. Numbers navigate only on stable browsing pages outside text entry. Filters and fields keep digits as text; details, forms, reviews, results, errors and busy states keep their contextual actions and Back path instead of collection tabs.

## Pages and dialogs

Workflow forms, batch editing, reviews, results, errors, history and ignored-path management are navigable pages. `surface.rs` provides page titles, breadcrumbs and Back. The render-owned Menu mouse target and Ctrl-G navigation shortcut remain available while editing text; Ctrl-G also dismisses that menu. Ctrl-K provides keyboard access to searchable destinations and tasks. Outside Menu, Left/Right never changes destinations. Opening/dismissing a menu preserves fields, focus and opaque previews; selecting an available destination, including the same tab, drops pending approvals and private Git payload references but retains drafts.

Short repository/profile choices, Actions, Help/About and sensitive-content disclosure gates are framed, dimmed dialogs. Only the foreground owns hit regions and input caret. Background clicks and navigation attempts explain the paused navigation; they never implicitly cancel, approve or disclose content. Esc and explicit Close/Cancel controls leave the dialog. A running worker also pauses navigation, with visible feedback; q/Ctrl-C waits for the one owned joined worker.

## Direct actions and metadata

Forms open outside field editing. Tab enters the first field; Esc leaves field editing while retaining the draft, and Tab enters it again. Esc outside editing returns one page. Tab/Shift-Tab traverses actual form fields, never footer actions. Enter requests the primary validation/review from any field, focusing a genuinely missing field when needed. Secondary actions use explicit nonprinting shortcuts or typed click targets; ordinary printable shortcut letters remain text in fields. A separate metadata reader, where needed, cannot submit a hidden form. Tools forms expose their facts through human-readable review rather than alternate F3 modes. Shared action bars wrap controls independently of field focus.

Choice lists traverse rows only. Space toggles optional selections; explicit pickers select with Enter. Tools, file, Git and history lists use Enter to open selected details, not to change between background tasks. Profile Enter validates/applies additions, while Space or checkbox clicks toggle optional names. Repository Ctrl-N opens Create/attach. Picker/Actions F3 opens scrollable full metadata; Esc returns to choices. Enter in metadata cannot activate hidden rows. Actions Esc closes directly, including from search.

Installed tools prepares detected query scopes without executing them. Its exact review has one Run inspection action. The result list keeps only the current batch; Enter opens a package and `a` seeds a declaration with known fields. `R` prepares a new review, and accepting refresh discards the old batch before querying. Filtering and navigation never query. Necessary provider/runtime settings are attached to a declared tool, not exposed as snapshot or internal-ID forms.

Batch adoption separates list selection from the three-field item form. List Enter reviews the complete marked set; `e` edits one item. Item Enter/Done or Esc retains edits and returns to the list, never requesting an individual preview. F3 exposes full selected-item paths.

Files, Recovery and Git Enter opens a full reading page at every width; reader arrows and paging scroll metadata, never hidden list rows. Esc returns to the retained list. Discover Enter remains inspection, not adoption; Recovery Enter remains inspection. Git patch content still requires its separate deliberate warning and explicit disclosure action. Approval, dry-run, layout-floor, shrink, freshness, source identity, unresolved-attempt and joined-worker contracts remain in the core services and existing frontend gates; navigation hints do not grant authority.

Tools list Enter always opens the selected tool. Named `C` Check and `i` Install missing prepare human-readable exact reviews; details Enter starts neither. Check derives relevant scopes and associations. Install reviews one immutable eligible set with explicit deferral reasons/next steps, execution/no-rollback warnings. Empty reviews offer details/editing instead of an install action. Execution is serial and stops on failure with partial effects/not-started tools retained. A later installation needs a fresh check. Navigation/shrink discards approvals; menus/Help retain them.

Installed package browsing uses shared tables, readers, filtering and the foreground strip. Scope facts are readable before query approval and package details are readable afterward, without F3 alternate modes. Add declaration opens a retained draft and a separate manifest-only approval; it never reruns inspection. Installation history remains a distinct collection of durable attempts. Enter reads a record; `x` requests guarded record closure, asking only necessary scope paths when the record is damaged. No history action replays an installer. Menu lists Declared tools, Installed tools and Installation history; Check/Add/Install remain actions.

Reviewed operation previews own approval: Enter confirms the exact displayed scope once and consumes its opaque approval. There is no subsequent confirmation dialog. Blocked, dry-run and below-floor layouts cannot approve. The declaration list and selected-tool details expose `e` Edit and `d` Remove. Edit opens a retained prefilled form with a fixed tool ID; Remove prepares the exact manifest-only preview. Installed packages stay untouched, and invalid prerequisite/runtime references prevent the change. Declaration Done returns to the refreshed declared-tools list. Tools Esc returns through the actual parent pages, retaining drafts and selections but discarding approvals.

## Text and validation

The single foreground action strip shows a relevance-ordered prefix of at most eight entries. The controller supplies order: primary row/action first, next useful task, selected-item management and secondary actions. Applicable Back and Help controls stay reserved at the end; root collections omit inert Back. F2 More precedes those controls when eligible actions overflow. Hidden actions are the exact ordered suffix, with the same keyboard/mouse dispatch model. Help, dialogs, errors and tasks own their foreground strip, never background shortcuts. Compact labels retain meaning: History must not become Install. Forms describe field/review actions and explicit readers describe scrolling/Back. Ctrl-K exposes named tasks; Menu exposes places. Neither grants execution authority.

Trusted Unicode decoration is distinct from metadata. `StatusText` escapes owned dynamic ingress; trusted static/count chrome is explicit. Metadata still escapes non-ASCII, control and bidi characters at boundaries. There is no global unescape. Terminal-cell clipping preserves UTF-8 and existing independent ASCII/color fallbacks.

An earlier-operation write blocker has its own Changes blocked feedback: explain why new changes are paused, name the affected repository/operation when known, and make Recovery the primary action. Do not label that refusal as a new partial-effects failure. Actual write failures retain their pinned risk and joined-worker guards; neither message grants repair or rollback authority.

State/backend tests cover row/field traversal, single-use real previews, draft retention versus actual navigation, dialog dimming/hit capture, foreground caret ownership, compact action availability and malicious metadata. Disposable-fixture PTY journeys supplement these assertions. Automated rendering and terminal tests do not establish unfamiliar-user intuitiveness; see [validation limits](validation.md).
