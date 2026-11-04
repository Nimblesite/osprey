#!/bin/bash
# Implements [WORKBENCH-DESIGN-COLOUR]: every ICNS representation uses the same contrasting tile.
set -euo pipefail
source_png="$1"
destination="$2"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/osprey-dock-icon.XXXXXX")
trap 'rm -r "$scratch"' EXIT
iconset="$scratch/Osprey.iconset"
mkdir -p "$iconset"
swift "$(dirname "$0")/icon.swift" "$source_png" "$scratch/icon.png"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$scratch/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$scratch/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$destination"
