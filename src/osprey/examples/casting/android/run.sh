#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
source "$repo/scripts/mobile/android-env.sh"
adb="$ANDROID_HOME/platform-tools/adb"
serial=${OSPREY_ANDROID_SERIAL:?Set OSPREY_ANDROID_SERIAL to the emulator or device to run}
architecture=$("$adb" -s "$serial" shell getprop ro.product.cpu.abi | tr -d '\r')
case "$architecture" in arm64-v8a|x86_64) ;; *) echo "Unsupported Android ABI: $architecture" >&2; exit 2 ;; esac
case "$architecture" in arm64-v8a) target=android-arm64 ;; x86_64) target=android-x64 ;; esac
apk=$(bash "$here/build.sh" "$target" | tail -1)
"$adb" -s "$serial" install -r "$apk"
"$adb" -s "$serial" shell am start -n org.ospreylang.casting/.CastingActivity
