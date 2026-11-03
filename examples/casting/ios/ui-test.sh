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
export TEST_RUNNER_CAST_TEST_NATIVE_HOST="$CAST_TEST_NATIVE_HOST"
export TEST_RUNNER_CAST_TEST_NATIVE_PORT="$CAST_TEST_NATIVE_PORT"
xcodegen generate --spec "$here/project.yml" --project "$here"
result="$here/../build/ios-test/results-$(date +%s).xcresult"
mkdir -p "$here/../build/ios-test"
coverage="$here/../build/ios-test/coverage.json"
rm -f "$coverage" "$coverage.partial"
printf '%s\n' "$result" > "$here/../build/ios-test/result-path.txt"
xcodebuild -project "$here/Casting.xcodeproj" -scheme Casting \
    -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$here/../build/ui-tests" \
    -resultBundlePath "$result" -enableCodeCoverage YES -parallel-testing-enabled NO -collect-test-diagnostics never -test-timeouts-enabled YES -maximum-test-execution-time-allowance 120 \
    CODE_SIGNING_ALLOWED=NO test
xcrun xccov view --report --json "$result" > "$coverage.partial"
python3 - "$coverage.partial" <<'PY'
import json,sys
report=json.load(open(sys.argv[1]))
apps=[target for target in report['targets'] if target['name']=='Casting.app']
assert len(apps)==1, 'Expected one native casting coverage target'
app=apps[0]
assert type(app['coveredLines']) is int and type(app['executableLines']) is int
assert 0 < app['coveredLines'] <= app['executableLines'], 'Missing native application coverage'
assert isinstance(app['files'],list) and app['files'], 'Missing native source-file coverage'
print(f"Collected iOS app coverage: {app['coveredLines']}/{app['executableLines']} lines")
PY
mv "$coverage.partial" "$coverage"
