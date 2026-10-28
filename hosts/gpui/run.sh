#!/usr/bin/env bash
# Builds the application (the probe by default) as this system's C ABI archive, then the GPUI host:
# --target=macos on a Mac, linux-x64 or linux-arm64 on Linux, windows-x64 under Git Bash on Windows.
#   run.sh            build and open the window
#   run.sh --smoke    drive start/dispatch without a window and assert
#   run.sh --build    build only; the host is target/release/osprey-gpui, the application linked in
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
app=${OSPREY_GPUI_APP:-"$here/probe"}
case "$(uname -s)-$(uname -m)" in
    Darwin-*) target=macos ;;
    Linux-x86_64) target=linux-x64 ;;
    Linux-aarch64 | Linux-arm64) target=linux-arm64 ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) target=windows-x64 ;;
    *) echo "run.sh: no Osprey desktop target for $(uname -s) $(uname -m)" >&2; exit 2 ;;
esac
mkdir -p "$app/build"
"$compiler" "$app" --compile --target="$target" -o "$app/build/libOspreyApp.a"
export OSPREY_GPUI_APP_DIR="$app/build"
cd "$here"
if [ "${1:-}" = --build ]; then
    exec cargo build --release --quiet
fi
exec cargo run --release --quiet -- "$@"
