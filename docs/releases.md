# Release mechanics

An approved version release includes GitHub Releases, Homebrew, and crates.io. Follow any channel limits in the request. Preparing a release does not authorize publication.

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

1. Push the version tag after CI passes on the exact commit. Verify the published GitHub release and all four archives against `SHA256SUMS`.
2. Update the [Homebrew tap](https://github.com/katesongsy/homebrew-tap) formula, receipts, and README with the release URLs, checksums, and commit. Run `ruby scripts/test-formula.rb`. Wait for all four tap CI installs to pass before updating its default branch.
3. From the clean tagged commit, run `cargo publish --locked -p dort-core --dry-run`, then rerun without `--dry-run`. Wait for that version to appear in the crates.io index. Repeat for `dort`.
4. Check that the public tap shows the new version. Install the exact version of `dort` from crates.io into a scratch directory and run `dort --version`.

## Changelog

Generate from committed history:

```sh
scripts/changelog.sh
scripts/changelog.sh --check
```

Use `chore(changelog): regenerate history` for a generated-only commit. Uncommitted work is excluded.
