#!/usr/bin/env bash
# Install the host's APK on the device named by OSPREY_ANDROID_SERIAL and
# either open the application or run its headless view check, printing the
# verdict line. Modelled on src/osprey/examples/mobile/android/run.sh.
#   run.sh [--view-check|--golden]
# `--golden` opens the application, waits for it to settle and compares the screen, without its
# system bars, with the application's `goldens/android/<width>-ready.png` (scripts/mobile/goldens.py).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
source "$repo/scripts/mobile/android-env.sh"
if [[ ${OSPREY_ANDROID_SKIP_BUILD:-0} != 1 ]]; then bash "$here/build.sh"; fi
adb="$android_sdk/platform-tools/adb"
serial=${OSPREY_ANDROID_SERIAL:-${ANDROID_SERIAL:-}}
if [[ -z "$serial" ]]; then
    serial=$("$adb" devices | awk 'NR>1 && $2=="device" {print $1}')
    [[ -n "$serial" && "$serial" != *$'\n'* ]] || { echo 'Connect one Android device or set OSPREY_ANDROID_SERIAL (emulator: launch an AVD in Android Studio).' >&2; exit 1; }
fi
id=${OSPREY_ANDROID_ID:-org.ospreylang.tally}
app=${OSPREY_ANDROID_APP:-"$repo/src/osprey/examples/tally"}
case "$app" in /*) ;; *) app="$repo/$app" ;; esac
apk="$here/host/build/outputs/apk/debug/host-debug.apk"
"$adb" -s "$serial" install -r "$apk"
activity="$id/org.ospreylang.host.HostActivity"
"$adb" -s "$serial" shell am force-stop "$id"
if [[ ${1:-} == --view-check ]]; then
    # An earlier run's verdict must not answer for this one.
    "$adb" -s "$serial" shell run-as "$id" rm -f files/view-check.txt
    "$adb" -s "$serial" shell am start -W -n "$activity" --ez view_check true >/dev/null
    for attempt in $(seq 1 60); do
        verdict=$("$adb" -s "$serial" shell run-as "$id" cat files/view-check.txt 2>/dev/null | tr -d '\r' || true)
        if [[ "$verdict" == OSPREY_ANDROID_VIEW_* ]]; then
            printf '%s\n' "$verdict"
            [[ "$verdict" == OSPREY_ANDROID_VIEW_OK* ]]
            exit
        fi
        sleep 1
    done
    "$adb" -s "$serial" logcat -d -s OspreyHost AndroidRuntime
    echo 'Android view check timed out' >&2
    exit 1
fi
if [[ ${1:-} == --golden ]]; then
    # The golden is the day theme with nothing over it: a cold emulator may still show a system
    # dialog, so it is closed and the screen is taken once Tally has focus and has stopped changing.
    "$adb" -s "$serial" shell cmd uimode night no >/dev/null
    "$adb" -s "$serial" shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null
fi
"$adb" -s "$serial" shell am start -W -n "$activity"
if [[ ${1:-} == --golden ]]; then
    for attempt in $(seq 1 30); do
        "$adb" -s "$serial" shell dumpsys window | grep -q "mCurrentFocus=.*$id" && break
        sleep 1
    done
    shot="$here/build/ready.png"
    "$adb" -s "$serial" exec-out screencap -p >"$shot"
    for attempt in $(seq 1 20); do
        sleep 1
        "$adb" -s "$serial" exec-out screencap -p >"$shot.next"
        cmp -s "$shot" "$shot.next" && break
        mv "$shot.next" "$shot"
    done
    rm -f "$shot.next"
    cd "$repo" && exec python3 scripts/mobile/goldens.py "$shot" "$app/goldens/android"
fi
printf '\nRunning %s on %s\n' "$id" "$serial"
