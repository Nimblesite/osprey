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
cat >"$scratch/golden.c" <<'C'
#include "golden.h"
int main(void) { return osprey_main(); }
C
goldens=(
    tests/core/arithmetic/calculator.test
    tests/core/collections/list_basics.test
    tests/core/collections/map_basics.test
    tests/regressions/basics/strings/string_pipeline.test
    tests/regressions/basics/json/json_document_query.test
    tests/regressions/basics/files/file_io_json_workflow.test
    tests/regressions/fiber/fiber_showcase.test
)
for base in "${goldens[@]}"; do
    for flavor in osp ospml; do
        "$compiler" "$base.$flavor" --target="$target" --compile -o "$scratch/golden.a"
        link_host "$scratch/golden.c" "$scratch/golden.a" "$scratch/golden"
        run_host "$scratch/golden"
        diff -u "$base.osp.expectedoutput" "$scratch/actual"
        echo "==> Android golden passed: $base.$flavor"
    done
done
