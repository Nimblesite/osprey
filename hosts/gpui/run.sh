#!/usr/bin/env bash
# Builds the probe application as a macOS C ABI archive, then the GPUI host.
#   run.sh            build and open the window
#   run.sh --smoke    drive start/dispatch without a window and assert
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
app=${OSPREY_GPUI_APP:-"$here/probe"}
mkdir -p "$app/build"
"$compiler" "$app" --compile --target=macos -o "$app/build/libOspreyApp.a"
export OSPREY_GPUI_APP_DIR="$app/build"
cd "$here" && cargo run --release --quiet -- "$@"
