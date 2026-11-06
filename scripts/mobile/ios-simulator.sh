#!/usr/bin/env bash
# Implements [IOS-VERIFICATION]: select one iPhone for C and Swift integration tests.
set -euo pipefail

selection=$(xcrun simctl list devices available --json | python3 -c '
import json, os, sys
devices = [d for runtime, ds in json.load(sys.stdin)["devices"].items()
           if ".iOS-" in runtime for d in ds]
requested = os.environ.get("OSPREY_SIMULATOR_UDID")
if requested:
    devices = [d for d in devices if d["udid"] == requested]
else:
    devices = [d for d in devices if ".iPhone-" in d.get("deviceTypeIdentifier", "")]
devices.sort(key=lambda d: d["state"] != "Booted")
if not devices:
    sys.exit("No matching iPhone simulator available. Install an iOS runtime in Xcode settings.")
print(devices[0]["udid"], devices[0]["state"])
')
read -r device state <<< "$selection"
if [[ "$state" != Booted ]]; then xcrun simctl boot "$device" >&2; fi
xcrun simctl bootstatus "$device" -b >&2
if [[ "${OSPREY_SIMULATOR_VISIBLE:-0}" == 1 ]]; then
    developer=$(xcode-select -p)
    if [[ -d "$developer/Applications/Simulator.app" ]]; then
        open -a "$developer/Applications/Simulator.app" --args -CurrentDeviceUDID "$device" >&2
    elif [[ -d "${developer%/*}/Applications/DeviceHub.app" ]]; then
        open -a "${developer%/*}/Applications/DeviceHub.app" "devices://device/open?id=$device" >&2
    else
        echo "No simulator viewer in the selected Xcode installation: $developer" >&2
        exit 1
    fi
fi
echo "$device"
