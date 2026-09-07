#!/usr/bin/env bash
# Implements [IOS-SWIFT-HOST] and [IOS-VERIFICATION].
set -euo pipefail

example_dir=$(cd "$(dirname "$0")" && pwd)
repo_dir=$(cd "$example_dir/../.." && pwd)
target=${1:-ios-sim}
signing=(CODE_SIGNING_ALLOWED=NO)
case "$target" in
    ios) sdk=iphoneos; destination='generic/platform=iOS' ;;
    ios-sim) sdk=iphonesimulator; destination='generic/platform=iOS Simulator' ;;
    ios-device)
        : "${OSPREY_DEVELOPMENT_TEAM:?Set OSPREY_DEVELOPMENT_TEAM to your Xcode signing team ID.}"
        : "${OSPREY_DEVICE_UDID:?Use run-device.sh to select a connected iPhone.}"
        sdk=iphoneos; destination="platform=iOS,id=$OSPREY_DEVICE_UDID"
        signing=("DEVELOPMENT_TEAM=$OSPREY_DEVELOPMENT_TEAM" -allowProvisioningUpdates -allowProvisioningDeviceRegistration)
        ;;
    *) echo "Usage: $0 [ios|ios-sim|ios-device]" >&2; exit 2 ;;
esac
compiler_target=${target/ios-device/ios}

compiler=${OSPREY_BIN:-"$repo_dir/target/release/osprey"}
if [[ ! -x "$compiler" ]]; then
    echo "Osprey compiler not found at $compiler. Run make ios from the repository root." >&2
    exit 1
fi
output="$example_dir/build/$target"
library_dir="$example_dir/build/$compiler_target"
mkdir -p "$output" "$library_dir"
"$compiler" "$example_dir/app.osp" --compile --target="$compiler_target" -o "$library_dir/libOspreyApp.a"
xcodebuild -quiet -project "$example_dir/OspreyCounter.xcodeproj" -scheme OspreyCounter \
    -configuration Debug -sdk "$sdk" -destination "$destination" \
    -derivedDataPath "$output/DerivedData" CONFIGURATION_BUILD_DIR="$output/products" \
    "${signing[@]}" build
echo "Built $output/products/OspreyCounter.app"
