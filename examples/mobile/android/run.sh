#!/usr/bin/env bash
# Install on a connected phone/emulator; smoke proves live API + SQLite + reactive UI.
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
source "$root/scripts/android-env.sh"
app="$root/examples/mobile/android"
if [[ ${OSPREY_ANDROID_SKIP_BUILD:-0} != 1 ]]; then bash "$app/build.sh"; fi
adb="$android_sdk/platform-tools/adb"
serial=${OSPREY_ANDROID_SERIAL:-${ANDROID_SERIAL:-}}
if [[ -z "$serial" ]]; then
    serial=$("$adb" devices | awk 'NR>1 && $2=="device" {print $1}')
    [[ -n "$serial" && "$serial" != *$'\n'* ]] || { echo 'Connect one Android device or set OSPREY_ANDROID_SERIAL (emulator: launch an AVD in Android Studio).' >&2; exit 1; }
fi
package=org.ospreylang.issueinbox
"$adb" -s "$serial" install -r "$app/app/build/outputs/apk/debug/app-debug.apk"
if [[ ${1:-} == --smoke || ${1:-} == --live-smoke ]]; then
    live=false
    if [[ ${1:-} == --live-smoke ]]; then live=true; fi
    "$adb" -s "$serial" shell am force-stop "$package"
    "$adb" -s "$serial" shell run-as "$package" rm -f databases/inbox-smoke.sqlite databases/inbox-smoke.sqlite-journal files/mobile-smoke-id files/mobile-smoke-fresh.json files/mobile-smoke-restore.json
    for phase in fresh restore; do
        "$adb" -s "$serial" shell am force-stop "$package"
        "$adb" -s "$serial" shell am start -W -n "$package/.MainActivity" --es smoke_phase "$phase" --ez live "$live"
        found=0
        for attempt in $(seq 1 60); do
            if "$adb" -s "$serial" shell run-as "$package" cat "files/mobile-smoke-$phase.json" >"$app/build/smoke-$phase.json" 2>/dev/null; then found=1; break; fi
            sleep 1
        done
        [[ $found == 1 ]] || { "$adb" -s "$serial" logcat -d -s OspreyInbox AndroidRuntime; echo "Android $phase smoke timed out" >&2; exit 1; }
        python3 - "$app/build/smoke-$phase.json" <<'PY'
import json, sys
result = json.load(open(sys.argv[1]))
if not result['ok']: sys.exit(result)
print('OSPREY_ANDROID_SMOKE_OK', result['phase'], result['source'], 'issues=', result['view']['total'], 'saved=', result['view']['bookmarked'])
PY
    done
fi
"$adb" -s "$serial" shell am force-stop "$package"
"$adb" -s "$serial" shell am start -W -n "$package/.MainActivity"
printf '\nRunning Osprey Inbox on %s\n' "$serial"
