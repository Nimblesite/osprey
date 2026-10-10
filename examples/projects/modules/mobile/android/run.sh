#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../../../.." && pwd)
source "$root/scripts/android-env.sh"
app="$root/examples/projects/modules/mobile/android"
if [[ ${OSPREY_ANDROID_SKIP_BUILD:-0} != 1 ]]; then bash "$app/build.sh"; fi
adb="$android_sdk/platform-tools/adb"
serial=${OSPREY_ANDROID_SERIAL:-${ANDROID_SERIAL:-}}
if [[ -z "$serial" ]]; then
    serial=$("$adb" devices | awk 'NR>1 && $2=="device" {print $1}')
    [[ -n "$serial" && "$serial" != *$'\n'* ]] || { echo 'Connect one Android device or set OSPREY_ANDROID_SERIAL.' >&2; exit 1; }
fi
"$adb" -s "$serial" reverse tcp:18790 tcp:18790
"$adb" -s "$serial" install -r "$app/app/build/outputs/apk/debug/app-debug.apk"
if [[ ${1:-} == --test ]]; then
    # Keep coordinate taps/screenshots deterministic, then restore this device's
    # animation settings even if instrumentation fails.
    animation_keys=(window_animation_scale transition_animation_scale animator_duration_scale)
    animation_values=()
    for key in "${animation_keys[@]}"; do
        animation_values+=("$("$adb" -s "$serial" shell settings get global "$key" | tr -d '\r')")
        "$adb" -s "$serial" shell settings put global "$key" 0
    done
    restore_animations() {
        for index in "${!animation_keys[@]}"; do
            if [[ ${animation_values[$index]} == null ]]; then
                "$adb" -s "$serial" shell settings delete global "${animation_keys[$index]}" >/dev/null
            else
                "$adb" -s "$serial" shell settings put global "${animation_keys[$index]}" "${animation_values[$index]}"
            fi
        done
    }
    trap restore_animations EXIT
    "$adb" -s "$serial" install -r -t "$app/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
    "$adb" -s "$serial" shell run-as org.ospreylang.talon rm -rf files/screenshots
    report="$app/build/instrumentation.txt"
    "$adb" -s "$serial" shell am instrument -w -r -e server_url "${TALON_SERVER_URL:-http://127.0.0.1:18790}" org.ospreylang.talon.test/android.test.InstrumentationTestRunner | tee "$report"
    mkdir -p "$app/build/screenshots"
    for screen in overview accounts deposit refusal activity security open-account landscape offline; do
        "$adb" -s "$serial" exec-out run-as org.ospreylang.talon cat "files/screenshots/$screen.png" > "$app/build/screenshots/$screen.png" 2>/dev/null || rm -f "$app/build/screenshots/$screen.png"
    done
    python3 - "$app/build/screenshots" <<'PY'
import pathlib, sys
for path in pathlib.Path(sys.argv[1]).glob('*.png'):
    if not path.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'):
        path.unlink()
PY
    if ! grep -Eq '^OK \([0-9]+ tests?\)' "$report" || grep -Eq 'FAILURES|INSTRUMENTATION_FAILED|shortMsg=' "$report"; then exit 1; fi
else
    "$adb" -s "$serial" shell am start -W -n org.ospreylang.talon/.MainActivity --es server_url "${TALON_SERVER_URL:-http://127.0.0.1:18790}"
fi
