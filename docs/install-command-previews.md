# Non-executable install-command descriptions

`tools::install_preview` adds inspectable command metadata to [pure package planning](tools-planning.md). This module does not run processes, observe files, read environment variables, resolve remote versions, write state or create approvals. The existing [reconciliation CLI/TUI](binary-reconciliation.md) displays these descriptions in computed rows; there is no executor accepting them. The separate [exact mise/npm capabilities](tool-installation.md) reconstructs independent opaque previews and does not dispatch these DTOs.

## Eligibility and authority

Only a `review_missing_package` row with reported absence, explicit missing binary observation, comparable policy and no blockers can receive a description. Existing matching package claims get `no_package_change`; unknown, unavailable, ambiguous, conflicting or dependency-blocked rows get no command. Existing/default/latest requests therefore do not become upgrade requests. Prerequisites needing their own review still block dependent command descriptions until fresh observations justify a new proposal.

Descriptions are serializable metadata, not deserializable approvals or executable capabilities. Each has `state: preview_only`, fixed executable/argument vectors, proposed cleared-environment controls, working directory `/`, unresolved/requested version selection, explicit effect warnings and `execution_authorized: false`. The selected inspection scope supplies the executable, HOME and **provider child search directories**; tool lookup directories are not substituted. No shell command string or clipboard handoff is generated.

Provider version/support, global configuration, mutable package metadata, global absence, side-effect boundaries and actual installability have **not** been independently established. A preview is not a tested invocation or a claim of safe execution. The separate exact mise/npm install workflows have their own fresh-validation and persistence contract; it never executes these descriptions. No new journal, receipt, lock, installation/recovery/cancellation or trust-persistence contract follows from these descriptions.

## Fixed descriptions

| Provider | Proposed arguments | Selection and limitations |
| --- | --- | --- |
| Homebrew | `install --formula PACKAGE` | Formula-only, native version unresolved. Proposed controls disable auto-update/analytics/API installation, root install-upgrade, installed-dependent checks and cleanup where supported. Native dependencies can still change or upgrade; no strict dependency-isolation guarantee. No unsupported ignore-dependencies shortcut is used. |
| mise | `install --jobs=1 TOOL[@REQUEST]` | Exact, prefix and explicit latest requests retain their spelling/semantics. A provider-default request stays a bare tool, delegating to global configuration, which can request multiple versions. `use`, activation and force/reinstall are not requested. Plugins/configuration can still execute code. |
| npm | Selected Node executes the explicitly bound npm CLI script with `install --global --prefix PREFIX --ignore-scripts --no-audit --no-fund --logs-max=0 --update-notifier=false PACKAGE[@REQUEST]` | Exact versions retain prerelease/build metadata. Explicit latest uses `@latest`; provider default remains unqualified, allowing the provider's configured default selection. Lifecycle scripts are disabled in the description, which may leave packages unusable; Node/npm/provider configuration is not an OS sandbox. |
| apt | Deferred: `privileged_handoff_not_designed` | A trusted dpkg-query executable is not authorization to infer apt-get/sudo. No privileged or alternate executable is invented. |

The environment description uses explicit HOME/XDG paths, provider child PATH, C locale and no-color controls, not inherited credentials/Node options. Network/downloads are disclosed as potential installation effects; query-only offline settings are deliberately not misrepresented as an installation guarantee. No remote version is fetched to complete this metadata.

All proposed installs disclose provider/package code, dependency changes (possibly upgrades), shared links/caches/data, partial failure and no automatic rollback. mise descriptions explicitly say activation is not requested, not that plugins cannot change configuration. Homebrew controls do not promise that no other installed tool changes; this unresolved native behavior must be addressed before an executor can satisfy Dort's no-automatic-upgrade contract, not silently treated as an exception. npm ignore-scripts is not proof of functional readiness or suppression of all trusted provider code.

## Bounds and presentation

Encoded command-preview metadata is limited to 65,536 bytes per description and 4 MiB across a plan. A description exceeding a limit is explicitly deferred as `description_limit`; package findings and the proposal remain inspectable, but no partial argv/control description is emitted. Budget order follows deterministic dependency-first plan order. These bounds cover command-preview metadata, not the other independently bounded observations.

CLI JSON exposes each row's `install_preview`; TUI full-row details show the same structured arguments, controls and warning with escaped metadata and viewport-only scrolling. No-op/blocked rows contain no command. `--yes` still approves only the command's explicitly requested inspections; it cannot execute an install description. Reconciliation preview mode still describes inspection/analysis scope without provider/binary lookup; install descriptions require an eligible computed row from supplied/approved observations.

Pure fixtures cover provider argv/control construction, default/latest/prefix/exact distinction, inactive/mismatched/unobserved/ambiguous refusal, npm binding/script controls, apt deferral and description budgets without files/processes/environment reads. CLI/TUI/PTY fake-provider fixtures inspect these fields and assert that `install` is never dispatched, even under explicit query `--yes`. These tests do not establish real-provider compatibility or safe installation. See [validation methods](validation.md).

Protocol references: [Homebrew manpage](https://docs.brew.sh/Manpage), [mise install](https://mise.jdx.dev/cli/install.html), [npm install](https://docs.npmjs.com/cli/v11/commands/npm-install/).
