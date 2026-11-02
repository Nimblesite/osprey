#!/usr/bin/env bash
# Build a Swift command-line host around an Osprey project for the Mac slice:
# the one way every macOS example joins Osprey and Swift. The project compiles
# to libOspreyApp.a plus its generated libOspreyApp.h, which the host's
# Bridge.h imports exactly as the iPhone apps do. [IOS-TARGET-MACOS]
#
#   macos-host.sh <project> <swift-dir> <output> [osprey flags...] [-- swiftc flags...]
set -euo pipefail

if [[ $# -lt 3 ]]; then
    echo "usage: $0 <project> <swift-dir> <output> [osprey flags...] [-- swiftc flags...]" >&2
    exit 2
fi
project=$1
swift_dir=$2
output=$3
shift 3
osprey_flags=()
while [[ $# -gt 0 && $1 != -- ]]; do osprey_flags+=("$1"); shift; done
[[ $# -gt 0 ]] && shift
repo_dir=$(cd "$(dirname "$0")/.." && pwd)
compiler=${OSPREY_BIN:-"$repo_dir/target/release/osprey"}
if [[ ! -x "$compiler" ]]; then
    echo "Osprey compiler not found at $compiler. Run make -C apps macos from the repository root." >&2
    exit 1
fi
build=$(dirname "$output")
mkdir -p "$build"
"$compiler" "$project" --compile --target=macos --quiet ${osprey_flags[@]+"${osprey_flags[@]}"} \
    -o "$build/libOspreyApp.a"
xcrun --sdk macosx swiftc -O -target arm64-apple-macos12.0 \
    -import-objc-header "$swift_dir/Bridge.h" -I "$build" \
    "$swift_dir"/*.swift -L "$build" -lOspreyApp "$@" -o "$output"
echo "Built $output"
