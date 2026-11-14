#!/usr/bin/env bash
# Build any Osprey Native application into the Android view host. [NATIVE-SHM-VIEW]
# The host leases the frame the application writes, checks every row and draws
# it in place with the platform's own views; no wire format crosses between.
#   OSPREY_ANDROID_APP=<application folder> \
#   [OSPREY_ANDROID_ID=<package id>] [OSPREY_ANDROID_NAME=<label>] build.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
app=${OSPREY_ANDROID_APP:-"$repo/src/osprey/examples/tally"}
case "$app" in /*) ;; *) app="$repo/$app" ;; esac
id=${OSPREY_ANDROID_ID:-org.ospreylang.tally}
name=${OSPREY_ANDROID_NAME:-Tally}

cd "$repo"
source scripts/mobile/android-env.sh
make _runtime_android_arm64_arc _runtime_android_x64_arc
compiler=${OSPREY_BIN:-$repo/target/release/osprey}
if [[ ! -x "$compiler" ]]; then cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli; fi

for slice in arm64 x64; do
    if [[ "$slice" == arm64 ]]; then abi=arm64-v8a; triple=aarch64-linux-android26; else abi=x86_64; triple=x86_64-linux-android26; fi
    output="$here/build/native/$abi"
    native="$here/build/generated/jniLibs/$abi"
    mkdir -p "$output" "$native"
    "$compiler" build "$app" --target="android-$slice" --memory=arc -o "$output/app.a"
    "$android_tools/clang" --target="$triple" -shared -fPIC -O2 -std=c11 \
        -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
        -Wl,--no-undefined -Wl,-z,max-page-size=16384 \
        -I"$output" -I"$repo/src/toolchain/compiler/runtime" \
        "$here/host/src/main/cpp/bridge.c" "$output/app.a" -lm -ldl -llog \
        -o "$native/libosprey_host.so"
    "$android_tools/llvm-strip" --strip-unneeded "$native/libosprey_host.so"
done

assets="$here/host/src/main/assets"
rm -rf "$assets"
mkdir -p "$assets"
if [[ -d "$app/assets" ]]; then cp -R "$app/assets/." "$assets/"; fi

"$repo/scripts/mobile/gradlew" -p "$here" -Posprey.id="$id" -Posprey.name="$name" \
    --console=plain :host:assembleDebug
printf '\nAndroid APK: %s\n' "$here/host/build/outputs/apk/debug/host-debug.apk"
