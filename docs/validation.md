# Validation

## Commands

From the repository root, with Rust 1.99.1 and Python 3:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/test.py full
cargo build --workspace --locked
python3 scripts/check-docs.py --self-test
python3 scripts/check-docs.py
python3 scripts/check-tui-style.py --self-test
python3 scripts/check-tui-style.py
python3 scripts/test-terminal.py
python3 scripts/test-cross-device-macos.py --self-test
python3 scripts/release.py --self-test
python3 scripts/check-crates.py --self-test
python3 scripts/check-crates.py
cargo package --workspace --locked
python3 scripts/check-crates.py --archives
git diff --check
```

`python3 scripts/test.py` runs focused unit checks. Choose `npm`, `tools` or `ui`; use `--list` to see which tests each suite selects. Every focused suite runs each test in its own process, up to `--jobs` at a time, each with a private temporary directory. `full` builds once and runs every test the same way, up to 8 at a time. Use `--jobs` and `--timeout` to tune either.

Direct `cargo test` remains valid for debugging. Keep `--test-threads=1`: same-process tests can briefly share a fork-inherited file lock, and two tests in one binary interfere. Two at once is enough to lose a test on some runs. `scripts/test.py` avoids this by giving every test its own process.

## Fixtures

- Tests use disposable canonical homes and repositories.
- Package-manager fixtures are fake. Real Homebrew, npm or system package-manager installs are not run.
- Child environments are isolated. Tests do not change the process HOME/XDG.
- Cross-device and storage-fault checks are opt-in and need separate infrastructure.
- Permission-denial tests cannot prove refusal when run as root.

## Limits

Automated checks do not establish human usability, visual approval, real package-manager compatibility or power-loss safety. A local pass is not evidence for an earlier or remote CI run. Optional skips are visible in test output.
