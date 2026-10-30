#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export OSPREY_GPUI_APP="$here"
export OSPREY_GPUI_APP_NAME=flock
exec "$here/../../../hosts/gpui/run.sh" "$@"
