#!/usr/bin/env bash
# Profiles the daw ([DAW-PERF]): one row of timings per kind of user action on every page, then a 1 ms
# `sample` of each named scenario attributed to Osprey functions, then the song renderer the same way.
#   daw-profile.sh ["timeline frame tick" "timeline resize" ...]   (default: the four below)
# Reports land in target/daw-profile. Every step is bounded and runs at low priority.
# DAW_PROFILE_APP_DIR reuses a built libOspreyApp.a; DAW_PROFILE_RENDERER reuses a built renderer.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
out="$repo/target/daw-profile"
host="$repo/src/toolchain/hosts/ferroui"
osprey="${OSPREY_BIN:-$repo/target/release/osprey}"
app="${DAW_PROFILE_APP_DIR:-$out/app}"
mkdir -p "$out" "$app"
[ -n "${DAW_PROFILE_APP_DIR:-}" ] || nice -n 10 gtimeout 600 "$osprey" "$repo/src/osprey/examples/daw" \
    --compile --memory=arc --target=macos -o "$app/libOspreyApp.a" >/dev/null
export OSPREY_NATIVE_APP_DIR="$app" OSPREY_RUNTIME_DIR="$repo/src/toolchain/compiler/lib" \
    OSPREY_NATIVE_ABI="$repo/src/toolchain/compiler/runtime/native_abi.h"
probe() { nice -n 10 gtimeout 900 cargo test --manifest-path "$host/Cargo.toml" --release --features desktop \
    --test daw_profile "$@"; }
probe -- --ignored --nocapture 2>&1 | grep '^PROBE' | sed 's/^PROBE //' | tee "$out/events.txt"
binary=$(probe --no-run 2>&1 | sed -n 's/.*Executable .*(\(.*\))$/\1/p' | tail -1)
[[ $binary = /* ]] || binary="$repo/$binary"
[ -x "$binary" ] || { echo "the probe binary was not built: $binary" >&2; exit 1; }
[ $# -gt 0 ] || set -- "timeline frame tick" "timeline resize" "mix frame tick" "patch frame tick"
cd "$host"
for scenario in "$@"; do
    report="$out/${scenario// /-}"
    PROBE_HOLD="$scenario" PROBE_SECONDS=9 nice -n 10 "$binary" --ignored --nocapture >"$report.log" 2>&1 &
    pid=$!
    sleep 2
    nice -n 10 sample "$pid" 5 1 -file "$report.sample" >/dev/null 2>&1
    wait "$pid"
    python3 "$repo/scripts/make/sample-attribution.py" "$report.sample" >"$report.txt"
    echo "$report.txt"
done
cd "$repo"
renderer="${DAW_PROFILE_RENDERER:-$out/daw}"
[ -n "${DAW_PROFILE_RENDERER:-}" ] || nice -n 10 gtimeout 600 "$osprey" build "$repo/src/osprey/examples/daw/render" \
    --memory=arc -o "$renderer" >/dev/null
mkdir -p "$out/render"
{ /usr/bin/time -p nice -n 10 gtimeout 300 "$renderer" "$repo/src/osprey/examples/daw/tracks/codex-version.json" \
    "$out/render" 32 8 >/dev/null; } 2>"$out/render-time.txt" &
sleep 4
nice -n 10 sample "$(pgrep -n -f "^$renderer ")" 15 1 -file "$out/render.sample" >/dev/null 2>&1
wait
python3 "$repo/scripts/make/sample-attribution.py" "$out/render.sample" >"$out/render.txt"
echo "$out/render.txt ($(tr '\n' ' ' <"$out/render-time.txt"))"
