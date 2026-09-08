#!/usr/bin/env bash
# Run the same scalar ABI fixture and language goldens as iOS, on Android.
# Implements [ANDROID-HOST-ABI] and [MOBILE-VERIFICATION].
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
source scripts/android-env.sh
compiler=${OSPREY_BIN:-$root/target/release/osprey}
adb="$android_sdk/platform-tools/adb"
serial=${OSPREY_ANDROID_SERIAL:-${ANDROID_SERIAL:-}}
if [[ -z "$serial" ]]; then serial=$("$adb" devices | awk 'NR>1 && $2=="device" {print $1}'); fi
[[ -n "$serial" && "$serial" != *$'\n'* ]] || { echo 'Select one Android device with OSPREY_ANDROID_SERIAL' >&2; exit 1; }
abi=$("$adb" -s "$serial" shell getprop ro.product.cpu.abi | tr -d '\r')
case "$abi" in
    arm64-v8a) target=android-arm64; triple=aarch64-linux-android26 ;;
    x86_64) target=android-x64; triple=x86_64-linux-android26 ;;
    *) echo "Unsupported Android ABI: $abi" >&2; exit 1 ;;
esac
scratch=$(mktemp -d "$root/compiler/bin/.android-tests.XXXXXX")
remote="/data/local/tmp/osprey-tests-$$"
cleanup() { rm -rf "$scratch"; "$adb" -s "$serial" shell rm -rf "$remote" >/dev/null; }
trap cleanup EXIT
"$adb" -s "$serial" shell mkdir -p "$remote"
cp scripts/mobile-abi.osp "$scratch/abi.osp"
cp scripts/mobile-abi.c "$scratch/abi.c"
link_host() {
    "$android_tools/clang" --target="$triple" -O2 -std=c11 -Wall -Wextra -Werror "$1" "$2" -lm -ldl -o "$3"
}
run_host() {
    "$adb" -s "$serial" push "$1" "$remote/host" >/dev/null
    "$adb" -s "$serial" shell chmod 755 "$remote/host"
    if ! "$adb" -s "$serial" shell "cd $remote && ./host" >"$scratch/actual"; then cat "$scratch/actual"; return 1; fi
}
"$compiler" "$scratch/abi.osp" --target="$target" --compile -o "$scratch/abi.a"
link_host "$scratch/abi.c" "$scratch/abi.a" "$scratch/abi"
run_host "$scratch/abi"
printf 'Mobile C ABI passed\n' >"$scratch/expected"
diff -u "$scratch/expected" "$scratch/actual"
echo "==> Android scalar imports/exports, bool, Unit and persistent global ABI passed ($abi)"
# The seven hand-picked goldens that used to run here are gone: `make
# _test_android_goldens` now runs the WHOLE corpus on this device against the
# same byte-exact goldens, with its rejections pinned in
# tests/MOBILE_UNPORTABLE.txt. Repeating fourteen of them here would observe a
# strict subset of what that harness already observed.

cat >"$scratch/golden.c" <<'C'
#include "golden.h"
int main(void) { return osprey_main(); }
C

# Run the same app tests on the device ABI, including the UTF-8 regressions.
"$compiler" examples/mobile/inbox/test --target="$target" --compile -o "$scratch/golden.a"
link_host "$scratch/golden.c" "$scratch/golden.a" "$scratch/golden"
run_host "$scratch/golden"
cat "$scratch/actual"
echo "==> Android shared mobile domain suite passed ($abi)"
