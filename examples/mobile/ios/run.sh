#!/usr/bin/env bash
# Implements [MOBILE-NATIVE-HOST]: reuse the iOS build and deployment tools.
set -euo pipefail
example_dir=$(cd "$(dirname "$0")" && pwd)
repo_dir=$(cd "$example_dir/../../.." && pwd)
export OSPREY_IOS_EXAMPLE_DIR="$example_dir"
export OSPREY_IOS_SCHEME=IssueInbox
export OSPREY_IOS_SOURCE="$example_dir/../inbox"
export OSPREY_IOS_BUNDLE_ID=org.ospreylang.IssueInbox
export OSPREY_IOS_SMOKE_FILE=inbox-smoke-result.txt
export OSPREY_IOS_SMOKE_ARG=--inbox-smoke
export OSPREY_IOS_SMOKE_EXPECT=OSPREY_INBOX_SMOKE_OK
icon="$example_dir/build/Assets.xcassets/AppIcon.appiconset/Icon.png"
if [[ ! -f "$icon" || "$example_dir/make-icon.swift" -nt "$icon" ]]; then
    swift "$example_dir/make-icon.swift" "$example_dir/build/Assets.xcassets"
fi
case "${1:-ios-sim}" in
    ios-sim) exec "$repo_dir/examples/ios/run.sh" ;;
    ios-device) exec "$repo_dir/examples/ios/run-device.sh" ;;
    --smoke) exec "$repo_dir/examples/ios/run.sh" --smoke ;;
    --build) exec "$repo_dir/examples/ios/build.sh" "${2:-ios-sim}" ;;
    *) echo "Usage: $0 [ios-sim|ios-device|--smoke|--build [ios|ios-sim|ios-device]]" >&2; exit 2 ;;
esac
