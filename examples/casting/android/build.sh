#!/usr/bin/env bash
# [CAST-MOBILE-DELIVERY] Build the shared player, native services and Android APK.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
target=${1:-android-arm64}
case "$target" in
    android-arm64) abi=arm64-v8a; triple=aarch64-linux-android26 ;;
    android-x64) abi=x86_64; triple=x86_64-linux-android26 ;;
    *) echo 'Usage: build.sh [android-arm64|android-x64]' >&2; exit 2 ;;
esac
cd "$repo"
source scripts/android-env.sh
if [[ ${OSPREY_MOBILE_SKIP_LIBRARY:-0} != 1 ]]; then bash examples/casting/mobile-build.sh "$target"; fi
output="$repo/examples/casting/build/$target"
native="$here/build/generated/jniLibs/$abi"
assets="$here/build/generated/assets"
mkdir -p "$native" "$assets"
"$android_tools/clang" --target="$triple" -shared -fPIC -O2 -std=c11 \
    -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
    -Wl,--no-undefined -Wl,-z,max-page-size=16384 -I"$output" \
    "$here/src/main/cpp/bridge.c" "$output/libOspreyApp.a" \
    compiler/runtime/file_server_runtime.c compiler/runtime/media_render_runtime.c \
    -lm -ldl -llog -o "$native/libosprey_casting.so"
cp examples/casting/player.machine.json "$assets/player.machine.json"
cp -R examples/casting/skins "$assets/skins"
cd "$here"
bash "$repo/examples/mobile/android/gradlew" --console=plain :app:assembleDebug
echo "$here/app/build/outputs/apk/debug/app-debug.apk"
