#!/usr/bin/env bash
# Implements [IOS-VERIFICATION]: launch the real SwiftUI app and inspect its result.
set -euo pipefail

example_dir=$(cd "$(dirname "$0")" && pwd)
mode=${1:-}
if [[ -n "$mode" && "$mode" != --smoke ]]; then
    echo "Usage: $0 [--smoke]" >&2
    exit 2
fi
if [[ ${OSPREY_IOS_SKIP_BUILD:-0} != 1 ]]; then "$example_dir/build.sh" ios-sim; fi

device=$("$example_dir/../../scripts/ios-simulator.sh")
bundle_id=org.ospreylang.OspreyCounter
xcrun simctl install "$device" "$example_dir/build/ios-sim/products/OspreyCounter.app"

if [[ "$mode" == --smoke ]]; then
    container=$(xcrun simctl get_app_container "$device" "$bundle_id" data)
    result="$container/Documents/smoke-result.txt"
    rm -f "$result"
    xcrun simctl launch --terminate-running-process "$device" "$bundle_id" --osprey-smoke
    for ((attempt = 0; attempt < 60; attempt++)); do
        [[ -f "$result" ]] && break
        sleep 1
    done
    if [[ ! -f "$result" ]]; then echo "Timed out waiting for iOS smoke result" >&2; exit 1; fi
    cat "$result"
    [[ $(cat "$result") == OSPREY_IOS_SMOKE_OK ]]
else
    open -a Simulator
    xcrun simctl launch --terminate-running-process "$device" "$bundle_id"
fi
