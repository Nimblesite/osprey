#!/usr/bin/env bash
# Implements [IOS-SWIFT-HOST] and [IOS-VERIFICATION].
set -euo pipefail

example_dir=$(cd "$(dirname "$0")" && pwd)
repo_dir=$(cd "$example_dir/../.." && pwd)
target=${1:-ios-sim}
case "$target" in
    ios) sdk=iphoneos; destination='generic/platform=iOS' ;;
    ios-sim) sdk=iphonesimulator; destination='generic/platform=iOS Simulator' ;;
    *) echo "Usage: $0 [ios|ios-sim]" >&2; exit 2 ;;
esac

compiler=${OSPREY_BIN:-"$repo_dir/target/release/osprey"}
if [[ ! -x "$compiler" ]]; then
    echo "Osprey compiler not found at $compiler. Run make ios from the repository root." >&2
    exit 1
fi
output="$example_dir/build/$target"
mkdir -p "$output"
"$compiler" "$example_dir/app.osp" --compile --target="$target" -o "$output/libOspreyApp.a"
xcodebuild -quiet -project "$example_dir/OspreyCounter.xcodeproj" -scheme OspreyCounter \
    -configuration Debug -sdk "$sdk" -destination "$destination" \
    -derivedDataPath "$output/DerivedData" CONFIGURATION_BUILD_DIR="$output/products" \
    CODE_SIGNING_ALLOWED=NO build
echo "Built $output/products/OspreyCounter.app"
