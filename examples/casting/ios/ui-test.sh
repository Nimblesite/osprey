#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
bash "$here/build.sh" ios-sim
device=$(bash "$repo/scripts/ios-simulator.sh")
xcrun simctl install "$device" "$here/../build/ios-sim/Casting.app"
container=$(xcrun simctl get_app_container "$device" org.ospreylang.casting data)
mkdir -p "$container/Documents"
cp "$CAST_TEST_AUDIO_SOURCE" "$container/Documents/Casting mobile test.mp3"
export TEST_RUNNER_CAST_TEST_AUDIO="Casting mobile test.mp3"
export TEST_RUNNER_CAST_TEST_RECEIVER="$CAST_TEST_RECEIVER"
xcodegen generate --spec "$here/project.yml" --project "$here"
result="$here/../build/ios-test/results-$(date +%s).xcresult"
mkdir -p "$here/../build/ios-test"
printf '%s\n' "$result" > "$here/../build/ios-test/result-path.txt"
xcodebuild -project "$here/Casting.xcodeproj" -scheme Casting \
    -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$here/../build/ui-tests" \
    -resultBundlePath "$result" -parallel-testing-enabled NO -test-timeouts-enabled YES -maximum-test-execution-time-allowance 120 \
    CODE_SIGNING_ALLOWED=NO test
