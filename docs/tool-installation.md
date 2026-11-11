# Bounded Tools installation and attempt metadata

`tools::install` is a separate opaque core capability, not an executor for `tools::install_preview` descriptions. The shared `services::installation` adapter reconstructs requests from explicit reconciliation bindings and the selected session snapshots. The CLI opts in through `tools reconcile --install-tool`; the TUI offers a dedicated review on eligible rows. A separate Homebrew capability installs a missing canonical core formula at provider-current default/latest policy on macOS. Apt/privileged writes remain non-executable. See the [mise](user/commands.md#exact-mise-installation), [npm](user/commands.md#exact-npm-installation) and [Homebrew](user/commands.md#homebrew-core-installation) workflows.

## Scope and approval

One observed missing mise/npm exact semver package or supported canonical core formula, an independently missing declared binary and clear prerequisites can be reviewed. The repository must be registered. Profiles include persistent selections and explicit additions; the adapter refuses a changed reconciliation profile scope instead of silently expanding it. Mise keeps its same-snapshot binding rule. npm and Homebrew use explicit target/prerequisite bindings across independently selected catalogs (npm additionally requires its selected runtime); necessary prerequisite closure is taken from declared dependencies, never inferred from ownership or automatically bound. Catalogs are not merged. Existing, conflicting, ambiguous, unobserved, unmanaged or blocked states do not authorize installation.

Preparation is process-free. It binds the bounded stable manifest, registration/profile configuration, host context, explicit regular executable and separate no-follow declared-name lookup directories. No candidate is executed. Public review metadata includes the selected package/version, repository/profiles, executable, provider child PATH, independent binary lookup scope, fixed arguments and deadline. Only consuming the real in-memory preview creates approval. Neither a stored attempt nor JSON metadata can recreate it.

One frontend confirmation covers fresh inspection, conditional installation and post-inspection. Execution holds the application writer lock, checks existing file-journal write guards and unresolved Tools attempts before querying, independently revalidates all bindings, and rebuilds eligibility from fresh fixed provider queries and binary metadata. npm and Homebrew revalidate program/source/prefix identities before and after each required query; read queries may be brew, dpkg, mise or npm, never another provider's installer. A newly satisfied package claim is a no-op with no installer or attempt; other conflicts or unavailable observations refuse. Intent/head/source guards are checked again immediately before spawn. Historical session snapshots are never overwritten; installation invalidates cached reconciliation and marks retained snapshot IDs stale for later TUI install reviews. Metadata comparisons remain available, but recomputing with an old snapshot cannot re-enable installation: the view offers Collect fresh inventory, and the new snapshot must be explicitly bound. CLI install requests always collect new approved inventories. No automatic replacement or rebinding occurs.

## Fixed process and worker contract

The mise runner constructs only:

```text
mise --no-config --no-env --no-hooks install --jobs=1 PACKAGE@EXACT
```

It runs at `/` with null stdin, cleared environment, explicit HOME/XDG/PATH, fixed locale and mise controls. No credentials, terminal, arbitrary argv, `force`, activation, upgrade or uninstall operation is inherited or dispatched. Unsupported flags fail without a weaker fallback. Providers/backends/package code and downloads can still have effects; this is hardening, not an OS sandbox, atomic transaction, ownership proof or rollback mechanism. An inactive installed mise version is normal and is not a PATH/readiness claim.

### npm runtime, prefix and fixed protocol

npm requires the declared runtime ID and the exact physical Node/npm CLI/global-prefix binding from a matching unchanged session manifest observation. The pure planner must still find the bound Node as the sole runtime candidate, with a satisfied runtime package claim and clear prerequisite rows. Each necessary explicit catalog is freshly queried under the writer lock. Neither closure nor inspection manufactures runtime satisfaction. Preparation does not query providers or execute Node/npm version/config commands.

The global prefix must be a physical owned directory strictly inside HOME, or absent descendants of safe owned parents there; its absolute ancestor chain is bounded to 32 entries. Required parents (prefix, `lib/node_modules`, any selected scoped-package parent, `bin`, `share/man` and ancestors) must be directories without group/world write access. Source snapshots bind their identities and the trusted programs plus independent no-follow tool-search directories. Node and npm CLI cannot lie in the selected prefix's native write roots. Approved missing parents are created descriptor-relative and synced only after durable intent; new identities are pinned before spawn. The initial scope/source budget applies to the durable intent; expanded in-memory fingerprints after approved parent creation do not re-encode or enlarge that immutable record. Native npm writes are trusted-program hardening, not isolation against a malicious same-user path race.

Node executes the selected CLI directly, never the npm wrapper:

```text
NODE NPM_CLI install --global --prefix PREFIX --ignore-scripts \
  --audit=false --fund=false --update-notifier=false --logs-max=0 \
  --loglevel=silent --registry=https://registry.npmjs.org/ \
  --userconfig=/dev/null --globalconfig=/dev/null PACKAGE@EXACT
```

The same cleared HOME/XDG/PATH, fixed locale, `/` and null-stdin boundary applies, without mise-specific environment controls. No inherited terminal/credential environment, arbitrary argv, private auth or weaker fallback is provided. Trusted builtin npm configuration and dependency behavior remain in scope: a fixed registry is not strict endpoint isolation or a promise that npm cannot read HOME.

Native dependency-tree writes and bin/man links are approved effects. They can expose commands immediately if prefix/bin is on PATH; npm is **not activation-free**. Dort does not edit PATH or runtime configuration. Disabling lifecycle scripts does not establish package usability; effects are not atomic and other prefix entries are not isolated. There is no intentional other-top-level upgrade, force, uninstall or rollback request.

### Homebrew core formula protocol

`tools::install::brew` supports only unqualified canonical core formula names on macOS, with omitted or `latest` version policy. Versioned names are identities, not exact patch pins. Qualified tap names retain the pure planner's identity rules and are not executable by this slice. Casks, aliases/renames, disabled/unknown formulae, HEAD, arbitrary options/URLs/files, services and privileged layouts refuse.

Preparation is process-free. An inspectable prefix hint derives only from the selected physical `REPOSITORY/bin/brew` path; supported repository layouts are `PREFIX` or `PREFIX/Homebrew`, with `PREFIX/Cellar`. Advanced can replace the expected prefix, never a fresh observed mismatch. Native write roots must already exist, be physical, caller-owned and owner-writable without group/world write access: Cellar, bin, etc, include, lib, opt, sbin, share, var, var/homebrew, and repository/Library. A safe root-owned prefix such as `/usr/local` may contain these owned roots; no group permission inference, directory creation or sudo fallback is used. Parent identities are bound; schema-3 metadata deduplicates ancestor chains without discarding identities. This does not isolate a trusted native program's writes or nested prefix entries against same-user races.

One approval covers fixed bounded `brew help install`, `--prefix`, `--repository`, `--cellar`, `info --json=v2 --formula homebrew/core/FORMULA`, necessary installed catalogs and post-observation. Normal inspection collection does not gain these commands. Help must advertise `--formula`, `--no-ask`, `--skip-post-install` and the install-upgrade/dependent/cleanup guards. Layout probes must return one matching absolute path; only the explicitly expected physical paths are observed/revalidated. Unmatched provider path strings are never passed to filesystem observation or copied into errors. Target JSON is whitelisted to canonical name/full-name, core tap, availability and version/install metadata; aliases/renames and contradictory catalog claims refuse. Help/JSON bodies and descriptions never enter reports or attempts. Advertised controls are hardening, not an installed-provider compatibility or malicious-program guarantee.

```text
brew install --formula --no-ask --skip-post-install homebrew/core/FORMULA
```

The cleared HOME/XDG/PATH, locale, `/` and null-stdin contract applies with `HOMEBREW_NO_AUTO_UPDATE`, `HOMEBREW_NO_ANALYTICS`, `HOMEBREW_NO_ENV_HINTS`, `HOMEBREW_NO_INSTALL_UPGRADE`, `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK` and `HOMEBREW_NO_INSTALL_CLEANUP`. No arbitrary environment or weaker fallback is supplied. Formula/build/resource/dependency code **can execute**, dependencies can install/upgrade, and native links can expose commands on PATH. Skip-post-install is not a ban on build code; this is not bottle-only, exact-pinned, atomic, isolated or a usability guarantee. Trusted Homebrew can download and maintain native metadata/caches; no strict endpoint or HOME-access isolation is promised. Dort does not request link/services/cleanup/upgrade/reinstall/pin/tap, manager installation/update or PATH/runtime edits.

### Shared worker and observation

The installer has a displayed ten-minute deadline; inventory queries retain their independent ten-second deadline and bounded parser inputs. Concurrent nonblocking fair stdout/stderr drains discard installer bytes, including successful output; only typed phases and outcomes leave the runner. Timeout/pipe failure terminates the owned process group and joins/reaps it, possibly after partial effects. The existing single owned joined worker and single input thread remain unchanged. Quit/Ctrl-C waits for approved work; no cooperative cancellation, detached workers or automatic rollback is provided. Progress uses separate typed install events, not fabricated file-journal checkpoints.

Post-inspection is attempted after process failure as well as success. A changed source/executable prevents re-executing the changed provider. Process outcome, package claim and binary metadata availability remain separate. Homebrew uses `provider_current_present`, never `exact_present`, with whitelisted observed versions; this is not a readiness claim. Output persistence failure is an unresolved attempt, never implied success. Reports do not claim binary readiness.

## Separate durable attempt store

`STATE/tools-attempts/<64-hex-id>/` is independent of file transactions and Recovery. Descriptor-relative no-follow private helpers create and sync immutable checksummed records:

- `intent.json`: schema 1 for mise, schema 2 for npm, schema 3 for Homebrew, attempt ID, requested scope and private source fingerprints/hashes; durable before installer/prefix mutation. npm adds runtime/CLI/prefix and catalog/parent/program bindings. Homebrew adds provider-current policy, native layout, advertised probe facts and deduplicated parent/program identities.
- `outcome.json`: observed typed process/package/binary outcome bound to the exact intent artifact.
- `close-NNNN.json`: explicit fresh-inspection closure bound to the unchanged census/head of all earlier artifacts; append-only, never repair/overwrite an earlier record.

Envelopes validate the checksum of the exact raw payload string before version-aware typed parsing. Mise continues to write/read schema-1 fields with absent npm extensions; old artifacts are never re-encoded or migrated. Outcomes bind the full exact intent artifact; closures bind the census/head including earlier artifact bytes. npm outcomes/closures use schema 2, Homebrew uses schema 3, and legacy mise retains schema 1. Schema-1/2 outcomes never accept Homebrew's non-exact presence state.

Records contain metadata, not raw output, manifest/configuration bodies, variable values, credentials, backups or persisted trust. There is no file-journal format change. Bounds are 256 attempts, 32 files per attempt and 64 KiB per record; exhausted or unsafe storage is refused, never silently evicted or followed through symlinks. File and directory sync establish the ordinary durability protocol; process-exit tests do not prove power-loss safety.

Missing/malformed intent, incomplete outcome, changed artifacts or invalid closure leave an unresolved attempt. Only subsequent Tools installations are blocked; unrelated file workflows are not blocked by this store. Startup never replays, cleans up, resumes or rolls back an installation.

## Explicit inspection and closure

Both frontends list all-repository attempt metadata, distinguish unavailable storage from an empty list, expose full IDs/scope/outcomes, and offer a process-free closure preview. A valid original scope requires its original executable/home/PATH; damaged or missing scope requires an explicit provider and trusted executable/child PATH, with Node/npm CLI/prefix for npm or the original prefix for Homebrew. Fallback runtime labels describe historical probes only, not declarations or satisfaction. Closure is historical metadata handling, not a current manifest operation.

One deliberate activation acquires the writer lock, checks unchanged head/host/executable bindings, runs a fresh fixed query for the historical provider scope, requires a complete observation and appends a head-bound closure. Unavailable queries do not close anything. A closure never retries an installer, repairs packages, verifies readiness or asserts that a damaged record was successful. Editing an earlier artifact invalidates the closure and makes the attempt unresolved again. Retry always prepares a fresh preview; no approval is replayed. File-journal problems do not prevent this separate metadata-only closure workflow. Current manifest availability or unchanged declarations are not required for historical closure; trusted npm CLI identity is checked as well as Node. Homebrew closure also brackets a fixed `--prefix` query with prefix/program identity checks and requires the original prefix before its complete installed catalog; no current declaration or runtime proof is fabricated.

## Validation boundaries

Disposable fake-provider fixtures cover fixed arguments/environment, output discard, source/profile/registration/executable drift, scope and prerequisite refusal, intent/outcome failure boundaries, independent file guards, unresolved-only Tools blocking, malformed/changed attempt closure, process exits, deadlines and descendant-held pipes. CLI, typed TUI/cell/caret and PTY checks exercise one-activation review, progress, joined exit, compact layouts and explicit closure. No workstation provider install, network compatibility, unfamiliar-user usability or power-loss guarantee follows from these checks. See [validation methods](validation.md).
