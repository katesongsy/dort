#!/bin/sh
# Reproducible, local-history-only changelog generation; no auto-install or Git writes.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
version=2.14.2

check=false
tag=
usage() { printf '%s\n' "Usage: $0 [--check] [--tag vMAJOR.MINOR.PATCH]" >&2; exit 2; }
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) [ "$check" = false ] || usage; check=true; shift ;;
        --tag)
            [ -z "$tag" ] && [ "$#" -ge 2 ] || usage
            tag=$2
            case "$tag" in *[!0-9v.]*) usage ;; esac
            printf '%s\n' "$tag" | LC_ALL=C grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || usage
            shift 2 ;;
        *) usage ;;
    esac
done

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
set --
if [ -n "$tag" ]; then
    # Label committed history for a planned release without creating/moving a Git tag.
    set -- --tag "$tag"
fi
"$binary" --config "$root/cliff.toml" --repository "$root" --workdir "$root" \
    --offline --no-exec "$@" --output "$temporary"

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
