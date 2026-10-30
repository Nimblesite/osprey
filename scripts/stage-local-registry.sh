#!/usr/bin/env bash
# Stage the repository's own packages as a folder registry ([PACKAGE-FOLDER-REGISTRY]).
#
# Every packages/<dir>/osprey.toml that declares a [package] name is published
# with the package client's `publish --folder`, so each release is checked and
# digested exactly as the server will check it. The result is the tree the local
# registry serves (`make registry-serve`) and the VS Code extension bundles.
#
# Usage: scripts/stage-local-registry.sh <osprey-pkg> <registry-root>
set -euo pipefail

client="$1"
out="$2"
repo="$(cd "$(dirname "$0")/.." && pwd)"

case "$client" in /*) ;; *) client="$PWD/$client" ;; esac
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac

rm -rf "$out"
mkdir -p "$out"

published=0
refused=0
for manifest in "$repo"/packages/*/osprey.toml; do
  project="$(dirname "$manifest")"
  grep -q '^\[package\]' "$manifest" || continue
  key="$(sed -n 's/^name = "\(@[^"]*\)"/\1/p' "$manifest" | head -1)"
  report="$(echo "publish --folder $out --project $project" | "$client" 2>&1 || true)"
  if printf '%s\n' "$report" | grep -qx 'status: 0'; then
    published=$((published + 1))
    echo "  published $key"
  else
    refused=$((refused + 1))
    echo "  refused   $key: $(printf '%s\n' "$report" | grep -v '^status:' | tr '\n' ' ')" >&2
  fi
done

echo "staged $published packages into $out ($refused refused)"
[ "$published" -gt 0 ]
