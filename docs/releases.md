# Release mechanics

[Documentation index](README.md) · [Validation](validation.md) · [User installation](user/getting-started.md#installation)

## Scope and authorization

GitHub releases provide tagged source snapshots and native binary archives. Cargo source packaging is a separate distribution path: both workspace crates allow only the `crates-io` registry, but packaging/validation never uploads them. Version, tag, GitHub release and irreversible registry publication require explicit operator approval. A release does not remove Dort's experimental status or its documented safety limitations. Maintainer and unfamiliar-user acceptance remain distinct from automated validation.

## Native build artifacts

The CI workflow runs the full required checks on Ubuntu 24.04 x86_64/ARM64 and macOS 15 Intel/Apple Silicon. For stable workspace versions, each runtime job also builds an optimized, locked native binary with the pinned Rust toolchain and retains an immutable Actions artifact for 14 days. Development/prerelease versions skip archive production, not the ordinary checks.

`scripts/release.py --package --tag vMAJOR.MINOR.PATCH --target TARGET` consumes a fresh Cargo JSON executable artifact. It requires a matching stable workspace version, native runtime/target and Rust 1.99.1. macOS builds explicitly use deployment target 15.0; Linux builds use the reviewed glibc 2.39 baseline. The binary's version is checked in a disposable child HOME/config/state and must create no state. No real-home application smoke test is performed.

Archive names are `dort-vMAJOR.MINOR.PATCH-TARGET.tar.gz`, with targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Each archive contains executable `dort`, the MIT `LICENSE`, `BUILD-INFO.json` with version/commit/target/compiler/minimum runtime, `THIRD-PARTY-NOTICES.txt` and `RUST-COPYRIGHT.html`. Dependency notices collect declared license expressions and upstream license/notice files from the target-resolved normal/build dependency closure; missing notices fail packaging. The pinned toolchain supplies its standard-library copyright bundle. Inclusion does not relicense Dort or assert every listed component is linked. Tar owner/group/timestamps and gzip timestamp are normalized; this does not promise reproducible compiler output. Archives have individual checksum files in CI; publication combines them into `SHA256SUMS`. Checksums/provenance are integrity and consistency checks, not independent publisher signatures, notarization or sandboxing.

## Tag-triggered publication

The release workflow runs only on pushed `v*` tags. It validates the exact stable tag/version and requires a successful push CI run for that exact commit. It downloads only that run's four immutable native artifacts, then verifies the complete asset set, hashes, regular archive entries/permissions, binary architecture, license and embedded version/commit/toolchain. Missing/expired artifacts or mismatches fail closed; rerun CI for the same commit rather than substituting another revision's files.

Only the publication job has repository-content write permission; it also has read access to CI artifacts. Credentials come from the job-scoped GitHub token and are never placed in archives or notes. After verification, GitHub CLI creates a draft containing all four archives, `SHA256SUMS` and user-facing notes, then publishes it as the latest release. It verifies the tag already exists remotely; it never invents a tag or silently overwrites an existing release. Failed drafts require explicit operator inspection, not automatic replacement/deletion. Per-tag concurrency prevents simultaneous publication jobs.

Before pushing an authorized annotated tag, validate the final candidate locally and observe successful four-platform CI for the exact commit. Source archives are GitHub's tag snapshots. Never move a published release tag to hide a failed candidate or overwrite published assets.

## Cargo source packages

`cargo package --workspace --locked` assembles and builds both source archives, including a temporary local registry for the versioned `dort-core` dependency before initial publication. `cargo publish --workspace --dry-run --locked --registry crates-io` additionally exercises publication checks without uploading. Do not substitute `--no-verify` or a path-only dependency. `scripts/check-crates.py` validates metadata, package-local license/example copies and literal Rust include resources; `--archives` also checks the complete file set, source bytes and clean current-commit provenance. Its `--self-test` is mount-free and does not contact a registry.

Each package includes its own README, MIT license, source/tests and example manifest. CI checks the source archives on all four supported runtime platforms; no workflow automatically publishes crates or receives registry credentials. A separately approved upload publishes `dort-core` before `dort` and waits for registry availability. The publishing account needs a verified crates.io email address. Use a short-lived token with `publish-new` for initial crate creation, or `publish-update` for later versions, restricted to the relevant crate names where possible; yank and ownership permissions are not required. Never print tokens or place them in source, logs, command-line arguments, archives or release notes; configure Cargo credentials through an approved local credential provider, for example the interactive `cargo login --registry crates-io` prompt.

The initial [`dort`](https://crates.io/crates/dort/0.1.0) and [`dort-core`](https://crates.io/crates/dort-core/0.1.0) Cargo 0.1.0 packages originate from later packaging-only commits than the already published GitHub `v0.1.0` tag. Their `.cargo_vcs_info.json` records the source commit; keep the existing tag and assets unchanged rather than implying the snapshots are identical. Package verification is not publication, and registry package versions cannot be overwritten.

## Generated history

`scripts/changelog.sh` uses pinned git-cliff 2.14.2 against committed history only. `--tag vMAJOR.MINOR.PATCH` labels the latest history for release preparation without creating, moving or pushing any Git tag. Combine it with `--check` to validate a prepared release before its tag exists. After the real tag is present, ordinary generation/checking must produce the same history. Generated-only and release-maintenance commits are excluded to avoid self-referential churn. Keep `CHANGLOG.md` generated, not hand-edited.

## Local guard tests

```sh
python3 scripts/release.py --self-test
```

The standard-library tests construct only disposable synthetic archives. They cover complete-set verification, missing platforms/notices, corruption, commit mismatch, invalid tags and binary-format rejection; they do not execute synthetic binaries, publish releases or establish native runtime correctness. Native version smoke checks and the full platform CI provide separate observations.
