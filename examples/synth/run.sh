#!/usr/bin/env bash
# Builds the Prism synth into the GPUI host, bundles it as "Prism Synth.app" with the Prism logo as
# its Dock icon, and opens it: the signal-flow editor's Prism skin, playing each edit.
#   run.sh            build, bundle and open
#   run.sh --bundle   build and bundle only; prints the bundle's path
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
OSPREY_GPUI_APP="$here" "$repo/hosts/gpui/run.sh" --build
export OSPREY_GPUI_APP_NAME=synth OSPREY_GPUI_TITLE="Prism Synth" OSPREY_GPUI_WINDOW=1152x621 OSPREY_GPUI_CHROME=bare OSPREY_GPUI_LIGHTS=16,18
app=$("$repo/hosts/gpui/bundle.sh" "$here/build" "Prism Synth" dev.ospreylang.synth \
    "$repo/packages/signal_flow/skins/prism/logo.png" "$repo/packages/signal_flow/signal_flow.machine.json" | tail -1)
if [ "${1:-}" = --bundle ]; then
    echo "$app"
    exit 0
fi
open "$app"
