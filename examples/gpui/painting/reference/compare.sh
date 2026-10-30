#!/usr/bin/env bash
# Pixel comparison of the Osprey painting port with GPUI's original ([GPUI-HOST-SCENE]): builds and
# captures the original, opens the port at the original's exact window size, captures it, and diffs
# the content below the title bar. With --draw, both windows first receive the same synthetic
# strokes (draw.swift). Needs Screen Recording and, for --draw, Accessibility permission.
#   compare.sh [--draw] [out-dir]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
draw=false
if [ "${1:-}" = "--draw" ]; then draw=true; shift; fi
out=${1:-"$here/out"}
mkdir -p "$out"
target="$repo/hosts/gpui/target"
(cd "$here" && CARGO_TARGET_DIR="$target" cargo build --release --quiet)
swiftc -O "$here/window.swift" -o "$out/window"
swiftc -O "$here/draw.swift" -o "$out/draw"

# capture <process> <png>: waits for the process's window, captures it without its shadow, prints its bounds.
capture() {
    local found=""
    for _ in $(seq 1 120); do
        found=$("$out/window" "$1" 2>/dev/null | head -1)
        [ -n "$found" ] && break
        sleep 1
    done
    [ -n "$found" ] || { echo "no window for $1" >&2; exit 1; }
    sleep 2
    if $draw; then "$out/draw" "$1" || { echo "drawing into $1 failed" >&2; return 1; }; sleep 1; fi
    screencapture -x -o -l "${found%% *}" "$2"
    echo "$found"
}

"$target/release/painting-reference" &
reference=$!
bounds=$(capture painting-reference "$out/reference.png") || exit 1
kill "$reference"
read -r _ width height <<<"$bounds"
# The title bar is 28 points; the host takes the content size.
OSPREY_GPUI_WINDOW="${width}x$((height - 28))" "$here/../run.sh" >"$out/port.log" 2>&1 &
port=$!
ported=$(capture osprey-gpui "$out/port.png") || exit 1
pkill -x osprey-gpui || true
wait "$port" 2>/dev/null || true
echo "reference window: $bounds"
echo "port window:      $ported"
pixels=$(sips -g pixelWidth -g pixelHeight "$out/reference.png" | awk '/pixel/ {print $2}' | xargs)
read -r pw ph <<<"$pixels"
swift "$here/diff.swift" "$out/reference.png" "$out/port.png" 0 56 "$pw" "$((ph - 24))" "$out/diff.png"
