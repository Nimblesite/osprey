#!/usr/bin/env bash
# [CAST-MOBILE-DELIVERY] Build the native SwiftUI player against the shared C ABI.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
target=${1:-ios-sim}
case "$target" in
    ios-sim) sdk=iphonesimulator; triple=arm64-apple-ios18.0-simulator ;;
    ios) sdk=iphoneos; triple=arm64-apple-ios18.0 ;;
    *) echo 'Usage: build.sh [ios|ios-sim]' >&2; exit 2 ;;
esac
cd "$repo"
if [[ ${OSPREY_MOBILE_SKIP_LIBRARY:-0} != 1 ]]; then bash examples/casting/mobile-build.sh "$target"; fi
output="$repo/examples/casting/build/$target"
bundle="$output/Casting.app"
mkdir -p "$bundle"
sdk_path=$(xcrun --sdk "$sdk" --show-sdk-path)
for unit in file_server_runtime media_render_runtime; do
    xcrun --sdk "$sdk" clang -target "$triple" -isysroot "$sdk_path" -std=c11 \
        -Wall -Wextra -Werror -fstack-protector-strong -D_FORTIFY_SOURCE=2 \
        -c "compiler/runtime/$unit.c" -o "$output/$unit.o"
done
xcrun --sdk "$sdk" swiftc -swift-version 5 -warnings-as-errors -parse-as-library \
    -target "$triple" -sdk "$sdk_path" -import-objc-header "$output/libOspreyApp.h" \
    hosts/swift/Sources/OspreyNative/*.swift "$here"/*.swift \
    "$output/libOspreyApp.a" "$output/file_server_runtime.o" "$output/media_render_runtime.o" \
    -framework Network -framework Security -framework SwiftUI -framework UIKit \
    -lsqlite3 -lz -o "$bundle/Casting"
cp examples/casting/player.machine.json "$bundle/player.machine.json"
cp -R examples/casting/skins "$bundle/skins"
python3 - "$bundle/Info.plist" <<'PY'
import plistlib,sys
with open(sys.argv[1], 'wb') as file:
    plistlib.dump(dict(CFBundleIdentifier='org.ospreylang.casting', CFBundleName='Casting',
        CFBundleExecutable='Casting', CFBundlePackageType='APPL', CFBundleVersion='1',
        CFBundleShortVersionString='1.0', MinimumOSVersion='18.0', LSRequiresIPhoneOS=True,
        UILaunchScreen={}, UIFileSharingEnabled=True, LSSupportsOpeningDocumentsInPlace=True, NSLocalNetworkUsageDescription='Find your Chromecast or Google TV and play your selected audio.',
        NSBonjourServices=['_googlecast._tcp'], UISupportedInterfaceOrientations=['UIInterfaceOrientationPortrait'],
        UIDeviceFamily=[1,2]), file)
PY
if [[ "$target" == ios-sim ]]; then codesign --force --sign - "$bundle"; fi
echo "$bundle"
