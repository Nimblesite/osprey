#!/usr/bin/env bash
set -euo pipefail
example_dir=$(cd "$(dirname "$0")" && pwd)
repo_dir=$(cd "$example_dir/../../../../.." && pwd)
if [[ ${OSPREY_IOS_SKIP_BUILD:-0} != 1 ]]; then "$example_dir/run.sh" --build ios-sim; fi
device=$("$repo_dir/scripts/ios-simulator.sh")
result="$example_dir/build/ios-sim/TestResults-$(date +%Y%m%d-%H%M%S).xcresult"
export TALON_TEST_SERVER_URL=${TALON_TEST_SERVER_URL:-http://127.0.0.1:18790}
xcodebuild -project "$example_dir/TalonBank.xcodeproj" -scheme TalonBank \
    -configuration Debug -sdk iphonesimulator -destination "platform=iOS Simulator,id=$device" \
    -derivedDataPath "$example_dir/build/ios-sim/DerivedData" -resultBundlePath "$result" \
    "TALON_TEST_SERVER_URL=$TALON_TEST_SERVER_URL" CODE_SIGNING_ALLOWED=NO test
echo "Talon iOS test results and screenshots: $result"
