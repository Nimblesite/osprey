#!/usr/bin/env bash
# Builds the same SwiftUI host ([NATIVE-HOSTS-FAMILY]) for iOS and runs it on the simulator:
# the application compiles to an ARM archive for the device or the simulator, the one
# OspreyShared library links it as on macOS, and the bundle that results is installed with
# simctl — only `--launch` opens Simulator.app. [NATIVE-SHM-VIEW]
#   OSPREY_SWIFT_APP=<folder> [OSPREY_IOS_ID=<bundle id>] [OSPREY_IOS_NAME=<name>] \
#     [OSPREY_IOS_SIMULATOR=<device>] ios.sh [ios-sim|ios] [--view-check|--launch]
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
action=${2:-}
case "$target/$action" in
    ios-sim/|ios-sim/--view-check|ios-sim/--launch|ios/) ;;
    *) echo "Usage: ios.sh [ios-sim|ios] [--view-check|--launch]" >&2; exit 2 ;;
esac

# The runtime and the application archive: the same ARC runtime and ABI macOS links.
cd "$repo"
nice make -s "$([[ $target == ios-sim ]] && echo _runtime_ios_sim_arc || echo _runtime_ios_arc)"
mkdir -p "$here/build/app"
nice "$compiler" build "$app" --target="$target" --memory=arc --quiet --no-warnings \
    -o "$here/build/app/libOspreyApp.a"
cp "$here/build/app/libOspreyApp.h" "$here/Sources/OspreyViewApp/include/libOspreyApp.h"
cp "$runtime/native_bridge.h" "$runtime/native_abi.h" "$here/Sources/OspreyViewApp/include/"
symbols="$(nm "$here/build/app/libOspreyApp.a" 2>/dev/null)"
for symbol in osprey_main osprey_app_argument osprey_app_open osprey_app_event osprey_host_abi_version; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The application does not export $symbol" >&2; exit 1; }
done

# The host itself: the one OspreyShared library, compiled for the iOS SDK.
cd "$here"
build="$here/build/$target"
products=$([[ $target == ios-sim ]] && echo Release-iphonesimulator || echo Release-iphoneos)
binary="$build/dd/Build/Products/$products/OspreyViewHostIOS"
nice xcodebuild -scheme OspreyViewHostIOS -configuration Release -destination "$([[ $target == ios-sim ]] \
    && echo 'generic/platform=iOS Simulator' || echo 'generic/platform=iOS')" \
    ARCHS=arm64 ONLY_ACTIVE_ARCH=YES -derivedDataPath "$build/dd" build >/dev/null
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
if [[ "$action" == "--launch" ]]; then
    open -a Simulator
    xcrun simctl launch "$device" "$identifier" --resources "$app"
    echo "$bundle"
    exit 0
fi
[[ "$action" == "--view-check" ]] || { echo "$bundle"; exit 0; }

# The headless journey: launch with the console attached and read the verdict back.
log="$build/view-check.log"
xcrun simctl launch --console "$device" "$identifier" --resources "$app" --view-check >"$log" 2>&1 &
launched=$!
seconds=0
while kill -0 "$launched" 2>/dev/null && (( seconds < 180 )); do
    sleep 1; seconds=$((seconds + 1))
done
if kill -0 "$launched" 2>/dev/null; then
    xcrun simctl terminate "$device" "$identifier" 2>/dev/null || true
    kill "$launched" 2>/dev/null || true
    echo "The view check did not finish in 180 s" >&2
    cat "$log" >&2
    exit 1
fi
wait "$launched" || true
cat "$log"
grep -q OSPREY_IOS_VIEW_OK "$log" || exit 1
written=$(xcrun simctl get_app_container "$device" "$identifier" data 2>/dev/null)/Documents/view-check.txt
[[ -f "$written" ]] && echo "verdict file: $(cat "$written")"
