#!/usr/bin/env bash
# Build the exact same Osprey project as iOS, then package its JNI transport.
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
app="$root/examples/mobile/android"
cd "$root"
source scripts/android-env.sh
make _runtime_android
if [[ -z ${OSPREY_BIN:-} ]]; then cargo build --release -p osprey-cli; fi
compiler=${OSPREY_BIN:-$root/target/release/osprey}
for slice in arm64 x64; do
    if [[ "$slice" == arm64 ]]; then abi=arm64-v8a; triple=aarch64-linux-android26; else abi=x86_64; triple=x86_64-linux-android26; fi
    output="$app/build/native/$abi"
    native="$app/build/generated/jniLibs/$abi"
    mkdir -p "$output" "$native"
    "$compiler" build examples/mobile/inbox --target="android-$slice" -o "$output/inbox.a"
    "$android_tools/clang" --target="$triple" -shared -fPIC -O2 -std=c11 \
        -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
        -Wl,--no-undefined -Wl,-z,max-page-size=16384 -I"$output" \
        "$app/app/src/main/cpp/bridge.c" "$output/inbox.a" -lm -ldl -llog \
        -o "$native/libosprey_inbox.so"
    "$android_tools/llvm-strip" --strip-unneeded "$native/libosprey_inbox.so"
done
cd "$app"
./gradlew --console=plain :app:assembleDebug
printf '\nAndroid APK: %s\n' "$app/app/build/outputs/apk/debug/app-debug.apk"
