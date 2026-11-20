#!/usr/bin/env bash
# Records the daw showcase video and checks it as a native journey [DAW-VIDEO]: opens the codex project in the
# FerroUI host, clicks through the Timeline, opens the patch of every voice type (FM operator, patch graph, modal
# bell, bowed string, drum), auditions it, visits Mix and plays bars 1-2. After every click a window screenshot must
# differ from the one before (the click changed the screen), or the journey fails. Writes OUT/prism-journey.mp4 with
# the rendered bars as its soundtrack, plus the step screenshots. Needs screen recording and accessibility access.
# usage: daw-video.sh <out-dir>   (PRISM_PREBUILT=1 reuses the built host)
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:?usage: daw-video.sh <out-dir>}
mkdir -p "$out" && out=$(cd "$out" && pwd)
song=${SONG:-$repo/src/osprey/examples/daw/tracks/codex-version.json}
mkdir -p "$out/steps" "$out/project"
project="$out/project/$(basename "$song" .json)-journey-$$.json"
cp "$song" "$project"
song="$project"
rm -f "$out"/steps/*.png
swiftc -O "$repo/scripts/make/daw-clicks.swift" -o "$out/clicks"
PRISM_SECONDS=300 PRISM_PROJECT="$song" bash "$repo/src/toolchain/hosts/ferroui/scripts/prism.sh" >"$out/host.log" 2>&1 &
launcher=$!
pid=
trap 'kill "$launcher" 2>/dev/null || true' EXIT
for _ in $(seq 120); do
    pid=$(pgrep -P "$launcher" -f /ferroui-osprey-native || true)
    [ -n "$pid" ] && break
    kill -0 "$launcher" 2>/dev/null || break
    sleep 5
done
[ -n "$pid" ] || { echo "daw-video: the host never started (see $out/host.log)" >&2; exit 1; }
trap 'kill "$pid" "$launcher" 2>/dev/null || true' EXIT
window=$(gtimeout 60 swift "$repo/src/toolchain/hosts/ferroui/scripts/window-id.swift" "$pid")
sleep 4
rm -f "$out/journey.mov"   # screencapture never overwrites
recording=$(date +%s)
gtimeout 400 screencapture -x -v -V "${VIDEO_SECONDS:-150}" -l "$window" "$out/journey.mov" &
recorder=$!
previous=
step=0
# Perform the pointer or menu action, then check the resulting window.
step_after() {
    local name=$1 delay=$2
    shift 2
    step=$((step + 1))
    "$out/clicks" "$pid" "$@"
    sleep "$delay"
    shot=$(printf '%s/steps/%02d-%s.png' "$out" "$step" "$name")
    screencapture -x -o -l "$window" "$shot"
    sum=$(md5 -q "$shot")
    [ "$sum" != "$previous" ] || { echo "daw-video: step $step ($name) left the screen unchanged" >&2; exit 1; }
    previous=$sum
    echo "step $step $name ok"
}
# At 1152×752 each 1440×940 scene unit occupies 0.8 window points.
row() { echo $((189 + 248 * (($1 - 1) % 7) / 10)); }
for voice in "3 patch-graph" "1 fm-operator" "11 modal-bell" "12 bowed-string" "14 drum"; do
    set -- $voice
    "$out/clicks" "$pid" "897,$((184 + 58 * (($1 - 1) / 7))),0.25"
    step_after "track-$2" 1.5 "185,$(row "$1"),0"
    step_after "patch-$2" 3 View "Track Patch"
    step_after "audition-$2" 1 Transport "Audition Patch"
    step_after "timeline-after-$2" 2 View Timeline
done
step_after mix 3 View "Bus Routing"
step_after timeline 2 View Timeline
started=$(date +%s)
step_after play-bars 2 Transport "Play Song"
wait "$recorder" || true
# The soundtrack is the bars Play rendered during this run: a WAV newer than the click that ffprobe can read.
bars=
for _ in $(seq 60); do
    for wav in "$repo/target/daw/$(basename "$song" .json)"/*/piece.wav; do
        [ -f "$wav" ] || continue
        [ "$(stat -f %m "$wav")" -ge "$started" ] && ffprobe -v error "$wav" 2>/dev/null && bars=$wav && break
    done
    [ -n "$bars" ] && break
    sleep 5
done
[ -n "$bars" ] || { echo "daw-video: Play bars produced no readable audio after $started" >&2; exit 1; }
# The host plays the bars as soon as their WAV is written, so the soundtrack starts at that moment of the recording.
nice -n 10 ffmpeg -v error -y -i "$out/journey.mov" -i "$bars" \
    -filter_complex "[0:v]fps=30,scale=1920:-2[v];[1:a]adelay=delays=$(( ($(stat -f %m "$bars") - recording) * 1000 )):all=1,apad[a]" \
    -map "[v]" -map "[a]" -t "${VIDEO_SECONDS:-150}" -c:v libx264 -threads 3 -pix_fmt yuv420p -c:a aac "$out/prism-journey.mp4"
echo "$out/prism-journey.mp4 ($step steps checked)"
