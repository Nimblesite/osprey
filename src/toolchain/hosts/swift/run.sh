#!/usr/bin/env bash
# Builds an Osprey Native application as the Mac's C ABI archive, links it into
# the shared-memory view host and assembles an ad-hoc signed development bundle.
#   OSPREY_SWIFT_APP=<folder> OSPREY_SWIFT_NAME=<name> OSPREY_SWIFT_BUNDLE_ID=<id> run.sh [--build] [-- app arguments]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
app=${OSPREY_SWIFT_APP:?OSPREY_SWIFT_APP names the application folder}
# Absolute, so `--resources` still names the application after this script changes folder.
case "$app" in /*) ;; *) app="$repo/$app" ;; esac
name=${OSPREY_SWIFT_NAME:-$(basename "$app")}
identifier=${OSPREY_SWIFT_BUNDLE_ID:-org.ospreylang.$name}
runtime="$repo/src/toolchain/compiler/runtime"
build_only=0
if [ "${1:-}" = --build ]; then build_only=1; shift; fi
if [ "${1:-}" = -- ]; then shift; fi

mkdir -p "$here/build/app"
# Packaging still checks types; advisory annotation proofs belong to analysis.
"$compiler" "$app" --compile --target=macos --memory=arc --quiet --no-warnings -o "$here/build/app/libOspreyApp.a"
symbols="$(nm "$here/build/app/libOspreyApp.a" 2>/dev/null)"
for symbol in osprey_main osprey_app_argument osprey_app_open osprey_app_event osprey_host_abi_version; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The application does not export $symbol" >&2; exit 1; }
done
cp "$here/build/app/libOspreyApp.h" "$here/Sources/OspreyViewApp/include/libOspreyApp.h"
cp "$runtime/native_bridge.h" "$runtime/native_abi.h" "$here/Sources/OspreyViewApp/include/"
cd "$here"
swift build -c release --product OspreyViewHost
binary="$(swift build -c release --show-bin-path)/OspreyViewHost"
bundle="$here/build/$name.app"
mkdir -p "$bundle/Contents/MacOS"
cp "$binary" "$bundle/Contents/MacOS/$name.new"
mv -f "$bundle/Contents/MacOS/$name.new" "$bundle/Contents/MacOS/$name"
# An application's icon.png, 1024 pixels square, becomes the bundle's icon in every size a Mac asks for.
mkdir -p "$bundle/Contents/Resources"
# An application's assets folder travels with it: fonts, symbol artwork and manifests the host reads.
if [ -d "$app/assets" ]; then
  cp -R "$app/assets/." "$bundle/Contents/Resources/"
fi
if [ -f "$app/icon.png" ]; then
    bash "$here/app-icon.sh" "$app/icon.png" "$bundle/Contents/Resources/AppIcon.icns"
fi
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$identifier</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundleExecutable</key><string>$name</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1</string>
<key>LSMinimumSystemVersion</key><string>15.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>NSAppTransportSecurity</key><dict><key>NSAllowsLocalNetworking</key><true/></dict>
</dict></plist>
PLIST
codesign --force --sign - "$bundle" 2>/dev/null
if [ "$build_only" = 1 ]; then echo "$bundle"; exit 0; fi
pkill -f "$bundle/Contents/MacOS/$name" 2>/dev/null || true
open "$bundle" --args --resources "$app" "$@"
