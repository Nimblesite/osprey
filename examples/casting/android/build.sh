#!/usr/bin/env bash
# [CAST-MOBILE-ABI] Relink both supported ABIs before packaging the universal APK.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
if (( $# )); then echo 'Usage: build.sh (builds ARM64 and x64)' >&2; exit 2; fi
cd "$repo"
source scripts/android-env.sh

record_inputs() {
    python3 - "$@" <<'PY'
import hashlib,sys
from pathlib import Path
output,*files=sys.argv[1:]
Path(output).write_text(''.join(f'{hashlib.sha256(Path(name).read_bytes()).hexdigest()}  {Path(name).resolve()}\n' for name in files))
PY
}

build_native() {
    local target=$1 abi=$2 triple=$3
    if [[ ${OSPREY_MOBILE_SKIP_LIBRARY:-0} != 1 ]]; then bash examples/casting/mobile-build.sh "$target"; fi
    local output="$repo/examples/casting/build/$target" native="$here/build/generated/jniLibs/$abi"
    mkdir -p "$native"
    "$android_tools/clang" --target="$triple" -shared -fPIC -O2 -std=c11 \
        -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
        -Wl,--no-undefined -Wl,-z,max-page-size=16384 -I"$output" \
        "$here/src/main/cpp/bridge.c" "$output/libOspreyApp.a" \
        compiler/runtime/file_server_runtime.c compiler/runtime/media_render_runtime.c \
        -lm -ldl -llog -o "$native/libosprey_casting.so"
    "$android_tools/llvm-strip" --strip-unneeded -o "$output/libosprey_casting.stripped.so" "$native/libosprey_casting.so"
    record_inputs "$output/native-inputs.sha256" "$output/libOspreyApp.a" "$output/libOspreyApp.h" \
        "$here/src/main/cpp/bridge.c" compiler/runtime/file_server_runtime.c compiler/runtime/media_render_runtime.c \
        "$here/build.sh" scripts/android-env.sh "$native/libosprey_casting.so" "$output/libosprey_casting.stripped.so"
}

build_native android-arm64 arm64-v8a aarch64-linux-android26
build_native android-x64 x86_64 x86_64-linux-android26
assets="$here/build/generated/assets"
mkdir -p "$assets"
cp examples/casting/player.machine.json "$assets/player.machine.json"
rm -rf "$assets/skins"
cp -R examples/casting/skins "$assets/"
cd "$here"
bash "$repo/examples/mobile/android/gradlew" --console=plain :app:assembleDebug
echo "$here/app/build/outputs/apk/debug/app-debug.apk"
