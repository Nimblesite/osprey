#!/usr/bin/env bash
# Implements [IOS-TARGET-MACOS]: one SwiftUI app for the Mac and iPhone, built
# by the same script as every iPhone example.
#   run.sh                     build and open the Mac app
#   run.sh --smoke             assert the boundary, the walk and the screens on this Mac
#   run.sh --build [target]    build only: macos (default), ios or ios-sim
#   run.sh ios-sim             build and launch in the iPhone simulator
#   run.sh ios-sim-smoke       the same smoke check, in the simulator
# OSPREY_MACOS_SKIP_BUILD=1 reuses the existing Mac build.
set -euo pipefail
example_dir=$(cd "$(dirname "$0")" && pwd)
apple_dir="$example_dir/../ios"
export OSPREY_IOS_EXAMPLE_DIR="$example_dir"
export OSPREY_IOS_SCHEME=OspreyStorage
export OSPREY_IOS_SOURCE="$example_dir/storage"
export OSPREY_IOS_BUNDLE_ID=org.ospreylang.OspreyStorage
export OSPREY_IOS_SMOKE_FILE=storage-smoke-result.txt
export OSPREY_IOS_SMOKE_ARG=--smoke
export OSPREY_IOS_SMOKE_EXPECT=OSPREY_MACOS_STORAGE_SMOKE_OK
app="$example_dir/build/macos/products/OspreyStorage.app"
icon="$example_dir/build/Assets.xcassets/AppIcon.appiconset/ios.png"
if [[ ! -f "$icon" || "$example_dir/make-icon.swift" -nt "$icon" ]]; then
    swift "$example_dir/make-icon.swift" "$example_dir/build/Assets.xcassets"
fi

build_mac() {
    if [[ ${OSPREY_MACOS_SKIP_BUILD:-0} != 1 ]]; then bash "$apple_dir/build.sh" macos >&2; fi
}

case "${1:-}" in
    "")
        build_mac
        pkill -f "$app/Contents/MacOS/OspreyStorage" || true
        open "$app"
        ;;
    --smoke) build_mac; "$app/Contents/MacOS/OspreyStorage" --smoke ;;
    --build) exec bash "$apple_dir/build.sh" "${2:-macos}" ;;
    ios-sim) exec bash "$apple_dir/run.sh" ;;
    ios-sim-smoke) exec bash "$apple_dir/run.sh" --smoke ;;
    *) echo "Usage: $0 [--smoke|--build [macos|ios|ios-sim]|ios-sim|ios-sim-smoke]" >&2; exit 2 ;;
esac
