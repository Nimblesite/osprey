#!/usr/bin/env bash
# Build the shared casting library for a native mobile shell. [CAST-MOBILE-ABI]
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
target=${1:-ios-sim}
case "$target" in
    ios|ios-sim) runtime="_runtime_${target//-/_}_arc" ;;
    android-arm64|android-x64) runtime="_runtime_${target//-/_}_arc" ;;
    *) echo "Usage: $0 [ios|ios-sim|android-arm64|android-x64]" >&2; exit 2 ;;
esac
cd "$repo"
make "$runtime"
compiler=${OSPREY_BIN:-$repo/target/release/osprey}
if [[ ! -x "$compiler" ]]; then
    echo 'Build the compiler with cargo build --release -p osprey-cli' >&2
    exit 1
fi
output="$repo/examples/casting/build/$target"
mkdir -p "$output"
"$compiler" build examples/casting --target="$target" -o "$output/libOspreyApp.a"
echo "Shared library: $output/libOspreyApp.a (mobile host and Cast services still required)"
