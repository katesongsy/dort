# Pure tool package planning

`dort_core::tools::planning` classifies supplied metadata against manifest tool declarations. It performs no filesystem/environment discovery, provider execution, remote version lookup, clock access or writes. [Declaration reconciliation](binary-reconciliation.md) supplies explicit CLI/TUI inputs to this pure function after separately approved [provider inspections](tools-inspection.md) and metadata-only binary observation. The planner itself never invokes either observation service. Eligible missing-package rows include [non-executable command descriptions](install-command-previews.md), not a runner. This module exposes no package executor or installation approval API.

## Inputs and identity

The caller supplies a structurally valid manifest, explicit platform, resolved known profiles including `base`, and observations keyed by selected tool ID. Missing observation entries mean **not observed**, never absent. Observations for unknown/inactive tool IDs are rejected.

Each tool is explicitly bound to one chosen inspection report. The planner does not pick an executable, merge provider/home/prefix scopes or search another catalog when the selected one lacks a package. Provider/query kinds must match. npm requires the report's explicit runtime ID to match the declaration, with its selected Node matching the report executable. The scope retains query, executable, home and npm binding for inspection; this is not proof of freshness, authenticity, binary identity or ownership.

Binary observations are separate: not observed, unavailable, missing or an explicit list of absolute UTF-8 candidate paths. Candidates remain unattributed. No candidate is executed to obtain a version, and a name/path never establishes provider ownership or links that binary to a package claim. Empty candidate lists, relative/dot-component paths, non-UTF-8 identities and invalid catalog paths are rejected rather than normalized or rendered lossily.

Package comparison uses exact provider-reported identifiers. Potentially related Homebrew tap names block a missing-package proposal when there is no exact match. These checks do not resolve aliases or prove that all aliases have been discovered. No alias normalization establishes satisfaction. Only matching package claims are retained; unsafe version metadata is withheld and blocks analysis.

## Findings and proposals

| Supplied facts | Result |
| --- | --- |
| No selected catalog, failed query or wrong scope | Not observed, unavailable or scope mismatch; blocked, never a healthy empty catalog. |
| Exact package with a policy-matching reported installed version | Package `no_change`, including default/latest requests; no remote latest lookup or automatic upgrade. |
| Multiple installed versions | Distinct versions and individual comparisons remain inspectable; any known matching version can justify package no-change, not activation or binary readiness. |
| Present package with only mismatched/unverifiable versions | Blocked version conflict/unverifiable observation, not an implicit upgrade/reinstall. |
| No matching or potentially related package in a complete chosen catalog | Package absence in that reported scope, not global binary/package absence. |
| Reported package absence plus an explicit missing binary observation, comparable policy and unblocked prerequisites | `review_missing_package`: a planning proposal only. Only non-executable command metadata may follow; no argv execution, approval, privilege handling or writes are available. |
| Package absence with unknown/unavailable binary observation or any unattributed candidate | Blocked; unmanaged-looking paths are not adopted, relabeled as managed or assumed safe to overwrite. |

Requests exceeding the bounded observed-version representation block missing-package proposals. Policy comparison is shared with `tools::VersionPolicy`: provider-current default/latest for Homebrew, default/latest or exact semver for npm, including exact prerelease/build metadata.

A package no-op is **not overall tool readiness**. A tool can have a matching installed-package claim while its binary is unobserved/missing or its dependencies are blocked. The separate fields and blockers retain those facts. No receipt, ownership, active version or verified executable-version relationship is inferred.

## Dependency and scope handling

Selection uses the existing profile rules and reuses the manifest's pure dependency extraction/topological ordering, including npm runtime dependencies. Rows are dependency-first, deterministic and consistent with manifest ordering. Each tool has one platform-independent install declaration. Inactive dependencies are reported as blockers rather than inferred installs. No host-specific file target expansion/observation is performed.

A dependency with blockers blocks its dependents even if its own package action is no-change. A dependency needing package review also blocks its dependents: it needs separate review and a subsequent fresh observation before a dependent proposal can be made. This is not an automatically executable chain or a claim that package scripts are rollbackable.

An npm missing-package proposal additionally needs exactly one explicitly observed runtime binary candidate equal to the chosen Node path. This is a necessary scope check only, not provider ownership or executable-version verification. Unknown/missing/different/multiple candidates cannot silently establish the association.

## Bounds, validation and authority

Analysis accepts at most 4,096 selected tools, 32,768 installation records per chosen catalog, 1,024 paths per candidate observation and 65,536 copied version claims across the plan. Exceeding these bounds fails explicitly. These are analysis limits, not changes to manifest grammar or persistence formats.

Pure tests supply metadata directly without files, HOME changes or subprocesses. They cover unobserved/unavailable versus absent states, no-upgrade behavior, version conflicts/multiple versions, scope and identifier ambiguity, explicit npm binding, inactive dependencies, deterministic ordering, blocked-review propagation, unsafe metadata/path refusal and analysis bounds. They do not establish real-provider compatibility or binary identity. See [validation methods](validation.md).

Each row's `install_preview` either records no package change/blocking/deferral or describes fixed argv/control settings with unresolved/requested version semantics and explicit native-side-effect warnings. Existing, ambiguous and blocked tools get no command. No scope/report is promoted into an install capability. Plans are serializable metadata, not execution authorization; they cannot be deserialized into an approval and provide no execute method. They are not persisted as trust/receipts, and do not change any lifecycle journal, schema or cancellation contract. The separate [installation service](tool-installation.md) owns execution, freshness, approval and failure handling.
