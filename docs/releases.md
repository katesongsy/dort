# Release mechanics

Releases are manual and explicit. Commit, tag, push, publish or upload only when asked. Registry publication needs separate approval.

## Requirements

- Rust **1.99.1**, edition 2024 and committed `Cargo.lock`.
- Clean working tree and full Git history/tags.
- Pinned `git-cliff` **2.14.2** for changelog generation.
- Package licenses and example manifest must match the repository copies exactly.

## Checks

Run the ordinary [validation](validation.md) checks. Release scripts also have self-tests:

```sh
python3 scripts/release.py --self-test
python3 scripts/check-crates.py --self-test
python3 scripts/check-crates.py
```

Packaging refuses a dirty tree. Do not bypass that with `--allow-dirty`.

## Artifacts

Native archives cover macOS 15+ and Ubuntu 24.04+ (glibc 2.39+) on x86_64 and ARM64. Each archive carries the executable, MIT license, dependency and Rust notices, build info and a SHA-256 checksum.

Tag-triggered publication uses only successful exact-commit CI artifacts. It verifies archives, checksums and build origin before publishing a draft.

## Distribution

- Homebrew: the separate [tap](https://github.com/katesongsy/homebrew-tap) installs release archives and keeps its own validation.
- Cargo: `dort` and `dort-core` are published separately after approval.

These channels own packaging. They do not change Dort behavior or supported hosts.

## Changelog

Generate from committed history:

```sh
scripts/changelog.sh
scripts/changelog.sh --check
```

Use `chore(changelog): regenerate history` for a generated-only commit. Uncommitted work is excluded.
