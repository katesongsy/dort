# Core provider inspection

`dort_core::tools::inspection` implements deliberate local provider queries. It is not called by existing CLI/TUI workflows. Frontend tool inventory and package installation are not exposed. Pure version policies in `tools.rs`, manifest checks, filesystem observation and planners do not execute providers.

## Authority and trust

The caller supplies a trusted absolute executable, absolute home and nonempty absolute executable-search directories. Paths must be UTF-8 and contain no NUL or dot components; an empty/relative PATH entry is rejected. The core never searches PATH or executes candidates to identify them. Queries run at `/`, not in a repository. Supplied paths remain trust decisions, not immutable executable identities or isolation against same-user replacement/configuration races.

Constructing `Preview` and serializing its `Description` are process-free. The description exposes the executable, fixed arguments, home, search directories, runtime binding, limits and risk warning. Only consuming that live preview into an opaque, single-use `Approval` permits inspection. Previews/approvals cannot be deserialized; approval is not installation authority. Presentation is responsible for deliberate confirmation of the exact preview; the core cannot establish human consent.

Provider queries are **not guaranteed side-effect-free**. Trusted global configuration/plugins can execute code and provider caches/logs can change. Offline/no-update settings are hardening, not an OS sandbox or a guarantee that a malicious provider/plugin cannot use the network. No installation, privileged handoff, trust persistence, receipts or recovery-journal changes are implemented here.

## Fixed protocols

| Query | Executed operation | Retained metadata |
| --- | --- | --- |
| Homebrew formulae | `brew info --json=v2 --installed --formula` | Formula `full_name` and each installed version; no casks or inferred binary ownership. |
| Debian packages | `dpkg-query --no-pager --show` with a fixed tab-delimited format for `binary:Package`, `Version`, `db:Status-Status`, `db:Status-Eflag` | Fully installed, error-free packages; architecture qualifiers and opaque Debian versions retained. Removed/config-only entries are not installs. Incomplete/repair-required entries make the observation unavailable rather than a healthy empty catalog. |
| mise installations | `mise ls --installed --json` | Provider-reported keys and installed versions, including inactive versions. Keys do not independently establish a backend or active executable identity. Missing/broken/lazy entries contradicting this protocol are rejected. |
| npm globals | Explicit Node executes an explicit npm CLI script: `ls --global --json --depth=0 --offline --ignore-scripts --logs-max=0 --update-notifier=false --audit=false --fund=false --prefix PREFIX` | Top-level package identifiers/versions only; no resolved URLs, transitive trees, scripts or diagnostic strings. Incomplete/error responses are unavailable. |

npm requires a manifest-declared runtime tool ID, trusted absolute Node and npm CLI paths, and an explicit global prefix. The npm launcher/shebang is never used, so a different Node is not silently selected through PATH. The binding is retained in the description/report. It records the caller's selection, not proof that a declared runtime owns that executable or prefix, or that Dort installed anything. Generic construction of an unbound npm query is rejected.

## Child lifetime and privacy

Children receive a cleared environment, explicit HOME/XDG directories and PATH, C locale and no-color setting. No parent credentials, Node options, shell startup variables or arbitrary environment overrides are forwarded. Homebrew disables auto-update/analytics/environment hints and API installation; mise disables auto-install and requests offline mode. These controls do not neutralize trusted provider configuration/plugins.

The direct child runs in a separate process group with null stdin. Both pipes drain concurrently using nonblocking reads. Combined stdout/stderr is bounded to 4 MiB and the operation to ten seconds, including descendants retaining pipes. Remaining group members are killed on completion/failure and the direct child is waited for. There is no detached background inspection job.

Raw stdout is private to execution/parsing; stderr is counted and discarded. Neither is exposed through Debug, serialization, logs or errors. Parsers retain only bounded ASCII metadata identifiers/versions, sort/deduplicate exact repeats, preserve distinct versions and reject malformed/unsupported protocols. A failed observation is a typed `Unavailable`, never `Complete` with an empty list. Provider claims are neither installation receipts nor evidence that a PATH binary satisfies a declaration. Version-policy matching alone grants no authority.

## Validation

Core unit tests exercise protocol whitelisting, empty versus malformed/unavailable results, multiple versions, incomplete package states, limits, concurrent pipe drains and descendant-held-pipe deadlines. Integration tests use canonical disposable homes and deterministic fake providers, covering inert previews, fixed arguments, sanitized environments, explicit npm bindings, path rejection and raw-output redaction. They require no installed package manager or Node. These fixtures establish adapter behavior, not real-provider compatibility or sandboxing. See [validation methods](validation.md) for ordinary gates and fixture boundaries.

Protocol references: [Homebrew](https://docs.brew.sh/Manpage), [dpkg-query](https://manpages.ubuntu.com/manpages/noble/en/man1/dpkg-query.1.html), [mise ls](https://mise.jdx.dev/cli/ls.html), [mise settings](https://mise.jdx.dev/configuration/settings.html), [npm ls](https://docs.npmjs.com/cli/v11/commands/npm-ls/).
