#!/usr/bin/env bash
#
# Install the Groot toolchain system-wide.
#
# Builds `groot`, `groot-host`, and `groot-monitor` from this checkout and
# installs all three into a directory on your PATH (default: /usr/local/bin).
#
# Usage:
#   ./_groot/install.sh                 # build + install (system-wide)
#   ./_groot/install.sh --user          # install to ~/.local/bin
#   ./_groot/install.sh --no-build      # install existing bin/ artifacts
#   ./_groot/install.sh --install-dir D # install into D
#
# Environment:
#   GROOT_INSTALL_DIR   Destination directory (same as --install-dir).

set -euo pipefail

if [[ -t 1 ]]; then
    purple=$'\033[38;5;141m'
    green=$'\033[38;5;48m'
    cyan=$'\033[38;5;81m'
    yellow=$'\033[38;5;220m'
    red=$'\033[38;5;197m'
    bold=$'\033[1m'
    reset=$'\033[0m'
else
    purple=''; green=''; cyan=''; yellow=''; red=''; bold=''; reset=''
fi

log()  { printf '%s==>%s %s\n' "$cyan" "$reset" "$*"; }
ok()   { printf '%s✓%s %s\n' "$green" "$reset" "$*"; }
warn() { printf '%sWARN:%s %s\n' "$yellow" "$reset" "$*" >&2; }
die()  { printf '%sError:%s %s\n' "$red" "$reset" "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Install the Groot toolchain (groot, groot-host, groot-monitor).

Usage:
  install.sh [options]

Options:
  --install-dir DIR   Install binaries into DIR (default: /usr/local/bin)
  --user              Install into ~/.local/bin
  --no-build          Skip building; use existing bin/ artifacts
  -h, --help          Show this help

Environment:
  GROOT_INSTALL_DIR   Destination directory (same as --install-dir)
EOF
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

INSTALL_DIR="${GROOT_INSTALL_DIR:-}"
INSTALL_DIR_EXPLICIT=0
[[ -n "${GROOT_INSTALL_DIR:-}" ]] && INSTALL_DIR_EXPLICIT=1

DO_BUILD=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --install-dir)
            [[ $# -ge 2 ]] || die "--install-dir requires a directory"
            INSTALL_DIR="$2"
            INSTALL_DIR_EXPLICIT=1
            shift 2
            ;;
        --install-dir=*)
            INSTALL_DIR="${1#--install-dir=}"
            INSTALL_DIR_EXPLICIT=1
            shift
            ;;
        --user)
            INSTALL_DIR="$HOME/.local/bin"
            INSTALL_DIR_EXPLICIT=1
            shift
            ;;
        --no-build)
            DO_BUILD=0
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown option '$1' (use --help for usage)"
            ;;
    esac
done

printf '%s%s◆ Groot toolchain installer%s\n' "$purple" "$bold" "$reset"

# Prefer updating the `groot` that actually runs in this shell. Installing only
# to /usr/local/bin leaves an older copy earlier on PATH shadowing the new one.
FOUND_EXISTING=0
if [[ "$INSTALL_DIR_EXPLICIT" -eq 0 ]]; then
    existing_groot="$(command -v groot || true)"
    if [[ -n "$existing_groot" ]]; then
        FOUND_EXISTING=1
        INSTALL_DIR="$(cd "$(dirname "$existing_groot")" && pwd)"
        log "Existing groot found at $existing_groot; updating that installation."
    else
        INSTALL_DIR="/usr/local/bin"
    fi
fi

[[ -n "$INSTALL_DIR" ]] || die "install directory must not be empty"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

if [[ "$DO_BUILD" -eq 1 ]]; then
    command -v go >/dev/null 2>&1 \
        || die "Go is required to build the groot CLI. Install Go 1.21+ and retry."
    command -v cargo >/dev/null 2>&1 \
        || die "Rust/Cargo is required to build groot-host and groot-monitor. Install Rust from https://rustup.rs and retry."

    log "Building groot, groot-host, and groot-monitor (release)..."
    if command -v make >/dev/null 2>&1; then
        (cd "$REPO_ROOT" && make groot-tools)
    else
        # No make available: replicate the Makefile's groot-tools target.
        mkdir -p "$REPO_ROOT/bin"
        (cd "$REPO_ROOT" && go build -trimpath -ldflags='-s -w' -o bin/groot ./cmd/groot)
        (cd "$REPO_ROOT/_groot" && cargo build --release --package groot-host --package groot-monitor)
        cp "$REPO_ROOT/_groot/target/release/groot-host"    "$REPO_ROOT/bin/groot-host"
        cp "$REPO_ROOT/_groot/target/release/groot-monitor" "$REPO_ROOT/bin/groot-monitor"
    fi
fi

BINARIES=(groot groot-host groot-monitor)
for bin in "${BINARIES[@]}"; do
    [[ -f "$REPO_ROOT/bin/$bin" ]] \
        || die "missing $REPO_ROOT/bin/$bin (run without --no-build to build it)"
    [[ -x "$REPO_ROOT/bin/$bin" ]] || chmod 0755 "$REPO_ROOT/bin/$bin"
done

# ---------------------------------------------------------------------------
# Choose an install directory
# ---------------------------------------------------------------------------

USE_SUDO=0

ensure_install_dir() {
    if [[ -d "$INSTALL_DIR" && -w "$INSTALL_DIR" ]]; then
        return 0
    fi

    if mkdir -p "$INSTALL_DIR" 2>/dev/null && [[ -w "$INSTALL_DIR" ]]; then
        return 0
    fi

    if command -v sudo >/dev/null 2>&1; then
        log "Administrator privileges are required to write to $INSTALL_DIR"
        if sudo mkdir -p "$INSTALL_DIR" && sudo test -w "$INSTALL_DIR"; then
            USE_SUDO=1
            return 0
        fi
    fi

    if [[ "$INSTALL_DIR_EXPLICIT" -eq 1 ]]; then
        die "cannot write to $INSTALL_DIR (set GROOT_INSTALL_DIR or use --user)"
    fi

    if [[ "$FOUND_EXISTING" -eq 1 ]]; then
        die "cannot replace the existing groot at $existing_groot (permission denied). Re-run with sudo, or install elsewhere with --user / --install-dir."
    fi

    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
    warn "No write access to /usr/local/bin; installing to $INSTALL_DIR instead."
    warn "Add $INSTALL_DIR to your PATH if it is not already there."
}

ensure_install_dir

# ---------------------------------------------------------------------------
# Install atomically (replace via rename, so running processes keep their inode)
# ---------------------------------------------------------------------------

TMP_FILES=()
cleanup() {
    local f
    for f in "${TMP_FILES[@]:-}"; do
        [[ -n "$f" ]] || continue
        if [[ "$USE_SUDO" -eq 1 ]]; then
            sudo rm -f "$f" 2>/dev/null || true
        else
            rm -f "$f" 2>/dev/null || true
        fi
    done
}
trap cleanup EXIT

install_binary() {
    local name="$1"
    local src="$REPO_ROOT/bin/$name"
    local dest="$INSTALL_DIR/$name"
    local tmp="$INSTALL_DIR/.${name}.install.$$.$RANDOM"

    TMP_FILES+=("$tmp")
    if [[ "$USE_SUDO" -eq 1 ]]; then
        sudo sh -c 'set -e; cp "$1" "$2"; chmod 0755 "$2"; mv -f "$2" "$3"' \
            sh "$src" "$tmp" "$dest"
    else
        cp "$src" "$tmp"
        chmod 0755 "$tmp"
        mv -f "$tmp" "$dest"
    fi
}

log "Installing toolchain into $INSTALL_DIR..."
for bin in "${BINARIES[@]}"; do
    install_binary "$bin"
    ok "$INSTALL_DIR/$bin"
done
TMP_FILES=()
trap - EXIT

# ---------------------------------------------------------------------------
# Verify
# ---------------------------------------------------------------------------

find_path_copies() {
    local name="$1" dir
    local IFS=':'
    for dir in $PATH; do
        [[ -n "$dir" ]] || dir="."
        if [[ -f "$dir/$name" && -x "$dir/$name" ]]; then
            printf '%s\n' "$dir/$name"
        fi
    done | awk '!seen[$0]++'
}

printf '\n'
"$INSTALL_DIR/groot" version || warn "installed groot failed to run"

resolved="$(command -v groot || true)"
if [[ -z "$resolved" ]]; then
    warn "'groot' is not on PATH. Add $INSTALL_DIR to your PATH, e.g.:"
    printf '       export PATH="%s:$PATH"\n' "$INSTALL_DIR" >&2
elif [[ "$resolved" != "$INSTALL_DIR/groot" ]]; then
    warn "'groot' resolves to $resolved, not $INSTALL_DIR/groot."
    warn "Remove the stale binary or reorder your PATH."
fi

# Warn about older copies elsewhere on PATH (e.g. a legacy Rust build in
# ~/.cargo/bin). They usually do not shadow the install above, but can in
# shells or services with a different PATH order.
for name in "${BINARIES[@]}"; do
    other_copies="$(find_path_copies "$name" | grep -v -F -x "$INSTALL_DIR/$name" || true)"
    if [[ -n "$other_copies" ]]; then
        warn "Other '$name' copies are still on PATH:"
        while IFS= read -r other; do
            printf '       %s\n' "$other" >&2
        done <<< "$other_copies"
        warn "They can shadow this install if your PATH order changes; remove or replace them."
    fi
done

if ! command -v ube >/dev/null 2>&1; then
    warn "'ube' was not found on PATH. 'groot start' needs the Ube compiler."
    warn "Install it with: curl -sSL https://raw.githubusercontent.com/JohnEsleyer/ube/main/install.sh | bash"
fi

printf '\n%s%s✓ Groot installed successfully%s\n\n' "$green" "$bold" "$reset"
printf 'Get started:\n'
printf '  groot init my_game\n'
printf '  cd my_game && groot start .\n'
printf '  groot start . --monitor\n\n'
