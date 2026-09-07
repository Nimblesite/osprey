#!/usr/bin/env bash
# Implements [IOS-SWIFT-HOST]: build, sign, install and launch on a physical iPhone.
set -euo pipefail

usage() {
    echo "Usage: OSPREY_DEVELOPMENT_TEAM=<team-id> [OSPREY_DEVICE_ID=<identifier-or-udid>] $0"
}
if [[ $# != 0 ]]; then
    if [[ $# == 1 && $1 == --help ]]; then usage; exit 0; fi
    usage >&2; exit 2
fi
: "${OSPREY_DEVELOPMENT_TEAM:?Set OSPREY_DEVELOPMENT_TEAM to your Xcode signing team ID.}"

example_dir=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/osprey-device.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
xcrun devicectl list devices --quiet --json-output "$scratch/devices.json"
selection=$(python3 - "$scratch/devices.json" <<'PY'
import json, os, sys
with open(sys.argv[1]) as source:
    devices = json.load(source)["result"]["devices"]
phones = [d for d in devices if d.get("hardwareProperties", {}).get("deviceType") == "iPhone"]
requested = os.environ.get("OSPREY_DEVICE_ID")
if requested:
    phones = [d for d in phones if requested in (d["identifier"], d["hardwareProperties"].get("udid"))]
else:
    phones = [d for d in phones if d.get("connectionProperties", {}).get("tunnelState") == "connected"]
if len(phones) != 1:
    choices = ", ".join(d["identifier"] for d in phones)
    sys.exit("Select one connected iPhone with OSPREY_DEVICE_ID; run xcrun devicectl list devices. " + choices)
phone = phones[0]
if phone.get("connectionProperties", {}).get("tunnelState") != "connected":
    sys.exit("The selected iPhone is not connected. Connect, unlock and trust this Mac, then retry.")
if phone.get("deviceProperties", {}).get("developerModeStatus") != "enabled":
    sys.exit("Enable Developer Mode on the iPhone in Settings > Privacy & Security, then retry.")
print(phone["identifier"], phone["hardwareProperties"]["udid"])
PY
)
read -r device device_udid <<< "$selection"
OSPREY_DEVICE_UDID="$device_udid" "$example_dir/build.sh" ios-device
app="$example_dir/build/ios-device/products/OspreyCounter.app"
xcrun devicectl device install app --device "$device" "$app"
if xcrun devicectl device process launch --terminate-existing --device "$device" org.ospreylang.OspreyCounter; then
    echo "Osprey Counter launched on your iPhone."
else
    status=$?
    echo "Launch failed. Keep the iPhone unlocked and resolve the device error above, then retry." >&2
    exit "$status"
fi
