# Release mechanics

[Documentation index](README.md) · [Validation](validation.md) · [User installation](user/getting-started.md#installation)

## Scope and authorization

Releases are GitHub source releases plus native binary archives, not crates.io publication. Both workspace crates retain `publish = false`. Version, tag and release publication require explicit operator approval. A release does not remove Dort's experimental status or its documented safety limitations. Maintainer and unfamiliar-user acceptance remain distinct from automated validation.

## Native build artifacts

The CI workflow runs the full required checks on Ubuntu 24.04 x86_64/ARM64 and macOS 15 Intel/Apple Silicon. For stable workspace versions, each runtime job also builds an optimized, locked native binary with the pinned Rust toolchain and retains an immutable Actions artifact for 14 days. Development/prerelease versions skip archive production, not the ordinary checks.

`scripts/release.py --package --tag vMAJOR.MINOR.PATCH --target TARGET` consumes a fresh Cargo JSON executable artifact. It requires a matching stable workspace version, native runtime/target and Rust 1.99.1. macOS builds explicitly use deployment target 15.0; Linux builds use the reviewed glibc 2.39 baseline. The binary's version is checked in a disposable child HOME/config/state and must create no state. No real-home application smoke test is performed.

Archive names are `dort-vMAJOR.MINOR.PATCH-TARGET.tar.gz`, with targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Each archive contains only executable `dort`, the MIT `LICENSE` and `BUILD-INFO.json` with version, commit, target, compiler and minimum runtime. Tar owner/group/timestamps and gzip timestamp are normalized; this does not promise reproducible compiler output. Archives have individual checksum files in CI; publication combines them into `SHA256SUMS`. Checksums/provenance are integrity and consistency checks, not independent publisher signatures, notarization or sandboxing.

## Tag-triggered publication

The release workflow runs only on pushed `v*` tags. It validates the exact stable tag/version and requires a successful push CI run for that exact commit. It downloads only that run's four immutable native artifacts, then verifies the complete asset set, hashes, regular archive entries/permissions, binary architecture, license and embedded version/commit/toolchain. Missing/expired artifacts or mismatches fail closed; rerun CI for the same commit rather than substituting another revision's files.

Only the publication job has repository-content write permission; it also has read access to CI artifacts. Credentials come from the job-scoped GitHub token and are never placed in archives or notes. After verification, GitHub CLI creates a draft containing all four archives, `SHA256SUMS` and user-facing notes, then publishes it as the latest release. It verifies the tag already exists remotely; it never invents a tag or silently overwrites an existing release. Failed drafts require explicit operator inspection, not automatic replacement/deletion. Per-tag concurrency prevents simultaneous publication jobs.

Before pushing an authorized annotated tag, validate the final candidate locally and observe successful four-platform CI for the exact commit. Source archives are GitHub's tag snapshots. Never move a published release tag to hide a failed candidate or overwrite published assets.

## Generated history

`scripts/changelog.sh` uses pinned git-cliff 2.14.2 against committed history only. `--tag vMAJOR.MINOR.PATCH` labels the latest history for release preparation without creating, moving or pushing any Git tag. Combine it with `--check` to validate a prepared release before its tag exists. After the real tag is present, ordinary generation/checking must produce the same history. Generated-only and release-maintenance commits are excluded to avoid self-referential churn. Keep `CHANGLOG.md` generated, not hand-edited.

## Local guard tests

```sh
python3 scripts/release.py --self-test
```

The standard-library tests construct only disposable synthetic archives. They cover complete-set verification, missing platforms, corruption, commit mismatch, invalid tags and binary-format rejection; they do not execute synthetic binaries, publish releases or establish native runtime correctness. Native version smoke checks and the full platform CI provide separate observations.
