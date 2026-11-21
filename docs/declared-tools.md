# Declaration-driven tool checks and installation

Tools opens declarations; Enter always opens the selected tool's details. Named **Check installed** (`C`) and **Install missing** (`i`) actions own their exact reviews. **Installed tools** (`v`) supports current package discovery and declaration seeding, and **Installation history** (`h`) exposes durable attempts. There are no numbered action destinations, Advanced comparison or snapshot-selection workflows. The [user guide](user/commands.md#declared-tool-workflow) covers controls and CLI equivalents.

## Boundaries and scope

`services::declared_tools` is shared CLI/TUI orchestration over existing core inspection, binary planning and installation capabilities. Core owns package policy, version matching, provider execution, installation revalidation and attempts. Combined installation retains the existing attempt schemas and cancellation contract. Manifest-only edit/remove transactions follow the separate [declaration contracts](tool-declarations.md).

The application captures PATH through `environment::inspection_inputs`; preparation runs no programs. It reads the bounded manifest independently of file observations, applies registered-repository persistent profile additions plus explicit session additions, canonicalizes existing absolute UTF-8 search directories, deduplicates them and enforces the core directory bound. Existing unreadable scopes remain unavailable. There is no process-global HOME/PATH mutation.

Only providers needed by active declarations enter the check review. Detected physical programs and accepted Tool settings use process-free inspection-default preparation. npm requires a compatible declared runtime and explicit Node/CLI/prefix scope. Each ambiguous runtime needs its own accepted settings; another runtime's settings never supply a fallback. The review exposes tool associations and exact query scopes before a single-use approval. Normal checks associate only these new scopes, never historical snapshots or merged catalogs. Stable internal scope IDs identify this check, not persisted trust.

Preparation binds the manifest and local configuration. Checks revalidate that source before executing approved queries and after comparison. Failed/unavailable queries remain unknown; raw provider output and manifest contents are withheld from diagnostics. Internal scope/record limits and all existing provider/runtime guards remain in force.

## Status

Status is derived from core package findings: Installed, Missing, Version differs, Couldn't check or Needs attention. Installed means a satisfying package-manager claim, not verified binary readiness or ownership. Details retain command-name observations, versions and blockers. Unknown never becomes Missing. Inactive declarations remain visible and are identified after checking the active scope.

The frontend retains only session metadata. Repository, declaration or profile changes invalidate results/reviews. After an install attempt, completed typed outcomes may be shown, but the old comparison cannot authorize another installation; a fresh check is required. No implicit query runs on page entry, navigation, refresh or result dismissal.

## Combined installation approval

`Checked::prepare_missing` independently verifies its source and prepares existing core install previews for eligible absent rows. npm uses exact versions for installation; Homebrew uses provider-current default/latest. It reports deferred IDs with typed reasons and next steps for unresolved prerequisites, scope, programs or version policy. Missing and ready counts are distinct. An empty review shows Installation needs attention, without an execution warning or install action; Enter opens the first affected tool's details and `e` opens its declaration. Unsupported version-policy exclusions explain how to edit the declaration. Mixed reviews label deferred tools as not included. Existing installed targets and version differences are not upgrades. More than 32 eligible previews refuses the review rather than truncating it.

The aggregate review shows every eligible tool's exact core scope, arguments, provider/native-effect warnings and deferred tools. Approval consumes this immutable set once. Execution requires the application writer lock, validates the source again, and calls the existing core executor serially. Each package independently revalidates its opaque scope, performs fresh inspection and records its existing durable attempt before mutation.

Execution stops on the first error or unsuccessful typed outcome. Results retain per-tool reports/errors and not-started IDs. Earlier effects remain; there is no rollback, retry or approval replay. Deferred prerequisites need a new check/review after their installation. Quit and terminal errors join the entire active worker, including later items in the already-approved set.

## Interfaces and validation

CLI `tools check` and `tools install-missing` use the same preparation, approvals and execution. Dry-run is process-free; nonterminal/JSON execution requires `--yes`. Installation JSON distinguishes pre-install facts from outcomes and not-started tools.

The TUI reuses shared pair tables, forms, notices, readers, semantic warning roles and a relevance-ordered foreground action strip. List Enter only opens details; details Enter cannot start a hidden check or install. Reviews show human-readable exact scopes without F3 alternate modes or raw JSON. Help and menus retain opaque reviews; actual navigation and below-floor layouts discard them. Mandatory execution/no-rollback warnings remain outside the scrolled review when an installation is ready.

[Service fixtures](../crates/dort/src/services/declared_tools_tests.rs), [CLI integration tests](../crates/dort/tests/declared_tools_cli.rs), [actual TUI tests](../crates/dort/src/tui/tests/declared_workflow.rs) and the [PTY harness](../scripts/test-terminal.py) use disposable canonical homes and fake providers. They cover process-free preparation, automatic associations, unknown-versus-missing facts, stale scopes, exact aggregate approval, serial outcomes, stop-on-failure, dry-run/layout guards and joined quit. Automated passes do not establish human visual/usability approval or real-provider compatibility.
