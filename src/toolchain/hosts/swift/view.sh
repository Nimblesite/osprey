#!/usr/bin/env bash
# Builds an Osprey Native application into the shared-memory view host and runs
# it: the host draws the rows the application writes and reads them in place,
# with no wire format between them. [NATIVE-SHM-VIEW]
#   OSPREY_SWIFT_APP=<folder> view.sh [--self-test]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
app=${OSPREY_SWIFT_APP:-"$repo/src/osprey/examples/tally"}
runtime="$repo/src/toolchain/compiler/runtime"

mkdir -p "$here/build/view"
# Packaging still checks types; advisory annotation proofs belong to analysis.
"$compiler" "$app" --compile --target=macos --memory=arc --quiet --no-warnings -o "$here/build/view/libOspreyViewApp.a"
cp "$here/build/view/libOspreyViewApp.h" "$here/Sources/OspreyViewApp/include/libOspreyViewApp.h"
cp "$runtime/native_bridge.h" "$runtime/native_abi.h" "$here/Sources/OspreyViewApp/include/"
symbols="$(nm "$here/build/view/libOspreyViewApp.a" 2>/dev/null)"
for symbol in osprey_main osprey_app_start osprey_app_event; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The application does not export $symbol" >&2; exit 1; }
done
cd "$here"
swift build -c release --product OspreyViewHost
exec "$(swift build -c release --show-bin-path)/OspreyViewHost" "$@"
