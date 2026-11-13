#!/usr/bin/env bash
# Builds an Osprey Native application into the shared-memory view host and runs
# it: the host leases the frame the application writes, checks every row and
# draws it in place, with no wire format between them. [NATIVE-SHM-VIEW]
#   OSPREY_SWIFT_APP=<folder> view.sh [--view-check]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
app=${OSPREY_SWIFT_APP:-"$repo/src/osprey/examples/tally"}
# Absolute, so `--resources` still names the application after this script changes folder.
case "$app" in /*) ;; *) app="$repo/$app" ;; esac
runtime="$repo/src/toolchain/compiler/runtime"

mkdir -p "$here/build/app"
# Packaging still checks types; advisory annotation proofs belong to analysis.
"$compiler" "$app" --compile --target=macos --memory=arc --quiet --no-warnings -o "$here/build/app/libOspreyApp.a"
cp "$here/build/app/libOspreyApp.h" "$here/Sources/OspreyViewApp/include/libOspreyApp.h"
cp "$runtime/native_bridge.h" "$runtime/native_abi.h" "$here/Sources/OspreyViewApp/include/"
symbols="$(nm "$here/build/app/libOspreyApp.a" 2>/dev/null)"
for symbol in osprey_main osprey_app_argument osprey_app_open osprey_app_event osprey_host_abi_version; do
    grep -q " T _$symbol\$" <<<"$symbols" || { echo "The application does not export $symbol" >&2; exit 1; }
done
cd "$here"
swift build -c release --product OspreyViewHost
exec "$(swift build -c release --show-bin-path)/OspreyViewHost" --resources "$app" "$@"
