#!/usr/bin/env bash
# Builds the painting port and opens it in the GPUI host at GPUI's default window size.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export OSPREY_GPUI_APP="$here" OSPREY_GPUI_APP_NAME=painting OSPREY_GPUI_WINDOW=${OSPREY_GPUI_WINDOW:-default}
exec "$here/../../../hosts/gpui/run.sh" "$@"
