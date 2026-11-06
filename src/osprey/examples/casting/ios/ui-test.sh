#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
bash "$here/build.sh" ios-sim
device=$(bash "$repo/scripts/mobile/ios-simulator.sh")
# Each suite owns a fresh test app; it seeds its documents below and exercises
# process-restart persistence within the suite. Stored UIKit scene delegates
# from an earlier app renderer can otherwise survive replacing its executable.
python3 - "$device" <<'CLEAN'
import json,subprocess,sys
device=sys.argv[1]
raw=subprocess.check_output(['xcrun','simctl','listapps',device])
apps=json.loads(subprocess.check_output(['plutil','-convert','json','-o','-','-'],input=raw))
if 'org.ospreylang.casting' in apps:
    subprocess.run(['xcrun','simctl','uninstall',device,'org.ospreylang.casting'],check=True)
CLEAN
xcrun simctl install "$device" "$here/../build/ios-sim/Casting.app"
container=$(xcrun simctl get_app_container "$device" org.ospreylang.casting data)
mkdir -p "$container/Documents"
cp "$CAST_TEST_AUDIO_SOURCE" "$container/Documents/Casting mobile test.mp3"
export TEST_RUNNER_CAST_TEST_AUDIO="Casting mobile test.mp3"
export TEST_RUNNER_CAST_TEST_AUDIO_PATH="$container/Documents/Casting mobile test.mp3"
# The shared GPUI host stores preferences under the app's HOME, using the Linux
# convention on iOS: $HOME/.local/share/osprey/<app>.
export TEST_RUNNER_CAST_TEST_APPEARANCE_PATH="$container/.local/share/osprey/osprey/casting-appearance.txt"
export TEST_RUNNER_CAST_TEST_RECEIVER="$CAST_TEST_RECEIVER"
export TEST_RUNNER_CAST_TEST_NATIVE_HOST="$CAST_TEST_NATIVE_HOST"
export TEST_RUNNER_CAST_TEST_NATIVE_PORT="$CAST_TEST_NATIVE_PORT"
xcodegen generate --spec "$here/project.yml" --project "$here"
result="$here/../build/ios-test/results-$(date +%s).xcresult"
mkdir -p "$here/../build/ios-test"
coverage="$here/../build/ios-test/coverage.json"
rm -f "$coverage" "$coverage.partial"
printf '%s\n' "$result" > "$here/../build/ios-test/result-path.txt"
test_status=0
xcodebuild -project "$here/Casting.xcodeproj" -scheme Casting \
    -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$here/../build/gpui-ui-tests" \
    -resultBundlePath "$result" -enableCodeCoverage YES -parallel-testing-enabled NO -collect-test-diagnostics never -test-timeouts-enabled YES -maximum-test-execution-time-allowance 120 \
    CODE_SIGNING_ALLOWED=NO test || test_status=$?
# XCTest can reinstall a matching app from DerivedProducts. A retired renderer
# there must never turn this GPUI journey green, even when its assertions pass.
installed=$(xcrun simctl get_app_container "$device" org.ospreylang.casting app)
if ! cmp -s "$here/../build/ios-sim/Casting.app/Casting" "$installed/Casting"; then
    echo 'FAIL: XCTest ran a different executable from the shared GPUI build' >&2
    exit 1
fi
[[ $test_status -eq 0 ]] || exit "$test_status"
xcrun xccov view --report --json "$result" > "$coverage.partial"
# [CAST-MOBILE-GPUI] The app is a Rust src/toolchain/hosts/gpui binary built outside Xcode,
# so xccov cannot see it until the Rust coverage pipeline is wired in here.
# The gate must fail clearly rather than fabricate app coverage; it passes
# unchanged once Casting.app coverage lands in the report.
python3 - "$coverage.partial" <<'PY'
import json,sys
report=json.load(open(sys.argv[1]))
apps=[target for target in report['targets'] if target['name']=='Casting.app']
assert len(apps)<=1, 'Expected at most one native casting coverage target'
if not apps:
    sys.exit('FAIL: Casting.app has no coverage in the Xcode report. The GPUI app is a '
             'Rust binary built by build.sh outside Xcode; Rust coverage integration is '
             'pending. Wire the Rust profdata/lcov export into this gate instead of '
             'weakening it.')
app=apps[0]
assert type(app['coveredLines']) is int and type(app['executableLines']) is int
assert 0 < app['coveredLines'] <= app['executableLines'], 'Missing native application coverage'
assert isinstance(app['files'],list) and app['files'], 'Missing native source-file coverage'
print(f"Collected iOS app coverage: {app['coveredLines']}/{app['executableLines']} lines")
PY
mv "$coverage.partial" "$coverage"
