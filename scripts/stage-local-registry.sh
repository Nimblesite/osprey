#!/usr/bin/env bash
# Stage the repository's own packages as a folder registry ([PACKAGE-FOLDER-REGISTRY]).
#
# Every packages/<dir>/osprey.toml that declares a [package] name is published
# with the package client's `publish --folder`, so each release is checked and
# digested exactly as the server will check it. The result is the tree the local
# registry serves (`make -C packages registry-serve`) and the VS Code extension bundles.
# A package that binds a raw native function is refused by design (source.unclassified-builtin);
# any other refusal, a bad manifest or name, is a mistake and fails the staging.
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
mistakes=0
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
    reason="$(printf '%s\n' "$report" | grep -v '^status:' | tr '\n' ' ')"
    echo "  refused   $key: $reason" >&2
    case "$reason" in source.unclassified-builtin*) ;; *) mistakes=$((mistakes + 1)) ;; esac
  fi
done

echo "staged $published packages into $out ($refused refused)"
[ "$mistakes" -eq 0 ] || { echo "stage-local-registry: $mistakes package(s) refused for a manifest mistake" >&2; exit 1; }
[ "$published" -gt 0 ]
