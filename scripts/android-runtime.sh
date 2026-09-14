#!/usr/bin/env bash
# Build both mobile runtime ABIs with the same hardened profile as native/iOS.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/android-env.sh
triple=$1
archive=$2
shift 2
flags=()
while [[ $# -gt 0 && $1 != -- ]]; do flags+=("$1"); shift; done
[[ $# -gt 1 ]] || { echo 'Android runtime requires -- and source names' >&2; exit 1; }
shift
sources=("$@")
mkdir -p "$(dirname "$archive")" compiler/lib
scratch=$(mktemp -d "$(dirname "$archive")/.android-runtime.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
{
    printf '%s\n' "$triple" "$android_ndk" "${flags[@]}"
    "$android_tools/clang" --version
    for source in "${sources[@]}"; do cksum "compiler/runtime/$source.c"; done
    cksum compiler/runtime/*.h scripts/android*.sh scripts/android.mk Makefile
} >"$scratch/inputs"
if [[ ! -s "$archive" ]] || ! cmp -s "$scratch/inputs" "$archive.inputs"; then
    echo "==> building Android C runtime: $triple"
    for source in "${sources[@]}"; do
        "$android_tools/clang" --target="$triple" "${flags[@]}" -fPIC -DOSPREY_ANDROID \
            "compiler/runtime/$source.c" -o "$scratch/$source.o"
    done
    "$android_tools/llvm-ar" rcs "$scratch/runtime.a" "$scratch"/*.o
    mv "$scratch/runtime.a" "$archive"
    mv "$scratch/inputs" "$archive.inputs"
fi
mirror="compiler/lib/$(basename "$archive")"
if ! cmp -s "$archive" "$mirror"; then cp "$archive" "$mirror"; fi
