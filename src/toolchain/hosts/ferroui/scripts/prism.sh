#!/usr/bin/env bash
# Builds Osprey's Prism synth as a C library, links it into the FerroUI host over the shared-memory
# boundary and opens it. Every step is bounded: nothing here can run unattended forever.
#   prism.sh                  build and open; it stays open until you close it (PRISM_SECONDS=<n> closes it after n seconds, for scripts)
#   prism.sh --bundle         build the macOS application without opening a window
#   prism.sh --shot <png>     build, open, wait PRISM_SETTLE seconds (default 6), capture the window, close
#                             (PRISM_TAP="x,y;x,y" clicks those points in scene units before the capture)
#   prism.sh --repaint <secs> build, open, rebuild and repaint the whole view every frame, print timings
#   prism.sh --drag <secs>    build, open, drag the Oscillator module every frame for <secs>, print timings
#   prism.sh --bench <secs>   build, open, print animation frame timings after <secs>, close
# PRISM_PREBUILT=1 reuses the built archive and host. OSPREY_BIN names the compiler.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../../../.." && pwd)
app="$repo/target/prism-app"
mkdir -p "$app"
[ "${PRISM_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 600 "${OSPREY_BIN:-$repo/target/release/osprey}" \
    "$repo/src/osprey/examples/daw" --compile --memory=arc --target=macos -o "$app/libOspreyApp.a" >/dev/null
export OSPREY_NATIVE_APP_DIR="$app" OSPREY_RUNTIME_DIR="$repo/src/toolchain/compiler/lib" \
    OSPREY_NATIVE_ABI="$repo/src/toolchain/compiler/runtime/native_abi.h"
[ "${PRISM_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 2400 cargo build --manifest-path "$here/Cargo.toml" --release --features desktop -q
window_size=1152x756
[[ -z "${PRISM_PROJECT:-}" ]] || window_size=1152x752
export OSPREY_NATIVE_TITLE="Prism" OSPREY_NATIVE_WINDOW="$window_size" OSPREY_NATIVE_CHROME=bare \
    OSPREY_NATIVE_TITLE_BAR=50 OSPREY_NATIVE_APPEARANCE=dark
machine="$repo/src/osprey/packages/signal_flow/signal_flow.machine.json"
binary="${PRISM_BINARY:-$repo/target/release/ferroui-osprey-native}"
# Launch the host inside its bundle so AppKit uses the Prism artwork for the Dock.
if [[ ${1:-} == --finder ]]; then
    binary=$2
    shift 2
else
    bundle="${PRISM_BUNDLE_DIR:-$app}/Prism.app"
    contents="$bundle/Contents"
    mkdir -p "$contents/MacOS" "$contents/Resources"
    /usr/bin/install -SC -m 755 "$binary" "$contents/MacOS/ferroui-osprey-native"
    /usr/bin/install -SC -m 644 "$here/scripts/Prism.plist" "$contents/Info.plist"
    binary="$contents/MacOS/ferroui-osprey-native"
    icon="$contents/Resources/AppIcon.icns"
    logo="$repo/src/osprey/packages/signal_flow/skins/prism/logo.png"
    if [[ ! -s $icon || $logo -nt $icon ]]; then
        bash "$here/../swift/app-icon.sh" "$logo" "$icon"
    fi
    # Finder supplies no song or machine arguments. Reuse the normal launch path and its defaults.
    entry=$(mktemp "$contents/MacOS/.Prism.XXXXXX")
    printf '#!/bin/bash\ncd %q\nexport PRISM_PREBUILT=1 PRISM_PROJECT=%q\nexec /bin/bash %q --finder "$(dirname "$0")/ferroui-osprey-native"\n' \
        "$repo" "$repo/src/osprey/examples/daw/tracks/codex-version.json" "$here/scripts/prism.sh" > "$entry"
    chmod 755 "$entry" && mv -f "$entry" "$contents/MacOS/Prism"
    if [[ ${1:-} == --bundle ]]; then
        echo "$bundle"
        exit 0
    fi
fi
worker="$repo/target/vst-worker/osprey-vst-worker"
launch=(--machine "$machine")
[ -x "$worker" ] && launch+=(--vst-worker "$worker")
if [ -n "${PRISM_PROJECT:-}" ]; then
    project=$PRISM_PROJECT
    [[ $project = /* ]] || project="$PWD/$project"
    [[ -r $project ]] || { echo "Cannot open project: $project" >&2; exit 1; }
    launch+=(--song "$project" --renderer "$repo/target/daw/daw")
fi
case "${1:-}" in
    --shot)
        OSPREY_NATIVE_TAP="${PRISM_TAP:-}" "$binary" "${launch[@]}" &
        pid=$!
        trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
        id=$(gtimeout 60 swift "$here/scripts/window-id.swift" "$pid")
        sleep "${PRISM_SETTLE:-6}"
        screencapture -x -o -l "$id" "$2"
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
        [ -n "${PRISM_SECONDS:-}" ] || exec "$binary" "${launch[@]}"
        exec gtimeout "$PRISM_SECONDS" "$binary" "${launch[@]}" ;;
esac
