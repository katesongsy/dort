# Core provider inspection

The [shared interaction contracts](tui-interactions.md) apply to declared tools and Installed tools. Enter opens a selected tool or package. Named Check/Inspect actions prepare an exact review; only its explicit approval executes package-manager queries. Browsing and opening menus never reruns inspection.

`dort_core::tools::inspection` implements deliberate local provider queries. CLI `tools inspect` and the Tools TUI share the current-batch `services::installed_tools` adapter over `services::tools`. Check installed associates its newly approved scopes with declarations and performs [internal comparison](binary-reconciliation.md) through the [pure package planner](tools-planning.md). Current results are point-in-time metadata; there is no inspection-history browser or user binding workflow. These inspection APIs do not install packages. The separate [bounded Tools installation](tool-installation.md) capabilities reuse fixed inventory queries for fresh validation and post-observation. The Homebrew installer separately discloses help/layout/canonical-formula probes; ordinary inspection collection never adds them. Pure version policies in `tools.rs`, manifest checks, filesystem observation and planners do not execute providers. End-user usage is in the [command reference](user/commands.md#installed-tool-discovery).

## Frontend workflow

`services::tools::Request` prepares an explicit managed scope without executing a provider or loading repository registration. Its bound npm request reads an absolute manifest through the bounded no-follow stable-identity service; source-bearing errors become static diagnostics. The wrapper retains the manifest scope and private session-only fingerprint/hash provenance beside the opaque preview and report. These facts let reconciliation refuse changed or different declarations; they are skipped by serialization and grant no authority. Other providers reject npm-only fields. Independent Installed discovery uses the separate manifest-free npm inventory capability below.

CLI terminal execution displays the exact preview and asks whether to run it (`y/N`); explicit `--yes` is required for nonterminal/JSON execution. `--dry-run` returns only the preview even with `--yes`. A completed unavailable observation is emitted as a report with exit 1; invalid input/approval is exit 2. No serialized preview can be executed.

Tools opens declarations without querying providers. **Check installed** (`C`) prepares scopes needed by active declarations; **Installed tools** (`v`) prepares detected package-manager scopes for independent package discovery. `environment::inspection_inputs` captures PATH at the application boundary. `services::inspection_defaults` filters unsafe PATH entries and resolves metadata without executing candidates, version probes or config queries. Declared npm checks need an unambiguous declared Node runtime or independently accepted per-runtime settings. Installed discovery can instead select npm from PATH without a repository or Node declaration. Conventional Node/npm co-location supplies managed-scope suggestions, not ownership or permission to install into that prefix. Missing/ambiguous facts remain unavailable. Necessary Tool settings derive the provider, manifest and runtime association instead of asking users for internal IDs. Saving settings is process-free; a later query still requires approval.

One review owns at most 16 real scope previews, prepared before any query runs. Approval consumes them once; the joined worker runs them serially and retains at most 65,536 package records. Scopes and Node runtimes remain distinct, never merged. Installed tools keeps only the current result; accepting a refresh discards the earlier result before execution, so failure cannot fall back to it. Preview-only and below-floor layouts cannot execute. Navigation/shrink drops approval; late preparation cannot reopen a departed page. Retry prepares a fresh review, not query replay. Filtering is display-only. Add declaration copies typed known package fields into a retained draft and requires a separate manifest-write preview/approval. It runs no provider and infers neither command names nor package ownership. Raw provider payloads stay outside reports, diagnostics and persistence.

## Authority and trust

The caller supplies a trusted absolute executable, absolute home and nonempty absolute executable-search directories. Paths must be UTF-8 and contain no NUL or dot components; an empty/relative PATH entry is rejected. The core never searches PATH or executes candidates to identify them. Queries run at `/`, not in a repository. Supplied paths remain trust decisions, not immutable executable identities or isolation against same-user replacement/configuration races.

Constructing `Preview` and serializing its `Description` are process-free. The description exposes the executable, fixed arguments, home, search directories, runtime binding, limits and risk warning. Only consuming that live preview into an opaque, single-use `Approval` permits inspection. Previews/approvals cannot be deserialized; approval is not installation authority. Presentation is responsible for deliberate confirmation of the exact preview; the core cannot establish human consent.

Provider queries are **not guaranteed side-effect-free**. Trusted global configuration/plugins can execute code and provider caches/logs can change. Offline/no-update settings are hardening, not an OS sandbox or a guarantee that a malicious provider/plugin cannot use the network. No installation, privileged handoff, trust persistence, receipts or recovery-journal changes are implemented here.

## Fixed protocols

| Query | Executed operation | Retained metadata |
| --- | --- | --- |
| Homebrew formulae | `brew info --json=v2 --installed --formula` | Formula `full_name` and each installed version; no casks or inferred binary ownership. |
| npm globals | Explicit Node executes an explicit npm CLI script: `ls --global --json --depth=0 --offline --ignore-scripts --logs-max=0 --update-notifier=false --audit=false --fund=false --prefix PREFIX` | Top-level package identifiers/versions only; no resolved URLs, transitive trees, scripts or diagnostic strings. Incomplete/error responses are unavailable. |

npm requires a manifest-declared runtime tool ID, trusted absolute Node and npm CLI paths, and an explicit global prefix. The npm launcher/shebang is never used, so a different Node is not silently selected through PATH. The binding is retained in the description/report. It records the caller's selection, not proof that a declared runtime owns that executable or prefix, or that Dort installed anything. Generic construction of an unbound managed npm query is rejected.

### Standalone npm inventory

Installed discovery has a separate `Preview::npm_inventory` capability. It captures one metadata-resolved npm executable from the reviewed PATH, including a launcher or shim, and first runs `npm prefix --global --offline --ignore-scripts --logs-max=0 --update-notifier=false --audit=false --fund=false`. After validating the response it runs the fixed `ls` arguments above through that same npm executable with `--prefix REPORTED_PREFIX`. The launcher can select Node from the reviewed PATH; the review discloses both commands and that selection rule.

The prefix must be one absolute UTF-8 path without controls, NUL or dot components, at most 4,096 bytes and 64 components. It is an argv value only: Dort never opens, canonicalizes or executes a reported prefix path. Both queries share a ten-second deadline and 4 MiB combined output budget; the prefix probe reserves 8 KiB. Failed or oversized prefix discovery prevents the list query. Only the validated actual prefix appears in result metadata; raw provider text stays private. Global configuration and caches can still have effects.

This capability needs no manifest, repository or runtime declaration. Its report has no managed Node/CLI binding and cannot authorize installation. Installation still requires a declared runtime and its independently reviewed exact binding.

## Child lifetime and privacy

Children receive a cleared environment, explicit HOME/XDG directories and PATH, C locale and no-color setting. No parent credentials, Node options, shell startup variables or arbitrary environment overrides are forwarded. Homebrew disables auto-update/analytics/environment hints and API installation; npm requests offline inspection. These controls do not neutralize trusted provider configuration/plugins.

The direct child runs in a separate process group with null stdin. Both pipes drain concurrently using nonblocking reads. Combined stdout/stderr is bounded to 4 MiB and the operation to ten seconds, including descendants retaining pipes. Remaining group members are killed on completion/failure and the direct child is waited for. There is no detached background inspection job.

Raw stdout is private to execution/parsing; stderr is counted and discarded. Neither is exposed through Debug, serialization, logs or errors. Parsers retain only bounded ASCII metadata identifiers/versions, sort/deduplicate exact repeats, preserve distinct versions and reject malformed/unsupported protocols. A failed observation is a typed `Unavailable`, never `Complete` with an empty list. Provider claims are neither installation receipts nor evidence that a PATH binary satisfies a declaration. Version-policy matching alone grants no authority.

## Validation

Core unit tests exercise protocol whitelisting, empty versus malformed/unavailable results, multiple versions, incomplete package states, limits, concurrent pipe drains and descendant-held-pipe deadlines. Integration tests use canonical disposable homes and deterministic fake providers, covering inert previews, fixed arguments, sanitized environments, explicit npm bindings, path rejection and raw-output redaction. Frontend CLI/state/view tests exercise approval modes, independent scope, retained fields, shortcut-as-text input, explicit activation, resize/preview-only guards, late-preview discard, search/full metadata and unavailable results. Caret/focus/cell-style checks supplement disposable PTY tests for real interactive CLI/TUI confirmation and fresh-review behavior. They require no installed package manager or Node. These fixtures establish adapter behavior, not real-provider compatibility or sandboxing. See [validation methods](validation.md) for ordinary gates and fixture boundaries.

Protocol references: [Homebrew](https://docs.brew.sh/Manpage), [npm prefix](https://docs.npmjs.com/cli/v11/commands/npm-prefix/), [npm ls](https://docs.npmjs.com/cli/v11/commands/npm-ls/).
