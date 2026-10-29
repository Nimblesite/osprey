#!/bin/bash
# Implements [UI-HOST] for the Mac: builds the Osprey Workbench and opens it.
# The Osprey application compiles to a static library through the Mac C ABI
# (`--target=macos`), the SwiftUI host links it, and the lessons are bundled
# as resources.
#   run.sh           build and open the app
#   run.sh --build   build only
set -euo pipefail
cd "$(dirname "$0")"
repo=$(cd ../../../.. && pwd)
compiler="${OSPREY_BIN:-$repo/target/release/osprey}"
if [[ ! -x "$compiler" ]]; then
    echo "Osprey compiler not found at $compiler. Run make build from the repository root." >&2
    exit 1
fi
library=.build/osprey/macos
mkdir -p "$library" Sources/OspreyApp/include
"$compiler" app --compile --target=macos --no-warnings -o "$library/libOspreyApp.a"
cp "$library/libOspreyApp.h" Sources/OspreyApp/include/libOspreyApp.h
symbols="$(nm "$library/libOspreyApp.a")"
for symbol in osprey_main osprey_app_start osprey_app_dispatch; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The Osprey library does not export $symbol" >&2; exit 1; }
done
swift build -c release
products="$(swift build -c release --show-bin-path)"
app="$PWD/.build/Osprey Workbench.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$products/Workbench" "$app/Contents/MacOS/Workbench"
rm -rf "$app/Contents/Resources/lessons"
cp -R ../../lessons "$app/Contents/Resources/lessons"
cp -R ../../assets/. "$app/Contents/Resources/"
python3 "$repo/scripts/workbench-proof-assets.py" "$app/Contents/Resources"
cp "$repo/website/src/assets/images/logo.png" "$app/Contents/Resources/osprey-logo.png"
iconset="$PWD/.build/Osprey.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$app/Contents/Resources/osprey-logo.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$app/Contents/Resources/osprey-logo.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Osprey.icns"
rm -rf "$iconset"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.ospreylang.workbench</string>
<key>CFBundleName</key><string>Osprey Workbench</string>
<key>CFBundleIconFile</key><string>Osprey.icns</string>
<key>CFBundleExecutable</key><string>Workbench</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
/usr/bin/codesign --force --sign - "$app"
printf 'Built %s\n' "$app"
if [[ "${1:-}" != --build ]]; then
    pkill -f "$app/Contents/MacOS/Workbench" || true
    open "$app"
fi
