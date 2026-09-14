#!/usr/bin/env zsh
# How one program is executed on each backend, and the one-time setup each
# needs. Sourced by run_test_corpus.sh, which owns the corpus, the goldens and
# the ratchets; this file owns only "run this program and give me its stdout,
# stderr and exit status".
#
# Split out when the harness passed 500 lines. The seam is real rather than
# arbitrary: adding a target means adding a `run_*` function and its setup
# here, and touching nothing that decides what a result MEANS.

# Status sentinel for a program the wasm32 target deliberately rejects.
SKIP_STATUS=skip

# Exit code run_wasm uses to say "not a failure, an unported feature". Distinct
# from any status the compiler or Node can return on its own.
SKIP_CODE=200

# Extract the compiler's explicit rejection of a program on this target, in
# either shape it takes: an unsupported capability, or a boundary the mobile C
# ABI cannot express. Unexpected LLVM/linker errors match NEITHER and so remain
# failures: target legality must be decided by the compiler before it emits
# code, never by a linker afterwards.
# [WASM-TARGET-CAPABILITIES] [IOS-TARGET-CAPABILITIES] [IOS-HOST-ABI]
target_rejection() {
  local reason
  reason=$(sed -n "s/^.*target \`$TARGET\` does not support \(.*\); use a supported target.*\$/\1/p" "$1" | head -1)
  [[ -z $reason ]] && reason=$(sed -n "s/^.*target \`$TARGET\` C ABI: \(.*\)\$/C ABI: \1/p" "$1" | head -1)
  print -r -- "${reason% near line <->:<->}"
}

# Compile and execute under Node's WASI host. Only explicit capability errors
# may skip execution; the exact program and reason remain pinned below.
run_wasm() {
  local file=$1 out=$2 err=$3 module=$4
  if ! $BIN "$file" --target=wasm32 --compile -o "$module" >"$err" 2>&1; then
    [[ -n $(target_rejection "$err") ]] && return $SKIP_CODE
    return 1
  fi
  "$NODE" "$SMOKE" "$module" >"$out" 2>"$err"
}

# The C host every mobile archive is linked into. It supplies the executable
# entry the library deliberately does not have, and chdir's into the program's
# own scratch directory so file-writing programs cannot collide when the corpus
# runs in parallel. [IOS-TARGET-ENTRY]
MOBILE_HOST_C='#include "golden.h"
#include <stdio.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2 || chdir(argv[1]) != 0) { perror("golden working directory"); return 1; }
    return osprey_main();
}'

# Build the mobile C ABI archive, link it against that host and run the result
# in the simulator. A capability or boundary rejection skips; anything else --
# a failed clang, a failed link, a non-zero exit -- is a failure.
run_ios() {
  local file=$1 out=$2 err=$3 work=$4
  mkdir -p "$work" || return 1
  if ! $BIN "$file" --target=ios-sim --compile -o "$work/golden.a" >"$err" 2>&1; then
    [[ -n $(target_rejection "$err") ]] && return $SKIP_CODE
    return 1
  fi
  print -r -- "$MOBILE_HOST_C" >"$work/host.c"
  xcrun --sdk iphonesimulator clang -target arm64-apple-ios15.0-simulator \
    -isysroot "$IOS_SDK" -O2 -std=c11 -I"$work" \
    "$work/host.c" "$work/golden.a" -o "$work/host" >>"$err" 2>&1 || return 1
  xcrun simctl spawn "$IOS_DEVICE" "$work/host" "$work" >"$out" 2>>"$err" </dev/null
}

# The same archive/link/run cycle against a real Android device or emulator.
# `adb shell` is used rather than `exec-out` because only `shell` reports the
# program's exit status back to the harness; `exec-out` returns 0 whatever the
# program did, which would turn every failing assertion into a silent pass.
run_android() {
  local file=$1 out=$2 err=$3 work=$4 index=$5
  local -a adb=("$ANDROID_ADB_BIN" -s "$ANDROID_SERIAL")
  mkdir -p "$work" || return 1
  if ! $BIN "$file" --target=$TARGET --compile -o "$work/golden.a" >"$err" 2>&1; then
    [[ -n $(target_rejection "$err") ]] && return $SKIP_CODE
    return 1
  fi
  print -r -- "$MOBILE_HOST_C" >"$work/host.c"
  "$ANDROID_TOOLS/clang" --target=$ANDROID_TRIPLE -O2 -std=c11 -I"$work" \
    "$work/host.c" "$work/golden.a" -lm -ldl -o "$work/host" >>"$err" 2>&1 || return 1
  local remote=$ANDROID_REMOTE/$index
  $adb shell "mkdir -p $remote" >>"$err" 2>&1 || return 1
  $adb push "$work/host" "$remote/host" >>"$err" 2>&1 || return 1
  $adb shell "cd $remote && ./host $remote" >"$out" 2>>"$err"
}

# Resolve the device and toolchain a cross target needs, exactly once. Called
# by the harness AFTER its `--worker` branch has exited, because a worker
# re-executes the script: doing this at source time would boot a simulator and
# interrogate adb once per program instead of once per run.
backend_setup() {
  local sdk_root adb_bin serial abi device_target
  # One simulator and one SDK path resolved ONCE, then inherited by every worker.
  # Booting per program would serialize the run behind simctl and make a device
  # that failed to boot look like every program failing. `ios-simulator.sh` boots
  # and waits; a failure here is fatal rather than a corpus of red programs.
  if [[ $TARGET == ios-sim ]]; then
    IOS_DEVICE=${OSPREY_IOS_DEVICE:-$(bash "$ROOT/scripts/ios-simulator.sh")} || exit 1
    IOS_SDK=${OSPREY_IOS_SDK:-$(xcrun --sdk iphonesimulator --show-sdk-path)} || exit 1
    [[ -n $IOS_DEVICE && -d $IOS_SDK ]] || { echo "no iPhone simulator or simulator SDK available" >&2; exit 1 }
    export IOS_DEVICE IOS_SDK
  fi

  # One device, toolchain and remote directory resolved ONCE, then inherited by
  # every worker. The device's own ABI is checked against the requested target:
  # pushing an arm64 binary to an x86-64 emulator fails per program and would
  # read as the whole corpus being broken rather than as one wrong flag.
  if [[ $TARGET == android* ]]; then
    sdk_root=$(bash -c 'source "$1"/scripts/android-env.sh >/dev/null 2>&1; printf %s "$ANDROID_HOME"' _ "$ROOT")
    ANDROID_TOOLS=$(bash -c 'source "$1"/scripts/android-env.sh >/dev/null 2>&1; printf %s "$android_tools"' _ "$ROOT")
    adb_bin=${OSPREY_ADB:-$sdk_root/platform-tools/adb}
    [[ -x $adb_bin && -x $ANDROID_TOOLS/clang ]] || { echo "Android SDK/NDK not found; see scripts/android-env.sh" >&2; exit 1 }
    serial=${OSPREY_ANDROID_SERIAL:-${ANDROID_SERIAL:-$($adb_bin devices | awk 'NR>1 && $2=="device" {print $1}')}}
    [[ -n $serial && $serial != *$'\n'* ]] || { echo "Select one Android device with OSPREY_ANDROID_SERIAL" >&2; exit 1 }
    ANDROID_ADB=("$adb_bin" -s "$serial")
    abi=$($ANDROID_ADB shell getprop ro.product.cpu.abi | tr -d '\r')
    # `OSPREY_TARGET=android` means "whichever slice this device runs", so one
    # command works on an arm64 phone and an x86-64 emulator alike. Naming a
    # slice explicitly still has to match the hardware: pushing the wrong
    # architecture fails per program and would read as a broken corpus.
    case $abi in
      arm64-v8a) device_target=android-arm64; ANDROID_TRIPLE=aarch64-linux-android26 ;;
      x86_64)    device_target=android-x64;   ANDROID_TRIPLE=x86_64-linux-android26 ;;
      *) echo "unsupported Android ABI: $abi" >&2; exit 1 ;;
    esac
    if [[ $TARGET != android && $TARGET != $device_target ]]; then
      echo "device ABI $abi runs $device_target, not $TARGET" >&2; exit 1
    fi
    TARGET=$device_target
    export OSPREY_TARGET=$TARGET
    ANDROID_REMOTE=/data/local/tmp/osprey-corpus-$$
    $ANDROID_ADB shell "mkdir -p $ANDROID_REMOTE" >/dev/null || exit 1
    # Workers re-exec this script, so the array is passed as a plain string and
    # re-split there; `ANDROID_SERIAL` is what adb itself reads.
    export ANDROID_SERIAL=$serial ANDROID_ADB_BIN=$adb_bin
    export ANDROID_TOOLS ANDROID_TRIPLE ANDROID_REMOTE
  fi
}
