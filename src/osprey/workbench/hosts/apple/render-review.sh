#!/bin/bash
# Render actual Osprey region snapshots at the reference viewport.
set -euo pipefail
if [[ $# != 4 ]]; then
    echo 'usage: render-review.sh SCENE_JSON WIDTH HEIGHT OUTPUT_PNG' >&2
    exit 2
fi
scene="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
output="$(cd "$(dirname "$4")" && pwd)/$(basename "$4")"
cd "$(dirname "$0")"
swift build --product SnapshotReview
products="$(swift build --show-bin-path)"
review="$(mktemp -d "${TMPDIR:-/tmp}/osprey-render-review.XXXXXX")"
trap 'rm -rf "$review"' EXIT
bundle="$review/Renderer Review.app/Contents"
mkdir -p "$bundle/MacOS" "$bundle/Resources"
cp "$products/SnapshotReview" "$bundle/MacOS/SnapshotReview"
cp -R ../../assets/. "$bundle/Resources/"
cat > "$bundle/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.ospreylang.workbench.renderreview</string>
<key>CFBundleExecutable</key><string>SnapshotReview</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSBackgroundOnly</key><true/>
</dict></plist>
PLIST
"$bundle/MacOS/SnapshotReview" "$2" "$3" "$output" < "$scene"
printf 'Rendered %s\n' "$output"
