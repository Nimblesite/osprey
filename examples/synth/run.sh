#!/usr/bin/env bash
# Builds the Prism synth into the GPUI host, bundles it as "Prism Synth.app" with the Prism logo as
# its Dock icon, and opens it: the signal-flow editor's Prism skin over a keyboard that plays it, fitted to the window.
#   run.sh            build, bundle and open
#   run.sh --bundle   build and bundle only; prints the bundle's path
#   run.sh --smoke    replay smoke.jsonl through the host without a window
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
machine="$repo/packages/signal_flow/signal_flow.machine.json"
if [ "${1:-}" = --smoke ]; then
    OSPREY_GPUI_APP="$here" exec "$repo/hosts/gpui/run.sh" --smoke --machine "$machine"
fi
OSPREY_GPUI_APP="$here" "$repo/hosts/gpui/run.sh" --build
export OSPREY_GPUI_APP_NAME=synth OSPREY_GPUI_TITLE="Prism Synth" OSPREY_GPUI_WINDOW=1152x756 OSPREY_GPUI_CHROME=bare OSPREY_GPUI_LIGHTS=16,18
app=$("$repo/hosts/gpui/bundle.sh" "$here/build" "Prism Synth" dev.ospreylang.synth \
    "$repo/packages/signal_flow/skins/prism/logo.png" "$machine" | tail -1)
if [ "${1:-}" = --bundle ]; then
    echo "$app"
    exit 0
fi
open "$app"
