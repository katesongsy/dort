# TUI design system

[Documentation](README.md) · [Interaction contracts](tui-interactions.md) · [Contributor instructions](../AGENTS.md)

This is the mandatory design standard for new or changed workbench components. It is a contributor contract, not a claim that every existing screen already conforms. Current end-user behavior is documented in the [command reference](user/commands.md#interactive-interface). Temporary designs, migration worklists and candidate-specific review evidence are not maintained here.

## Experience and information hierarchy

The workbench is destination-led: users choose a place, inspect objects and enter focused task pages. Searchable commands are an accelerator, never the only route to a supported task. Use one clear location/title, relevant scope data, working content and one foreground action strip. Search fields contain queries, not shortcut catalogues. Status is not another command footer.

Use progressive disclosure without hiding required scope or risks. Prefer task language such as Installed packages, Inspection settings and Review installation to internal architecture terminology. Provider reports, installer outcomes, command readiness and historical closure are independent facts. Never convert a favorable package observation into a blanket healthy/success state.

## Semantic tokens

All shared visual components live in the Rust module folder `crates/dort/src/tui/components/`; this is not a separate Cargo package. Page/workflow controllers and opaque approval ownership remain outside. `design.rs` owns semantic recipes, `glyphs.rs` owns trusted decoration/fallbacks, `motion.rs` owns deterministic liveness state and `surface.rs` owns shared framing. Components request roles from the shared visual vocabulary. Page renderers must not introduce raw colors, emphasis modifiers, semantic glyphs or private animation loops. Different jobs have different shapes; they must not all become similarly styled paragraphs.

| Role | Treatment | Meaning |
| --- | --- | --- |
| Main text | Terminal foreground, normal weight | Data and readable values. |
| Heading | Main text, bold | One page/location title or a meaningful section. |
| Supporting text | Neutral, restrained emphasis | Scope, labels and explanation; not illegible fine print. |
| Interactive | Cyan accent | Focus, active navigation, primary action and active work. |
| Selection | Inverse/selection surface plus leading marker | Current row, never checkbox membership or approval. |
| Success | Green plus a specific fact and completion glyph | A confirmed narrow success, not inferred readiness. |
| Attention | Amber plus `!` and a specific label | Risk, blocked prerequisites, unavailable observation or overdue waiting. |
| Error | Red plus failure/invalid/destructive wording | Failed operation, invalid input or destructive action. |
| Information | Blue plus a source/fact label | Recorded/historical evidence, not success. |
| Unknown | Neutral plus `?` and explicit wording | Not inspected or not verified; distinct from unavailable. |

Preserve terminal foreground/background by default. Monochrome removes color, not hierarchy. Reference explicit dark/light palettes are below; these do not authorize automatic background guessing or claim an appearance setting exists. Quantized/terminal-defined palettes need their own rendering review.

| Token | Dark reference | Light reference |
| --- | --- | --- |
| Canvas | `#10141C` | `#F7F9FC` |
| Surface / raised | `#18202C` / `#202B3A` | `#EDF1F7` / `#FFFFFF` |
| Selection surface | `#193845` | `#DCEFF3` |
| Main / secondary / quiet text | `#E6EDF3` / `#A9B6C7` / `#94A3B8` | `#182235` / `#4E6076` / `#596B82` |
| Interactive / text on interactive | `#66D9EF` / `#071C24` | `#006678` / `#FFFFFF` |
| Success / attention | `#7BD99B` / `#F2C66D` | `#176A3A` / `#805600` |
| Error / information | `#FF8A93` / `#98B8FF` | `#B3233A` / `#305CB3` |
| Decorative border | `#46556B` | `#8B99AD` |

Normal text on reference surfaces targets at least 4.5:1 contrast. Decorative separators are not text. Do not use a muted treatment to hide full identities, required warnings or unavailable reasons. A terminal has one cell-based font size: use weight, placement and spacing, not smaller critical text or reliance on italics.

## Component recipes

| Component | Required presentation and behavior |
| --- | --- |
| Page | Neutral ground, one location/title, focused content; no decorative outer box or duplicated breadcrumb/title stack. Full workflows, forms, reviews, progress, results and substantial errors are pages. |
| Table/list | Aligned columns, concise heading, clear selection marker and independent marked membership. Do not concatenate unrelated fields into a pseudo-table or append action buttons to row traversal. |
| Filter | Object-specific label, editable surface and caret only while focused. No commands inside its title/border. Filtering never executes a provider or changes operation scope. |
| Text field | Label plus editable surface; focused boundary/marker and real caret; invalid state retains value with a local error. |
| Choice/checkbox | Same control vocabulary, no text caret; visible choice/membership state. Mandatory membership is labeled Required. |
| Read-only fact | Label/value, not an input-shaped box. Full identities remain inspectable. |
| Badge | Compact glyph/word for one fact; not a pill on every noun or a full colored paragraph. |
| Callout | One semantic edge/marker, short bold lead and readable neutral explanation. Risk/error/information treatments differ. |
| Primary action | One clear verb with interactive emphasis. A destructive action has an explicit destructive label and error treatment. Green is not a generic Confirm button. |
| Secondary/disabled action | Secondary emphasis; a disabled action never looks primary or acts, and its reason is available. |
| Dialog | Raised/enclosed foreground, muted background and one input owner. Short choices, launchers, Help and existing final gates only; no long workflow stuffed into a modal. |
| Review | Structured requested change, exact scope, other effects, uncertainty and explicit next decision. Important risks remain visible; no generic raw-plan wall. |
| Result | Honest headline plus independent process/package/binary facts. No blanket success celebration for partial or uncertain outcomes. |

Selection, focus and marked membership are separate axes. A selected failed row retains selection treatment and an independently labeled failed fact. A focused invalid field keeps a caret/focus marker and error boundary without flashing or nested outlines. A dialog suppresses background caret and active hints. Metadata escaping and private-content policies remain authoritative; visual styling never broadens disclosure.

Use a small single-cell glyph vocabulary with ASCII fallbacks: selection `›`/`>`, disclosure `▸`/`>` and `▾`/`v`, completion `✓`/`+`, failure `×`/`x`, attention `!`, uncertainty `?`, information `i`, pending `·`/`.`, and `[ ]`/`[x]`/`[-]` membership. No emoji, private-use font dependency or color-only meaning. Unknown-width decoration falls back conservatively. Trusted glyphs are never a reason to unescape untrusted metadata.

## Actions and interaction

The [interaction contract](tui-interactions.md) owns input/approval behavior. The visual system must describe the actual foreground, not advertise underlying shortcuts. Keep one primary action and one foreground key-hint region with up to eight relevant entries. Show F2 More only when additional eligible actions do not fit; overflow belongs there, not in another footer. A page's primary task must not require guessing a shortcut or opening a command palette.

Global navigation uses one visible Menu control with a Ctrl-G shortcut, not a separate Places control. Its explicit navigation surface presents sections and their local destinations together: Up/Down selects a section, Left/Right selects a destination, Enter opens it and Esc dismisses the menu. Navigation arrows outside that surface remain owned by the foreground. Stable section destinations are also visible and directly clickable on section pages, with the current destination identified; compact layouts expose an overflow control for destinations that do not fit. Outside text entry, plain o/f/t/r/c/g directly opens Overview/Files/Tools/Recovery/Checks/Git. Menu and the wide rail share that order, with Session last in Menu. Reserve these lowercase letters for sections; contextual O/F/T/R/C retains adoption/discovery/profiles/refresh/inspection actions, P chooses Git scope, and Home replaces the former gg first-row chord. Fields and filters accept uppercase and lowercase letters literally. Session remains reachable through Ctrl-G Menu. Show these bindings on the wide section rail and Menu rows. Number local destinations in visible order (1, 2, 3, ...), consistently between section pages and Menu. Numbers activate only within the current section on stable browsing pages outside text entry, or within the selected Menu section; they remain literal input in fields and filters. Direct shortcuts use the same navigation/approval guards as destination clicks. Focused workflow forms, reviews and results retain their title and Back path rather than becoming permanent destinations. Opening or dismissing Menu retains drafts and opaque previews; actual navigation discards pending approvals.

Forms traverse input controls, never action-strip buttons. Enter requests the contextual primary step from a single-line field; a short choice surface owns its selection/application behavior. Esc returns one meaningful level with drafts retained. Menus merely opened/dismissed retain focus and opaque previews; actual navigation drops approval. Do not remap existing workflow keys incidentally while changing styling: migrate a coherent surface, document its actual controls and validate the complete path.

## Motion and honest progress

Motion communicates active owned work, not decoration or inferred advancement.

- Reveal a busy spinner after 150 ms to avoid flicker; input/approval restrictions apply immediately.
- Advance at most every 100 ms (10 fps), or 125 ms for the ASCII spinner. Use one foreground running indicator. Keep its cell width stable.
- Indeterminate spinner means the task has not completed; it does not prove child-process progress.
- Determinate bars require actual completed/total units and display the unit. Never invent installation-time percentages, interpolate from elapsed time or imply durable completion from a counter.
- Stage changes come from real service events, not clocks. Completed, active and pending phases have distinct static/running treatments.
- Overdue work uses attention wording and preserves completion/termination uncertainty. Exit requested during work says it is waiting; it does not claim cancellation or root-descendant termination.
- Stop animation at completion or when its foreground is hidden. Static screens have no animation redraw loop. Input/data updates take priority.
- Reduced motion uses a static marker and explicit Working/Waiting label. Color, Unicode and motion capabilities are independent. Native caret behavior remains user/terminal controlled.
- No blinking errors, pulsing approvals, sliding tables, fake percentages, animated warnings or celebratory success transitions. Do not delay keyboard ownership with fades.

Timing stays at the application/UI boundary. Animation adds no provider queries, detached workers, cancellation behavior, logs or private payloads. Existing writer locks, opaque approvals, dry-run/floor/shrink/source guards, explicit patch disclosure and joined-worker lifecycle remain unchanged.

## Agent workflow and enforcement

Before changing TUI behavior, read this page and the interaction contract. Identify the shared components/roles being reused. New component semantics, exceptions or changes to the standard require explicit operator review; routine reuse does not require reapproval.

Run the [validation gates](validation.md), including the style checker and component/backend tests. Do not update an exception list or visual expectation merely to make a failing test pass. Inspect the difference against the standard, preserve unrelated work and report actual evidence separately from unobserved CI or human usability. No AI-generated snapshot update is its own design approval.

The style checker rejects raw literal table highlight symbols as well as page-local color, emphasis, glyph and clock drift. Shared disclosure decoration is composed separately from escaped row metadata. This is a mechanical boundary, not a proof of design quality. The component gallery must render actual shared Ratatui components with fixture-only data. Review compact, baseline and wide layouts, Unicode/ASCII, monochrome, focus/selection/membership, error/disabled states and full/reduced motion. Deterministic state/cell/PTY tests supplement unfamiliar-user task completion; they do not establish intuitiveness.
