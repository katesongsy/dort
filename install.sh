#!/usr/bin/env bash
# Install the dort CLI from GitHub Releases on macOS, Linux or WSL.
set -euo pipefail

VERSION="1.0.0"
REPOSITORY="katesongsy/dort"
BREW_INSTALLER="https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"
NVM_VERSION="v0.40.0"
NVM_INSTALLER="https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh"
SUPPORTED_TARGETS="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu"

# name|probe executables|what it is for. Probed in this order, installed in this order.
PREREQUISITES="brew|brew|Homebrew installs Git and other development tools
git|git|Git commands and repository history
node|node npm|Node.js and npm for npm tool checks"

RELEASE_TAG="latest"
INSTALL_DIR_ARG=""
INSTALL_DIR=""
ASSUME_YES=0
SKIP_DEPS=0
TARGET=""
HOST_SYSTEM=""
FETCH=""
DIGEST_TOOL=""
WORK=""
STAGED=""
MISSING_COUNT=0
MISSING_NAMES=()
MISSING_REASONS=()

usage() {
  cat <<'TEXT'
Install the dort CLI from GitHub Releases.

Usage: install.sh [options]

Options:
  --tag TAG           Install release TAG, for example v0.1.0. Default: the latest release.
  --install-dir DIR   Install dort into DIR. Default: /usr/local/bin when writable,
                      otherwise $HOME/.local/bin.
  --yes               Accept every prerequisite install without prompting.
  --skip-deps         Leave Homebrew, Git and Node.js alone.
  --version           Print the installer version.
  --help              Print this message.

Environment:
  DORT_INSTALL_DIR    Same as --install-dir.
  DORT_TARGET         Host target, when detection is wrong.

The script reads confirmations from the terminal, so `curl ... | bash` works.
TEXT
}

cleanup() {
  if [ -n "$STAGED" ]; then
    rm -f "$STAGED"
  fi
  if [ -n "$WORK" ]; then
    rm -rf "$WORK"
  fi
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

reject_option() {
  printf 'error: %s\n' "$*" >&2
  printf 'Run this script with --help for the option list.\n' >&2
  exit 2
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --tag)
        [ "$#" -ge 2 ] && [ -n "$2" ] || reject_option "--tag needs a value"
        RELEASE_TAG="$2"
        shift 2
        ;;
      --install-dir)
        [ "$#" -ge 2 ] && [ -n "$2" ] || reject_option "--install-dir needs a value"
        INSTALL_DIR_ARG="$2"
        shift 2
        ;;
      --yes)
        ASSUME_YES=1
        shift
        ;;
      --skip-deps)
        SKIP_DEPS=1
        shift
        ;;
      --version)
        printf '%s\n' "$VERSION"
        exit 0
        ;;
      --help)
        usage
        exit 0
        ;;
      *)
        reject_option "unknown option: $1"
        ;;
    esac
  done
  [ -n "$RELEASE_TAG" ] || reject_option "--tag needs a value"
  case "$RELEASE_TAG" in
    *[!A-Za-z0-9._-]*) reject_option "invalid tag: $RELEASE_TAG" ;;
  esac
}

detect_host() {
  local machine arch
  HOST_SYSTEM="$(uname -s)"
  machine="$(uname -m)"
  if [ -n "${DORT_TARGET:-}" ]; then
    TARGET="$DORT_TARGET"
  else
    case "$machine" in
      arm64|aarch64) arch=aarch64 ;;
      x86_64|amd64|x86-64) arch=x86_64 ;;
      *) arch=unknown ;;
    esac
    case "$HOST_SYSTEM:$arch" in
      Darwin:aarch64) TARGET=aarch64-apple-darwin ;;
      Darwin:x86_64) TARGET=x86_64-apple-darwin ;;
      Linux:aarch64) TARGET=aarch64-unknown-linux-gnu ;;
      Linux:x86_64) TARGET=x86_64-unknown-linux-gnu ;;
      *)
        die "unsupported host $HOST_SYSTEM/$machine reported by uname. Supported targets: $SUPPORTED_TARGETS. Set DORT_TARGET to choose one."
        ;;
    esac
  fi
  case " $SUPPORTED_TARGETS " in
    *" $TARGET "*) ;;
    *) die "unsupported host target $TARGET from DORT_TARGET. Supported targets: $SUPPORTED_TARGETS." ;;
  esac
}

detect_tools() {
  if command -v curl >/dev/null 2>&1; then
    FETCH=curl
  elif command -v wget >/dev/null 2>&1; then
    FETCH=wget
  else
    die "no downloader found. Install curl or wget, then run this script again."
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    DIGEST_TOOL=sha256sum
  elif command -v shasum >/dev/null 2>&1; then
    DIGEST_TOOL=shasum
  else
    die "no SHA-256 tool found. Install sha256sum or shasum, then run this script again. The download cannot be verified without one."
  fi
  command -v tar >/dev/null 2>&1 || die "no tar found. Install tar, then run this script again."
}

fetch() {
  case "$FETCH" in
    curl) curl -fsSL "$1" ;;
    wget) wget -qO- "$1" ;;
    *) return 1 ;;
  esac
}

file_digest() {
  case "$DIGEST_TOOL" in
    sha256sum) sha256sum "$1" | awk '{print $1}' ;;
    shasum) shasum -a 256 "$1" | awk '{print $1}' ;;
    *) return 1 ;;
  esac
}

tty_available() {
  : 2>/dev/null >/dev/tty
}

run_with_tty() {
  if tty_available; then
    "$@" </dev/tty
  else
    "$@" </dev/null
  fi
}

confirm() {
  local reply
  if [ "$ASSUME_YES" -eq 1 ]; then
    printf '%s y from --yes\n' "$1"
    return 0
  fi
  if ! tty_available; then
    return 1
  fi
  printf '%s [y/N] ' "$1" >/dev/tty || return 1
  if ! read -r reply </dev/tty; then
    printf '\n' >/dev/tty || true
    return 1
  fi
  case "$reply" in
    y|Y|yes|YES|Yes) return 0 ;;
    *) return 1 ;;
  esac
}

probe_prerequisites() {
  local name probe reason
  while IFS='|' read -r name probe reason; do
    [ -n "$name" ] || continue
    if prerequisite_present "$probe"; then
      continue
    fi
    MISSING_NAMES[$MISSING_COUNT]="$name"
    MISSING_REASONS[$MISSING_COUNT]="$reason"
    MISSING_COUNT=$((MISSING_COUNT + 1))
  done <<EOF
$PREREQUISITES
EOF
}

prerequisite_present() {
  local executable
  for executable in $1; do
    command -v "$executable" >/dev/null 2>&1 || return 1
  done
  return 0
}

manual_command() {
  case "$1" in
    brew) printf '%s\n' "/bin/bash -c \"\$(curl -fsSL $BREW_INSTALLER)\"" ;;
    git)
      if command -v brew >/dev/null 2>&1; then
        printf 'brew install git\n'
      elif [ "$HOST_SYSTEM" = Darwin ]; then
        printf 'xcode-select --install\n'
      else
        printf 'sudo apt-get install git\n'
      fi
      ;;
    node)
      printf 'curl -fsSL %s | bash\n' "$NVM_INSTALLER"
      printf '. "$NVM_DIR/nvm.sh" && nvm install --lts\n'
      ;;
    *) printf 'unknown prerequisite %s\n' "$1" ;;
  esac
}

install_prerequisites() {
  local index name manual
  index=0
  while [ "$index" -lt "$MISSING_COUNT" ]; do
    name="${MISSING_NAMES[$index]}"
    manual="$(manual_command "$name")"
    if [ "$SKIP_DEPS" -eq 1 ]; then
      printf 'Missing %s: %s. Install it with: %s\n' "$name" "${MISSING_REASONS[$index]}" "$manual"
    elif confirm "Missing $name: ${MISSING_REASONS[$index]}. Install $name now?"; then
      if install_prerequisite "$name"; then
        printf 'Installed %s\n' "$name"
      else
        printf 'Could not install %s. Install it with: %s\n' "$name" "$(manual_command "$name")"
      fi
    else
      printf 'Skipped %s. Install it with: %s\n' "$name" "$manual"
    fi
    index=$((index + 1))
  done
}

install_prerequisite() {
  case "$1" in
    brew) install_brew ;;
    git) install_git ;;
    node) install_node ;;
    *) return 1 ;;
  esac
}

install_brew() {
  local brew
  if [ "$(id -u)" -eq 0 ]; then
    printf 'Homebrew needs a normal user account. Run this script as that user.\n'
    return 1
  fi
  fetch "$BREW_INSTALLER" >"$WORK/homebrew-install.sh" || return 1
  NONINTERACTIVE=1 run_with_tty /bin/bash "$WORK/homebrew-install.sh" || return 1
  for brew in /opt/homebrew/bin/brew /usr/local/bin/brew "$HOME/homebrew/bin/brew"; do
    if [ -x "$brew" ]; then
      eval "$("$brew" shellenv)" || return 1
      break
    fi
  done
  command -v brew >/dev/null 2>&1
}

install_git() {
  if ! command -v brew >/dev/null 2>&1; then
    printf 'Homebrew is missing, so this script cannot install Git.\n'
    return 1
  fi
  printf 'Installing git with Homebrew.\n'
  brew install git
}

install_node() {
  local restore status
  NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  export NVM_DIR
  if [ ! -s "$NVM_DIR/nvm.sh" ]; then
    fetch "$NVM_INSTALLER" >"$WORK/nvm-install.sh" || return 1
    run_with_tty /bin/bash "$WORK/nvm-install.sh" || return 1
  fi
  [ -s "$NVM_DIR/nvm.sh" ] || return 1
  printf 'Installing the Node.js LTS release with nvm.\n'
  restore="$(set +o)"
  set +eu
  # shellcheck disable=SC1091
  . "$NVM_DIR/nvm.sh" && nvm install --lts
  status=$?
  eval "$restore"
  [ "$status" -eq 0 ] || return 1
  prerequisite_present "node npm"
}

resolve_install_dir() {
  if [ -n "$INSTALL_DIR_ARG" ]; then
    INSTALL_DIR="$INSTALL_DIR_ARG"
  elif [ -n "${DORT_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$DORT_INSTALL_DIR"
  elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    INSTALL_DIR=/usr/local/bin
  else
    INSTALL_DIR="$HOME/.local/bin"
  fi
  case "$INSTALL_DIR" in
    /*) ;;
    *) die "install directory must be an absolute path: $INSTALL_DIR" ;;
  esac
  mkdir -p "$INSTALL_DIR" || die "cannot create $INSTALL_DIR"
  [ -w "$INSTALL_DIR" ] || die "$INSTALL_DIR is not writable. Pick one with --install-dir."
}

checksum_entry() {
  local digest name matches count
  count=0
  matches=""
  while read -r digest name; do
    [ -n "${name:-}" ] || continue
    case "$name" in
      dort-v*"-$TARGET.tar.gz") ;;
      *) continue ;;
    esac
    [ "${#digest}" -eq 64 ] || die "malformed checksum line for $name"
    case "$digest" in
      *[!0-9a-f]*) die "malformed checksum line for $name" ;;
    esac
    count=$((count + 1))
    matches="$digest $name"
  done <"$1"
  if [ "$count" -eq 0 ]; then
    die "$1 has no checksum entry for target $TARGET."
  fi
  if [ "$count" -gt 1 ]; then
    die "$1 has $count checksum entries for target $TARGET, expected one."
  fi
  printf '%s\n' "$matches"
}

install_dort() {
  local base entry expected name archive actual source staged
  resolve_install_dir
  if [ -n "${DORT_RELEASE_BASE:-}" ]; then
    base="$DORT_RELEASE_BASE"
  elif [ "$RELEASE_TAG" = latest ]; then
    base="https://github.com/${REPOSITORY}/releases/latest/download"
  else
    base="https://github.com/${REPOSITORY}/releases/download/${RELEASE_TAG}"
  fi
  base="${base%/}"

  printf 'Installing dort for %s from %s\n' "$TARGET" "$base"
  fetch "$base/SHA256SUMS" >"$WORK/SHA256SUMS" || die "cannot download $base/SHA256SUMS. Check the tag and the network."
  if ! entry="$(checksum_entry "$WORK/SHA256SUMS")"; then
    exit 1
  fi
  expected="${entry%% *}"
  name="${entry#* }"

  archive="$WORK/$name"
  fetch "$base/$name" >"$archive" || die "cannot download $base/$name"
  actual="$(file_digest "$archive")"
  if [ "$actual" != "$expected" ]; then
    die "SHA-256 mismatch for $name. Expected $expected, got $actual. The download is corrupt or replaced; nothing was installed."
  fi
  printf 'Verified SHA-256 for %s\n' "$name"

  tar -xzf "$archive" -C "$WORK" dort || die "cannot extract $name"
  source="$WORK/dort"
  [ -f "$source" ] || die "$name has no dort binary"
  staged="$INSTALL_DIR/.dort-new.$$"
  STAGED="$staged"
  cp "$source" "$staged" || die "cannot write $staged"
  chmod 755 "$staged" || die "cannot make $staged executable"
  mv -f "$staged" "$INSTALL_DIR/dort" || die "cannot replace $INSTALL_DIR/dort"
  STAGED=""
}

verify() {
  local binary reported
  binary="$INSTALL_DIR/dort"
  reported="$("$binary" --version)" || die "$binary --version failed. Check the file and run this script again."
  printf 'Installed %s\n' "$binary"
  printf '%s\n' "$reported"
  case ":${PATH:-}:" in
    *":$INSTALL_DIR:"*) ;;
    *) printf 'Add it to PATH: export PATH="%s:$PATH"\n' "$INSTALL_DIR" ;;
  esac
}

main() {
  trap cleanup EXIT
  parse_args "$@"
  detect_host
  detect_tools
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/dort-install.XXXXXX")" || die "cannot create a work directory"
  probe_prerequisites
  install_prerequisites
  install_dort
  verify
}

main "$@"
