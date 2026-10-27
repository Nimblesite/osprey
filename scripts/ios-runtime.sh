#!/usr/bin/env bash
# Build one runtime slice with the Makefile's hardened flags.
# The fingerprint includes every source, header, flag and selected Apple SDK.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/runtime-memory.sh

if [[ $(uname -s) != Darwin ]] || ! command -v xcrun >/dev/null; then
    echo "Apple runtime builds require macOS and Xcode with the selected SDK" >&2
    exit 1
fi

sdk=$1
triple=$2
archive=$3
shift 3
flags=()
while [[ $# -gt 0 && $1 != -- ]]; do flags+=("$1"); shift; done
[[ $# -gt 0 ]] || { echo "iOS runtime: missing source separator" >&2; exit 1; }
shift
[[ $# -gt 0 ]] || { echo "iOS runtime: no sources selected" >&2; exit 1; }
sources=("$@")
sdk_path=$(xcrun --sdk "$sdk" --show-sdk-path)
clang=$(xcrun --sdk "$sdk" --find clang)
libtool=$(xcrun --sdk "$sdk" --find libtool)
mkdir -p "$(dirname "$archive")" compiler/lib
scratch=$(mktemp -d "$(dirname "$archive")/.ios-runtime.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
fingerprint="$archive.inputs"
mirror="compiler/lib/$(basename "$archive")"

{
    printf '%s\n' "$triple" "$sdk_path" "$clang" "$libtool" "${flags[@]}" "${OSPREY_RUNTIME_SHIMS:-}"
    "$clang" --version
    xcrun --sdk "$sdk" --show-sdk-build-version
    for source in "${sources[@]}"; do cksum "compiler/runtime/$source.c"; done
    for source in ${OSPREY_RUNTIME_OBJC:-}; do cksum "compiler/runtime/$source.m"; done
    if [[ -n ${OSPREY_RUNTIME_OBJC:-} ]]; then cksum compiler/bin/gpu_shader_source.h compiler/runtime/*.metal; fi
    cksum compiler/runtime/*.h scripts/ios-runtime.sh scripts/runtime-memory.sh scripts/ios.mk Makefile
} >"$scratch/inputs"

if [[ ! -s "$archive" ]] || ! cmp -s "$scratch/inputs" "$fingerprint"; then
    echo "==> building Apple C runtime: $triple"
    for source in "${sources[@]}"; do
        compile_runtime_source "$source" "$clang" -target "$triple" -isysroot "$sdk_path" "${flags[@]}"
    done
    # Objective-C units, such as the Metal device adapter of the Mac slice ([GPU-DEVICE]).
    for source in ${OSPREY_RUNTIME_OBJC:-}; do
        "$clang" -target "$triple" -isysroot "$sdk_path" "${flags[@]}" -fobjc-arc \
            "compiler/runtime/$source.m" -o "$scratch/$source.o"
    done
    "$libtool" -static -o "$scratch/runtime.a" "$scratch"/*.o
    mv "$scratch/runtime.a" "$archive"
    mv "$scratch/inputs" "$fingerprint"
fi
if ! cmp -s "$archive" "$mirror"; then cp "$archive" "$mirror"; fi
