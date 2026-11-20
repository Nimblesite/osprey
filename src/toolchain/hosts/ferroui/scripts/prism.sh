#!/usr/bin/env bash
# Builds Osprey's Prism synth as a C library, links it into the FerroUI host over the shared-memory
# boundary and opens it. Every step is bounded: nothing here can run unattended forever.
#   prism.sh                  build and open (closes after PRISM_SECONDS, default 600)
#   prism.sh --shot <png>     build, open, wait PRISM_SETTLE seconds (default 6), capture the window, close
#                             (PRISM_TAP=<x>,<y> taps the canvas there first, in scene units)
#   prism.sh --repaint <secs> build, open, rebuild and repaint the whole view every frame, print timings
#   prism.sh --drag <secs>    build, open, drag the Oscillator module every frame for <secs>, print timings
#   prism.sh --bench <secs>   build, open, print animation frame timings after <secs>, close
# PRISM_PREBUILT=1 reuses the built archive and host. OSPREY_BIN names the compiler.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../../../.." && pwd)
app="$here/target/prism-app"
mkdir -p "$app"
[ "${PRISM_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 600 "${OSPREY_BIN:-$repo/target/release/osprey}" \
    "$repo/src/osprey/examples/synth" --compile --target=macos -o "$app/libOspreyApp.a" >/dev/null
export OSPREY_NATIVE_APP_DIR="$app" OSPREY_RUNTIME_DIR="$repo/src/toolchain/compiler/lib" \
    OSPREY_NATIVE_ABI="$repo/src/toolchain/compiler/runtime/native_abi.h"
[ "${PRISM_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 2400 cargo build --manifest-path "$here/Cargo.toml" --release --features desktop -q
export OSPREY_NATIVE_TITLE="Prism Synth" OSPREY_NATIVE_WINDOW=1152x756 OSPREY_NATIVE_CHROME=bare \
    OSPREY_NATIVE_TITLE_BAR=50
machine="$repo/src/osprey/packages/signal_flow/signal_flow.machine.json"
binary="$here/target/release/ferroui-osprey-native"
worker="$repo/target/vst-worker/osprey-vst-worker"
launch=(--machine "$machine")
[ -x "$worker" ] && launch+=(--vst-worker "$worker")
case "${1:-}" in
    --shot)
        OSPREY_NATIVE_TAP="${PRISM_TAP:-}" gtimeout 120 "$binary" "${launch[@]}" &
        pid=$!
        sleep "${PRISM_SETTLE:-6}"
        id=$(gtimeout 60 swift "$here/scripts/window-id.swift" ferroui-osprey-native)
        screencapture -x -o -l "$id" "$2"
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        echo "$2" ;;
    --repaint)
        OSPREY_NATIVE_REPAINT=1 OSPREY_NATIVE_BENCH="$2" \
            exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    --drag)
        OSPREY_NATIVE_DRAG="${PRISM_DRAG:-131,376}" OSPREY_NATIVE_BENCH="$2" \
            exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    --bench)
        OSPREY_NATIVE_BENCH="$2" exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    *)
        exec gtimeout "${PRISM_SECONDS:-600}" "$binary" "${launch[@]}" ;;
esac
