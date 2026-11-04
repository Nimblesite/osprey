#!/usr/bin/env bash
# Implements [IOS-TARGET-MACOS]. Build a scene for the Mac slice, link it into
# the Swift Metal host and run it until its window closes:
#   run.sh [kali|opal|character]          build and run one scene
#   run.sh --build [kali|opal|character]  build only
# OSPREY_GFX_FRAMES=<n> ends the scene after n frames. OSPREY_MACOS_SKIP_BUILD=1
# reuses the existing build.
set -euo pipefail

macos_dir=$(cd "$(dirname "$0")" && pwd)
graphics_dir=$(dirname "$macos_dir")
repo_dir=$(cd "$graphics_dir/../.." && pwd)
build_only=0
if [[ ${1:-} == --build ]]; then build_only=1; shift; fi
scene=${1:-kali}
host="$macos_dir/build/$scene/osprey-$scene"
if [[ ${OSPREY_MACOS_SKIP_BUILD:-0} != 1 ]]; then
    bash "$repo_dir/scripts/macos-host.sh" "$graphics_dir/$scene" "$macos_dir" "$host" --entry-only \
        -- -I "$graphics_dir" -framework AppKit -framework Metal -framework QuartzCore >&2
fi
[[ $build_only == 1 ]] && exit 0
# The scene names its shader relative to the repository root.
cd "$repo_dir"
exec "$host"
