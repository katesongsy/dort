# Workbench interaction contracts

The workbench separates navigation surfaces from approval ownership. End-user keys and workflows are documented in the [command reference](user/commands.md#interactive-interface).

## Pages and dialogs

Workflow forms, batch editing, reviews, results, errors, history and ignored-path management are navigable pages. `surface.rs` provides page titles, breadcrumbs and Back. F10 and render-owned mouse menu targets remain available while editing text. Opening/dismissing a menu preserves fields, focus and opaque previews; selecting an available destination, including the same tab, drops pending approvals and private Git payload references but retains drafts.

Short repository/profile choices, Actions, Help/About and existing final confirmation/disclosure gates are framed, dimmed dialogs. Only the foreground owns hit regions and input caret. Background clicks and navigation attempts explain the paused navigation; they never implicitly cancel, approve or disclose content. Esc and explicit Close/Cancel controls leave the dialog. A running worker also pauses navigation, with visible feedback; q/Ctrl-C waits for the one owned joined worker.

## Direct actions and metadata

Tab/Shift-Tab traverses actual form fields, never footer actions. Enter requests the primary validation/review from any field, focusing a genuinely missing field when needed. Secondary actions use explicit nonprinting shortcuts or typed click targets; ordinary printable shortcut letters remain text in fields. Full form metadata uses F2 and scrollable views. Shared action bars wrap controls independently of field focus.

Choice and inventory lists traverse rows only. Space toggles optional selections; Enter invokes the contextual primary action from any row. Profile Enter validates/applies additions, while Space or checkbox clicks toggle optional names. Repository F2 opens Create/attach. Picker/Actions F3 opens scrollable full metadata; Esc returns to choices. Enter in metadata cannot activate hidden rows. Actions Esc closes directly, including from search.

Inspect installed tools defaults to four manager rows. Space toggles eligible real query previews; Enter consumes the selected previews once. `e` opens Advanced for the selected row, `r` refreshes suggestions without querying, and `d` shows exact scopes. Advanced and declaration Enter prepare a read-only review; only the subsequent existing gate can execute a provider or write a declaration.

Batch adoption separates list selection from the three-field item form. List Enter reviews the complete marked set; `e` edits one item. Item Enter/Done or Esc retains edits and returns to the list, never requesting an individual preview. F2 exposes full selected-item paths.

Discover Enter remains inspection, not adoption; Recovery Enter remains inspection. Git patch content still requires its separate deliberate warning and explicit disclosure action. Approval, dry-run, layout-floor, shrink, freshness, source identity, unresolved-attempt and joined-worker contracts remain in the core services and existing frontend gates; navigation hints do not grant authority.

## Text and validation

Trusted Unicode decoration is distinct from metadata. `StatusText` escapes owned dynamic ingress; trusted static/count chrome is explicit. Metadata still escapes non-ASCII, control and bidi characters at boundaries. There is no global unescape. Terminal-cell clipping preserves UTF-8 and existing independent ASCII/color fallbacks.

State/backend tests cover row/field traversal, single-use real previews, draft retention versus actual navigation, dialog dimming/hit capture, foreground caret ownership, compact action availability and malicious metadata. Disposable-fixture PTY journeys supplement these assertions. Automated rendering and terminal tests do not establish unfamiliar-user intuitiveness; see [validation limits](validation.md).
