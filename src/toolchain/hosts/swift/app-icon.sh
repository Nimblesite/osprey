#!/usr/bin/env bash
set -euo pipefail
image="$1"
output="$2"
work="$(mktemp -d "${TMPDIR:-/tmp}/osprey-icon.XXXXXX")"
trap 'rm -rf "$work"' EXIT
iconset="$work/AppIcon.iconset"
mkdir -p "$iconset" "$(dirname "$output")"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$image" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$image" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$output"
