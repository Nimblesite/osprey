#!/usr/bin/env bash
# Install the pinned `deslop` duplication-gate CLI.
#
# Single source of truth for the deslop version AND the platform→asset mapping.
# Shared by `make setup` (local dev) and .github/workflows/ci.yml so the version
# is pinned in exactly one place. Downloads the release tarball, verifies its
# SHA-256, and installs the `deslop` binary onto PATH.
#
# Usage:
#   scripts/install-deslop.sh [INSTALL_DIR]
#
# INSTALL_DIR defaults to /usr/local/bin when writable, else ~/.local/bin.
# When run in CI ($GITHUB_PATH set) the chosen dir is appended to $GITHUB_PATH
# so subsequent workflow steps (e.g. `make lint`) can find the binary.
set -euo pipefail

DESLOP_VERSION="${DESLOP_VERSION:-0.5.1}"
BASE_URL="https://github.com/Nimblesite/Deslop/releases/download/v${DESLOP_VERSION}"

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'
say()  { echo -e "${CYAN}${BOLD}▶ $*${RESET}"; }
ok()   { echo -e "${GREEN}✓ $*${RESET}"; }
fail() { echo -e "${RED}✗ $*${RESET}" >&2; exit 1; }

# Already at the pinned version? Nothing to do (keeps `make setup` idempotent).
if command -v deslop &>/dev/null && deslop --version 2>/dev/null | grep -q "${DESLOP_VERSION}"; then
    ok "deslop ${DESLOP_VERSION} already installed ($(command -v deslop))"
    exit 0
fi

case "$(uname -s)" in
    Linux)  os=linux ;;
    Darwin) os=macos ;;
    *) fail "Unsupported OS for deslop install: $(uname -s) — install manually: $BASE_URL" ;;
esac
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64)  arch=x64 ;;
    *) fail "Unsupported arch for deslop install: $(uname -m) — install manually: $BASE_URL" ;;
esac

stem="deslop-${DESLOP_VERSION}-${os}-${arch}"
asset="${stem}.tar.gz"

# Pick an install dir on PATH (explicit arg wins; else writable system bin; else user bin).
dest="${1:-}"
if [[ -z "$dest" ]]; then
    if [[ -w /usr/local/bin ]]; then dest=/usr/local/bin; else dest="$HOME/.local/bin"; fi
fi
mkdir -p "$dest"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading $asset"
curl -sSfL -o "$tmp/$asset" "$BASE_URL/$asset"
curl -sSfL -o "$tmp/$asset.sha256" "$BASE_URL/$asset.sha256"

say "Verifying SHA-256"
expected="$(awk '{print $1}' "$tmp/$asset.sha256")"
if command -v sha256sum &>/dev/null; then
    actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
else
    actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
fi
[[ "$expected" == "$actual" ]] || fail "SHA-256 mismatch: expected $expected, got $actual"
ok "checksum verified"

tar -xzf "$tmp/$asset" -C "$tmp"
install -m 0755 "$tmp/$stem/deslop" "$dest/deslop"
ok "installed deslop ${DESLOP_VERSION} → $dest/deslop"

# CI: make the install dir discoverable by subsequent steps.
if [[ -n "${GITHUB_PATH:-}" ]]; then
    echo "$dest" >> "$GITHUB_PATH"
fi

# Local: nudge if the dir isn't already on PATH.
case ":$PATH:" in
    *":$dest:"*) ;;
    *) echo -e "${CYAN}  Note: add $dest to your PATH to run 'deslop' directly.${RESET}" ;;
esac
