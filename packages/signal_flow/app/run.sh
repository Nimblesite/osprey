#!/usr/bin/env bash
# Opens the signal-flow editor in the GPUI host. The interface is signal_flow.machine.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
export OSPREY_GPUI_APP="$here"
export OSPREY_GPUI_APP_NAME=signal_flow
export OSPREY_GPUI_TITLE="Signal flow" OSPREY_GPUI_CHROME=bare OSPREY_GPUI_WINDOW=1152x620
exec "$repo/hosts/gpui/run.sh" "$@" --machine "$here/../signal_flow.machine.json"
