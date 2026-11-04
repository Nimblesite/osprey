#!/usr/bin/env bash
# Implements [IOS-VERIFICATION]: launch the real SwiftUI app and inspect its result.
set -euo pipefail

script_dir=$(cd "$(dirname "$0")" && pwd)
example_dir=${OSPREY_IOS_EXAMPLE_DIR:-$script_dir}
scheme=${OSPREY_IOS_SCHEME:-OspreyCounter}
mode=${1:-}
if [[ -n "$mode" && "$mode" != --smoke ]]; then
    echo "Usage: $0 [--smoke]" >&2
    exit 2
fi
if [[ ${OSPREY_IOS_SKIP_BUILD:-0} != 1 ]]; then "$script_dir/build.sh" ios-sim; fi

device=$("$script_dir/../../scripts/ios-simulator.sh")
bundle_id=${OSPREY_IOS_BUNDLE_ID:-org.ospreylang.OspreyCounter}
xcrun simctl install "$device" "$example_dir/build/ios-sim/products/$scheme.app"

if [[ "$mode" == --smoke ]]; then
    container=$(xcrun simctl get_app_container "$device" "$bundle_id" data)
    result="$container/Documents/${OSPREY_IOS_SMOKE_FILE:-smoke-result.txt}"
    rm -f "$result"
    xcrun simctl launch --terminate-running-process "$device" "$bundle_id" "${OSPREY_IOS_SMOKE_ARG:---osprey-smoke}"
    for ((attempt = 0; attempt < 60; attempt++)); do
        [[ -f "$result" ]] && break
        sleep 1
    done
    if [[ ! -f "$result" ]]; then echo "Timed out waiting for iOS smoke result" >&2; exit 1; fi
    cat "$result"
    [[ $(cat "$result") == "${OSPREY_IOS_SMOKE_EXPECT:-OSPREY_IOS_SMOKE_OK}" ]]
else
    open -a Simulator
    xcrun simctl launch --terminate-running-process "$device" "$bundle_id"
fi
