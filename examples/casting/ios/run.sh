#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
bash "$here/build.sh" ios-sim
device=$(bash "$repo/scripts/ios-simulator.sh")
xcrun simctl install "$device" "$here/../build/ios-sim/Casting.app"
xcrun simctl launch --terminate-running-process "$device" org.ospreylang.casting
