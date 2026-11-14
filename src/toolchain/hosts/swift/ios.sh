#!/usr/bin/env bash
# Builds the same SwiftUI host ([NATIVE-HOSTS-FAMILY]) for iOS and runs it on the simulator:
# the application compiles to an ARM archive for the device or the simulator, the one
# OspreyShared library links it as on macOS, and the bundle that results is installed with
# simctl — only `--launch` opens Simulator.app. [NATIVE-SHM-VIEW]
#   OSPREY_SWIFT_APP=<folder> [OSPREY_IOS_ID=<bundle id>] [OSPREY_IOS_NAME=<name>] \
#     [OSPREY_IOS_SIMULATOR=<device>] ios.sh [ios-sim|ios] [--view-check|--launch|--golden]...
# Several actions run in order on one build. `--golden` (ios-sim) launches the app, waits for it to settle and compares the screen, without
# its system bars, with the application's `goldens/ios/<width>-ready.png` (scripts/mobile/goldens.py).
# `--view-check` (ios-sim) runs the headless journey with no person present and fails unless it
# reads OSPREY_IOS_VIEW_OK; `--launch` (ios-sim) installs the app and opens it in the Simulator. The default
# application is Tally.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
runtime="$repo/src/toolchain/compiler/runtime"
app=${OSPREY_SWIFT_APP:-"$repo/src/osprey/examples/tally"}
case "$app" in /*) ;; *) app="$repo/$app" ;; esac
name=${OSPREY_IOS_NAME:-$(basename "$app")}
identifier=${OSPREY_IOS_ID:-org.ospreylang.$name}
target=${1:-ios-sim}
actions=("${@:2}")
usage() { echo "Usage: ios.sh [ios-sim|ios] [--view-check|--launch|--golden]..." >&2; exit 2; }
[[ $target == ios-sim || $target == ios ]] || usage
for action in ${actions[@]+"${actions[@]}"}; do
    case "$target/$action" in ios-sim/--view-check|ios-sim/--launch|ios-sim/--golden) ;; *) usage ;; esac
done

# The runtime and the application archive: the same ARC runtime and ABI macOS links.
cd "$repo"
nice make -s "$([[ $target == ios-sim ]] && echo _runtime_ios_sim_arc || echo _runtime_ios_arc)"
archive="$here/build/$target/app"
mkdir -p "$archive"
nice "$compiler" build "$app" --target="$target" --memory=arc --quiet --no-warnings \
    -o "$archive/libOspreyApp.a"
cp "$archive/libOspreyApp.h" "$here/Sources/OspreyViewApp/include/libOspreyApp.h"
cp "$runtime/native_bridge.h" "$runtime/native_abi.h" "$here/Sources/OspreyViewApp/include/"
symbols="$(nm "$archive/libOspreyApp.a" 2>/dev/null)"
for symbol in osprey_main osprey_app_argument osprey_app_open osprey_app_event osprey_host_abi_version; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The application does not export $symbol" >&2; exit 1; }
done

# Compile the one OspreyShared library directly for the selected iOS SDK.
cd "$here"
build="$here/build/$target"
sdk=iphoneos
triple=arm64-apple-ios18.0
if [[ $target == ios-sim ]]; then
    sdk=iphonesimulator
    triple+=-simulator
fi
arguments=(--configuration release --triple "$triple" --sdk "$(xcrun --sdk "$sdk" --show-sdk-path)"
    --product OspreyViewHostIOS --scratch-path "$build/swift")
export OSPREY_APP_ARCHIVE="$archive"
nice xcrun swift build "${arguments[@]}"
binary="$(xcrun swift build "${arguments[@]}" --show-bin-path)/OspreyViewHostIOS"
[[ -x "$binary" ]] || { echo "The iOS host did not build" >&2; exit 1; }
bundle="$build/$name.app"
rm -rf "$bundle"
mkdir -p "$bundle"
cp -p "$binary" "$bundle/OspreyViewHostIOS"
OSPREY_IOS_BUNDLE="$bundle" python3 - "$identifier" "$name" <<'PLIST'
import os, plistlib, sys
bundle = os.environ['OSPREY_IOS_BUNDLE']
with open(os.path.join(bundle, 'Info.plist'), 'wb') as file:
    plistlib.dump(dict(CFBundleIdentifier=sys.argv[1], CFBundleName=sys.argv[2],
        CFBundleExecutable='OspreyViewHostIOS', CFBundlePackageType='APPL', CFBundleVersion='1',
        CFBundleShortVersionString='1.0', MinimumOSVersion='18.0', LSRequiresIPhoneOS=True,
        UILaunchScreen={}, UIApplicationSceneManifest={'UIApplicationSupportsMultipleScenes': False},
        UISupportedInterfaceOrientations=['UIInterfaceOrientationPortrait', 'UIInterfaceOrientationLandscapeLeft',
                                          'UIInterfaceOrientationLandscapeRight'],
        UIDeviceFamily=[1, 2], UIFileSharingEnabled=True, LSSupportsOpeningDocumentsInPlace=True), file)
PLIST
if [[ "$target" == ios-sim ]]; then
    codesign --force --sign - "$bundle" >/dev/null
else
    # A device build stays unsigned: signing needs this Mac's distribution identity.
    echo "Unsigned device bundle: $bundle"
    exit 0
fi

# The device every simulator action runs on: the one OSPREY_IOS_SIMULATOR names, or the first iPhone.
device=${OSPREY_IOS_SIMULATOR:-$(xcrun simctl list devices available --json | python3 -c '
import json, sys
ran = json.load(sys.stdin)["devices"].values()
phones = [x for run in ran for x in run if "iphone" in x["name"].lower()]
print(phones[0]["udid"] if phones else "")')}
[[ -n "$device" ]] || { echo "No iPhone simulator is available" >&2; exit 1; }
xcrun simctl boot "$device" 2>/dev/null || true
xcrun simctl bootstatus "$device" -b >/dev/null
xcrun simctl install "$device" "$bundle"

launch() {
    open -a Simulator
    xcrun simctl launch "$device" "$identifier" --resources "$app"
    echo "$bundle"
}

golden() {
    xcrun simctl terminate "$device" "$identifier" 2>/dev/null || true
    xcrun simctl launch "$device" "$identifier" --resources "$app" >/dev/null
    sleep 3
    xcrun simctl io "$device" screenshot "$build/ready.png" >/dev/null
    (cd "$repo" && python3 scripts/mobile/goldens.py "$build/ready.png" "$app/goldens/ios")
}

# The headless journey: launch with the console attached and read the verdict back.
view_check() {
    local log="$build/view-check.log" launched seconds=0 written
    xcrun simctl terminate "$device" "$identifier" 2>/dev/null || true
    xcrun simctl launch --console "$device" "$identifier" --resources "$app" --view-check >"$log" 2>&1 &
    launched=$!
    while kill -0 "$launched" 2>/dev/null && (( seconds < 180 )); do
        sleep 1; seconds=$((seconds + 1))
    done
    if kill -0 "$launched" 2>/dev/null; then
        xcrun simctl terminate "$device" "$identifier" 2>/dev/null || true
        kill "$launched" 2>/dev/null || true
        echo "The view check did not finish in 180 s" >&2
        cat "$log" >&2
        return 1
    fi
    wait "$launched" || true
    cat "$log"
    grep -q OSPREY_IOS_VIEW_OK "$log" || return 1
    written=$(xcrun simctl get_app_container "$device" "$identifier" data 2>/dev/null)/Documents/view-check.txt
    [[ ! -f "$written" ]] || echo "verdict file: $(cat "$written")"
}

(( ${#actions[@]} )) || { echo "$bundle"; exit 0; }
for action in ${actions[@]+"${actions[@]}"}; do
    case "$action" in
        --launch) launch ;;
        --golden) golden ;;
        --view-check) view_check ;;
    esac
done
