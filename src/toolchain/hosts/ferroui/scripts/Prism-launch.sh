#!/bin/bash
set -euo pipefail
binary=$(cd "$(dirname "$0")" && pwd)
resources="$binary/../Resources"
support="$HOME/Library/Application Support/Prism"
render_cache="$HOME/Library/Caches/Prism"
cd "$resources"
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
export OSPREY_NATIVE_TITLE=Prism OSPREY_NATIVE_WINDOW=1152x752 OSPREY_NATIVE_CHROME=bare \
    OSPREY_NATIVE_TITLE_BAR=50 OSPREY_NATIVE_APPEARANCE=dark

if [[ ${1:-} == --project ]]; then
    project=$2
else
    mkdir -p "$support"
    project="$support/codex-version.json"
    [[ -e $project ]] || cp -n "$resources/codex-version.json" "$project"
fi
mkdir -p "$render_cache"
ln -sfn "$binary/daw" "$render_cache/daw"
launch=(--machine "$resources/signal_flow.machine.json" --song "$project" --renderer "$render_cache/daw")
worker="$binary/osprey-vst-worker"
[[ ! -x $worker ]] || launch+=(--vst-worker "$worker")
exec "$binary/ferroui-osprey-native" "${launch[@]}"
