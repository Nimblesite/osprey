#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../../../.." && pwd)
app="$root/examples/projects/modules/mobile/android"
cd "$root"
source scripts/android-env.sh
if [[ ${OSPREY_ANDROID_SKIP_RUNTIME:-0} != 1 ]]; then make _runtime_android; fi
if [[ -z ${OSPREY_BIN:-} ]]; then cargo build --release -p osprey-cli; fi
compiler=${OSPREY_BIN:-$root/target/release/osprey}
for slice in ${OSPREY_ANDROID_SLICES:-arm64 x64}; do
    if [[ "$slice" == arm64 ]]; then abi=arm64-v8a; triple=aarch64-linux-android26; else abi=x86_64; triple=x86_64-linux-android26; fi
    output="$app/build/native/$abi"
    native="$app/build/generated/jniLibs/$abi"
    mkdir -p "$output" "$native"
    "$compiler" build examples/projects/modules/mobile/app --target="android-$slice" -o "$output/talon.a"
    "$android_tools/clang" --target="$triple" -shared -fPIC -O2 -std=c11 \
        -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
        -Wl,--no-undefined -Wl,-z,max-page-size=16384 -I"$output" \
        "$app/app/src/main/cpp/bridge.c" "$output/talon.a" -lm -ldl -llog \
        -o "$native/libosprey_talon.so"
    "$android_tools/llvm-strip" --strip-unneeded "$native/libosprey_talon.so"
done
cd "$app"
./gradlew --console=plain :app:assembleDebug :app:assembleDebugAndroidTest
printf '\nTalon Android APK: %s\n' "$app/app/build/outputs/apk/debug/app-debug.apk"
