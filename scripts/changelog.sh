#!/bin/sh
# Reproducible, local-history-only changelog generation; no auto-install or Git writes.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
version=2.14.2

case "${1:-}" in
    "") check=false ;;
    --check) check=true ;;
    *) printf '%s\n' "Usage: $0 [--check]" >&2; exit 2 ;;
esac
if [ "$#" -gt 1 ]; then
    printf '%s\n' "Usage: $0 [--check]" >&2
    exit 2
fi

binary="$root/temp/tooling/git-cliff/bin/git-cliff"
if [ ! -x "$binary" ]; then
    binary=$(command -v git-cliff || true)
fi
if [ -z "$binary" ]; then
    printf '%s\n' "git-cliff $version is required. From the repository root, run:" >&2
    printf '%s\n' "cargo install git-cliff --version $version --locked --root temp/tooling/git-cliff" >&2
    exit 1
fi
actual=$("$binary" --version)
if [ "$actual" != "git-cliff $version" ]; then
    printf '%s\n' "Expected git-cliff $version; found $actual" >&2
    exit 1
fi

mkdir -p "$root/temp/tooling"
temporary=$(mktemp "$root/temp/tooling/changelog.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
"$binary" --config "$root/cliff.toml" --repository "$root" --workdir "$root" \
    --offline --no-exec --output "$temporary"

if [ "$check" = true ]; then
    if ! cmp -s "$temporary" "$root/CHANGLOG.md"; then
        printf '%s\n' "CHANGLOG.md differs from committed history; run scripts/changelog.sh." >&2
        exit 1
    fi
    printf '%s\n' "CHANGLOG.md is up to date."
elif ! cmp -s "$temporary" "$root/CHANGLOG.md"; then
    chmod 644 "$temporary"
    mv "$temporary" "$root/CHANGLOG.md"
    printf '%s\n' "Updated CHANGLOG.md from committed history."
else
    printf '%s\n' "CHANGLOG.md is already up to date."
fi
