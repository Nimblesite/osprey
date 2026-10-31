#!/usr/bin/env bash
# Pixel comparison of the Osprey painting port with GPUI's original ([GPUI-HOST-SCENE]): builds both,
# captures the original, opens the port at the original's exact window size, captures it, and diffs
# the content below the title bar. With --draw, both windows first receive the same synthetic
# strokes (draw.swift). Needs Screen Recording and, for --draw, Accessibility permission.
#   compare.sh [--draw] [out-dir]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
python3 "$repo/hosts/gpui/dependencies.py"
draw=false
if [ "${1:-}" = "--draw" ]; then draw=true; shift; fi
out=${1:-"$here/out"}
mkdir -p "$out"
target="$repo/hosts/gpui/target"

# The trap is the only place a window is closed, and a stopped child is forgotten before the trap
# can run again, so no failure path leaves a window open or signals a PID that has been reused.
reference=""
port=""
stop() {
    [ -n "$1" ] || return 0
    kill "$1" 2>/dev/null || true
    wait "$1" 2>/dev/null || true
}
cleanup() {
    trap - EXIT INT TERM
    stop "$reference"
    stop "$port"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

# differing_pixels <report>: the differing-pixel count diff.swift reported, or nothing when the report
# is unreadable. The drawn-content check and the final comparison read it the same way.
differing_pixels() {
    printf '%s\n' "$1" | sed -n 's/.*px: \([0-9][0-9]*\) differing pixels.*/\1/p'
}

# draw_gain <status> <count> <report> <name>: a drawn window counts as changed only when the diff both
# reported a difference and named a single positive count. A zero count, an unusable report or any
# other status is refused, so a read or compile error cannot pass as delivered input.
draw_gain() {
    case $1:$2 in
        1:0) echo "drawing into $4 reported no differing pixels: $3" >&2; return 1 ;;
        1:'' | 1:*[!0-9]*) echo "drawing into $4 reported no differing-pixel count: $3" >&2; return 1 ;;
        1:*) echo "drawing into $4 changed $2 pixels" >&2; return 0 ;;
        0:*) echo "drawing into $4 changed no content; the window received no input: $3" >&2; return 1 ;;
        *) echo "drawing into $4 could not be compared (diff status $1): $3" >&2; return 1 ;;
    esac
}

# drawn_change <before> <after>: the strokes must alter the RGB content below the title bar, so a
# window that received no input (no Accessibility permission) fails instead of passing as a drawn
# comparison. It uses the final comparison's rectangle, so a title-bar focus change cannot pass it.
drawn_change() {
    local pixels width height status report
    pixels=$(sips -g pixelWidth -g pixelHeight "$1" | awk '/pixel/ {print $2}' | xargs)
    read -r width height <<<"$pixels"
    if report=$(swift "$here/diff.swift" "$1" "$2" 0 56 "$width" "$((height - 24))" \
        "${2%.png}-draw-change.png"); then
        status=0
    else
        status=$?
    fi
    draw_gain "$status" "$(differing_pixels "$report")" "$report" "${2##*/}"
}

# capture <process> <png>: waits for the process's window, captures it without its shadow, prints its bounds.
capture() {
    local found="" before=""
    for _ in $(seq 1 120); do
        found=$("$out/window" "$1" 2>/dev/null | head -1)
        [ -n "$found" ] && break
        sleep 1
    done
    [ -n "$found" ] || { echo "no window for $1" >&2; return 1; }
    sleep 2
    if $draw; then
        before="${2%.png}-before-draw.png"
        screencapture -x -o -l "${found%% *}" "$before"
        "$out/draw" "$1" || { echo "drawing into $1 failed" >&2; return 1; }
        sleep 1
    fi
    screencapture -x -o -l "${found%% *}" "$2"
    if $draw; then drawn_change "$before" "$2" || return 1; fi
    echo "$found"
}

(cd "$here" && CARGO_TARGET_DIR="$target" cargo build --release --quiet)
swiftc -O "$here/window.swift" -o "$out/window"
swiftc -O "$here/draw.swift" -o "$out/draw"

# Both builds finish before the first capture, so the window waits below measure the windows alone:
# a port that fails to compile ends the comparison here instead of opening a stale or absent window.
build_port() {
    CARGO_TARGET_DIR="$target" OSPREY_GPUI_APP_NAME=painting "$here/../run.sh" --build
}
if ! build_port >"$out/port-build.log" 2>&1; then
    echo "painting port build failed; see $out/port-build.log" >&2
    cat "$out/port-build.log" >&2
    exit 1
fi

"$target/release/painting-reference" &
reference=$!
bounds=$(capture painting-reference "$out/reference.png") || exit 1
stop "$reference"
reference=""
read -r _ width height <<<"$bounds"
# The title bar is 28 points; the host takes the content size. The already-built host is launched
# with that size, not through run.sh, whose build would run while the port's window was being waited for.
OSPREY_GPUI_APP="$here/.." OSPREY_GPUI_APP_NAME=painting OSPREY_GPUI_WINDOW="${width}x$((height - 28))" \
    "$target/release/osprey-gpui" >"$out/port.log" 2>&1 &
port=$!
ported=$(capture osprey-gpui "$out/port.png") || exit 1
stop "$port"
port=""
echo "reference window: $bounds"
echo "port window:      $ported"
pixels=$(sips -g pixelWidth -g pixelHeight "$out/reference.png" | awk '/pixel/ {print $2}' | xargs)
read -r pw ph <<<"$pixels"
# diff.swift keeps its mask and fails on any difference; the count is checked as well, so a report
# that names differing pixels while exiting 0 cannot pass either. An unreadable report fails too.
if ! report=$(swift "$here/diff.swift" "$out/reference.png" "$out/port.png" \
    0 56 "$pw" "$((ph - 24))" "$out/diff.png"); then
    [ -z "$report" ] || echo "$report"
    echo "pixel comparison failed; see $out/diff.png" >&2
    exit 1
fi
echo "$report"
differing=$(differing_pixels "$report")
case $differing in
    '' | *[!0-9]*) echo "pixel comparison printed no differing-pixel count" >&2; exit 1 ;;
esac
if [ "$differing" != 0 ]; then
    echo "painting port differs in $differing pixels; see $out/diff.png" >&2
    exit 1
fi
