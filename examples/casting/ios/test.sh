#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
bash "$here/build.sh" ios-sim
device=$(bash "$repo/scripts/ios-simulator.sh")
xcodegen generate --spec "$here/project.yml" --project "$here"
xcodebuild -project "$here/Casting.xcodeproj" -scheme Casting \
    -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$here/../build/ui-tests" \
    CODE_SIGNING_ALLOWED=NO test
