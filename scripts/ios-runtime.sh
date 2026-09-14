#!/usr/bin/env bash
# Build one default-memory runtime slice with the Makefile's hardened flags.
# The fingerprint includes every source, header, flag and selected Apple SDK.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $(uname -s) != Darwin ]] || ! command -v xcrun >/dev/null; then
    echo "iOS runtime builds require macOS and Xcode with the iOS SDKs" >&2
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
    printf '%s\n' "$triple" "$sdk_path" "$clang" "$libtool" "${flags[@]}"
    "$clang" --version
    xcrun --sdk "$sdk" --show-sdk-build-version
    for source in "${sources[@]}"; do cksum "compiler/runtime/$source.c"; done
    cksum compiler/runtime/*.h scripts/ios-runtime.sh scripts/ios.mk Makefile
} >"$scratch/inputs"

if [[ ! -s "$archive" ]] || ! cmp -s "$scratch/inputs" "$fingerprint"; then
    echo "==> building iOS C runtime: $triple"
    for source in "${sources[@]}"; do
        "$clang" -target "$triple" -isysroot "$sdk_path" "${flags[@]}" \
            "compiler/runtime/$source.c" -o "$scratch/$source.o"
    done
    "$libtool" -static -o "$scratch/runtime.a" "$scratch"/*.o
    mv "$scratch/runtime.a" "$archive"
    mv "$scratch/inputs" "$fingerprint"
fi
if ! cmp -s "$archive" "$mirror"; then cp "$archive" "$mirror"; fi
