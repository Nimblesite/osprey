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
mkdir -p "$out/steps"
rm -f "$out"/steps/*.png
swiftc -O "$repo/scripts/make/daw-clicks.swift" -o "$out/clicks"
PRISM_SECONDS=300 PRISM_PROJECT="$song" bash "$repo/src/toolchain/hosts/ferroui/scripts/prism.sh" >"$out/host.log" 2>&1 &
launcher=$!
pid=
for _ in $(seq 120); do pid=$(pgrep -n ferroui-osprey-native || true); [ -n "$pid" ] && break; sleep 5; done
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
# click <x> <y> (window points) <seconds to settle> <name>
click() {
    step=$((step + 1))
    "$out/clicks" "$pid" "$1,$2,$3"
    shot=$(printf '%s/steps/%02d-%s.png' "$out" "$step" "$4")
    screencapture -x -o -l "$window" "$shot"
    sum=$(md5 -q "$shot")
    [ "$sum" != "$previous" ] || { echo "daw-video: step $step ($4) left the screen unchanged" >&2; exit 1; }
    previous=$sum
    echo "step $step $4 ok"
}
row() { echo $((116 + 29 * ($1 - 1) / 2)); }     # Timeline track row, 14.5 points apart
timeline=(961 24)                                # on the patch page
for voice in "3 patch-graph" "1 fm-operator" "11 modal-bell" "12 bowed-string" "14 drum"; do
    set -- $voice
    click 50 "$(row "$1")" 1.5 "track-$2"
    click 1033 314 3 "patch-$2"
    click 534 24 4 "audition-$2"
    click "${timeline[@]}" 2 "timeline-after-$2"
done
click 510 21 3 mix
click 412 21 2 timeline
started=$(date +%s)
click 903 21 2 play-bars
wait "$recorder" || true
# The soundtrack is the bars Play rendered during this run: a WAV newer than the click that ffprobe can read.
bars=
for _ in $(seq 60); do
    for wav in $(ls -t "$repo/target/daw/$(basename "$song" .json)"/*/*.wav 2>/dev/null); do
        [ "$(stat -f %m "$wav")" -ge "$started" ] && ffprobe -v error "$wav" 2>/dev/null && bars=$wav && break
    done
    [ -n "$bars" ] && break
    sleep 5
done
[ -n "$bars" ] || { echo "daw-video: Play bars produced no readable audio after $started" >&2; exit 1; }
# The host plays the bars as soon as their WAV is written, so the soundtrack starts at that moment of the recording.
ffmpeg -v error -y -i "$out/journey.mov" -i "$bars" \
    -filter_complex "[0:v]fps=30,scale=1920:-2[v];[1:a]adelay=delays=$(( ($(stat -f %m "$bars") - recording) * 1000 )):all=1,apad[a]" \
    -map "[v]" -map "[a]" -t "${VIDEO_SECONDS:-150}" -c:v libx264 -pix_fmt yuv420p -c:a aac "$out/prism-journey.mp4"
echo "$out/prism-journey.mp4 ($step steps checked)"
