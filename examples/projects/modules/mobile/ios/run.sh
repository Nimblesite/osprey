#!/usr/bin/env bash
set -euo pipefail
example_dir=$(cd "$(dirname "$0")" && pwd)
repo_dir=$(cd "$example_dir/../../../../.." && pwd)
export OSPREY_IOS_EXAMPLE_DIR="$example_dir"
export OSPREY_IOS_SCHEME=TalonBank
export OSPREY_IOS_SOURCE="$example_dir/../app"
export OSPREY_IOS_BUNDLE_ID=org.ospreylang.TalonBank
case "${1:-ios-sim}" in
    ios-sim) exec "$repo_dir/examples/ios/run.sh" ;;
    ios-device) exec "$repo_dir/examples/ios/run-device.sh" ;;
    --test) exec "$example_dir/test.sh" ;;
    --build) exec "$repo_dir/examples/ios/build.sh" "${2:-ios-sim}" ;;
    *) echo "Usage: $0 [ios-sim|ios-device|--test|--build [ios|ios-sim|ios-device]]" >&2; exit 2 ;;
esac
