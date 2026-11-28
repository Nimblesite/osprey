#!/usr/bin/env bash
# Builds Osprey's Perception Studio as a C library, links it into the FerroUI host over the shared-memory
# boundary and opens it. Every step is bounded: nothing here can run unattended forever.
#   studio.sh                  build and open; it stays open until you close it (STUDIO_SECONDS=<n> closes it after n seconds, for scripts)
#   studio.sh --bundle         build the macOS application without opening a window
#   studio.sh --install        install Perception Studio.app in Applications and register it with Launch Services
#   studio.sh --shot <png>     build, open, wait STUDIO_SETTLE seconds (default 6), capture the window, close
#                             (STUDIO_TAP="x,y;x,y" clicks those points in scene units before the capture)
#   studio.sh --repaint <secs> build, open, rebuild and repaint the whole view every frame, print timings
#   studio.sh --drag <secs>    build, open, drag the Oscillator module every frame for <secs>, print timings
#   studio.sh --bench <secs>   build, open, print animation frame timings after <secs>, close
# STUDIO_PREBUILT=1 reuses the built archive and host. OSPREY_BIN names the compiler.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../../../.." && pwd)
app="$repo/target/perception-studio-app"
mkdir -p "$app"
[ "${STUDIO_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 600 "${OSPREY_BIN:-$repo/target/release/osprey}" \
    "$repo/src/studio" --compile --memory=arc --target=macos -o "$app/libOspreyApp.a" >/dev/null
export OSPREY_NATIVE_APP_DIR="$app" OSPREY_RUNTIME_DIR="$repo/src/toolchain/compiler/lib" \
    OSPREY_NATIVE_ABI="$repo/src/toolchain/compiler/runtime/native_abi.h"
[ "${STUDIO_PREBUILT:-}" = 1 ] || nice -n 10 gtimeout 2400 cargo build --manifest-path "$here/Cargo.toml" --release --features desktop -q
window_size=1152x756
[[ -z "${STUDIO_PROJECT:-}" ]] || window_size=1152x752
export OSPREY_NATIVE_TITLE="Perception Studio" OSPREY_NATIVE_WINDOW="$window_size" OSPREY_NATIVE_CHROME=bare \
    OSPREY_NATIVE_TITLE_BAR=50 OSPREY_NATIVE_APPEARANCE=dark
machine="$repo/src/osprey/packages/signal_flow/signal_flow.machine.json"
binary="${STUDIO_BINARY:-$repo/target/release/ferroui-osprey-native}"
worker="$repo/target/vst-worker/osprey-vst-worker"

install_studio() (
    applications=${STUDIO_APPLICATIONS_DIR:-/Applications}
    mkdir -p "$applications"
    destination="$applications/Perception Studio.app"
    if [[ -e $destination || -L $destination ]]; then
        [[ ! -L $destination && $(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$destination/Contents/Info.plist" 2>/dev/null) == dev.ospreylang.perception-studio ]] || {
            echo "Cannot replace a different app at $destination" >&2
            exit 1
        }
    fi
    stage=$(mktemp -d "$applications/.perception-studio-install.XXXXXX")
    trap 'if [[ -d $stage/previous.app && ! -e $destination ]]; then mv "$stage/previous.app" "$destination"; fi; rm -rf "$stage"' EXIT
    /usr/bin/ditto "$bundle" "$stage/Perception Studio.app"
    packed="$stage/Perception Studio.app/Contents"
    /usr/libexec/PlistBuddy -c 'Add :CFBundleVersion string 1' "$packed/Info.plist"
    /usr/bin/install -m 755 "${STUDIO_RENDERER:-$repo/target/daw/daw}" "$packed/MacOS/daw"
    [[ ! -x $worker ]] || /usr/bin/install -m 755 "$worker" "$packed/MacOS/osprey-vst-worker"
    /usr/bin/install -m 644 "$machine" "$packed/Resources/signal_flow.machine.json"
    /usr/bin/ditto "$(dirname "$machine")/skins/prism" "$packed/Resources/skins/prism"
    /usr/bin/install -m 644 "$repo/src/studio/tracks/codex-version.json" "$packed/Resources/codex-version.json"
    sample=src/studio/tracks/samples/808-kick.wav
    mkdir -p "$packed/Resources/$(dirname "$sample")"
    /usr/bin/install -m 644 "$repo/$sample" "$packed/Resources/$sample"
    /usr/bin/install -m 755 "$here/scripts/PerceptionStudio-launch.sh" "$packed/MacOS/PerceptionStudio"
    [[ ! -d $destination ]] || mv "$destination" "$stage/previous.app"
    mv "$stage/Perception Studio.app" "$destination"
    if [[ $applications == /Applications ]]; then
        /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$destination"
    fi
    echo "Installed $destination"
    if [[ -n ${STUDIO_PROJECT:-} ]]; then
        project=$STUDIO_PROJECT
        [[ $project = /* ]] || project="$PWD/$project"
        [[ -r $project ]] || { echo "Cannot open project: $project" >&2; exit 1; }
        open -n "$destination" --args --project "$project"
    fi
)

# Launch the host inside its bundle so AppKit uses the Perception Studio artwork for the Dock.
if [[ ${1:-} == --finder ]]; then
    binary=$2
    shift 2
else
    bundle="${STUDIO_BUNDLE_DIR:-$app}/Perception Studio.app"
    contents="$bundle/Contents"
    mkdir -p "$contents/MacOS" "$contents/Resources"
    /usr/bin/install -SC -m 755 "$binary" "$contents/MacOS/ferroui-osprey-native"
    /usr/bin/install -SC -m 644 "$here/scripts/PerceptionStudio.plist" "$contents/Info.plist"
    binary="$contents/MacOS/ferroui-osprey-native"
    icon="$contents/Resources/AppIcon.icns"
    logo="$repo/src/osprey/packages/signal_flow/skins/prism/logo.png"
    if [[ ! -s $icon || $logo -nt $icon ]]; then
        bash "$repo/src/toolchain/hosts/swift/app-icon.sh" "$logo" "$icon"
    fi
    # Finder supplies no song or machine arguments. Reuse the normal launch path and its defaults.
    entry=$(mktemp "$contents/MacOS/.PerceptionStudio.XXXXXX")
    printf '#!/bin/bash\ncd %q\nexport STUDIO_PREBUILT=1 STUDIO_PROJECT=%q\nexec /bin/bash %q --finder "$(dirname "$0")/ferroui-osprey-native"\n' \
        "$repo" "$repo/src/studio/tracks/codex-version.json" "$here/scripts/studio.sh" > "$entry"
    chmod 755 "$entry" && mv -f "$entry" "$contents/MacOS/PerceptionStudio"
    if [[ ${1:-} == --bundle ]]; then
        echo "$bundle"
        exit 0
    fi
    if [[ ${1:-} == --install ]]; then
        install_studio
        exit 0
    fi
fi
launch=(--machine "$machine")
[ -x "$worker" ] && launch+=(--vst-worker "$worker")
if [ -n "${STUDIO_PROJECT:-}" ]; then
    project=$STUDIO_PROJECT
    [[ $project = /* ]] || project="$PWD/$project"
    [[ -r $project ]] || { echo "Cannot open project: $project" >&2; exit 1; }
    launch+=(--song "$project" --renderer "$repo/target/daw/daw")
fi
case "${1:-}" in
    --shot)
        OSPREY_NATIVE_TAP="${STUDIO_TAP:-}" "$binary" "${launch[@]}" &
        pid=$!
        trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
        id=$(gtimeout 60 swift "$here/scripts/window-id.swift" "$pid")
        sleep "${STUDIO_SETTLE:-6}"
        screencapture -x -o -l "$id" "$2"
        echo "$2" ;;
    --repaint)
        OSPREY_NATIVE_REPAINT=1 OSPREY_NATIVE_BENCH="$2" \
            exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    --drag)
        OSPREY_NATIVE_DRAG="${STUDIO_DRAG:-131,376}" OSPREY_NATIVE_BENCH="$2" \
            exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    --bench)
        OSPREY_NATIVE_BENCH="$2" exec gtimeout $(( ${2%.*} + 60 )) "$binary" "${launch[@]}" ;;
    *)
        [ -n "${STUDIO_SECONDS:-}" ] || exec "$binary" "${launch[@]}"
        exec gtimeout "$STUDIO_SECONDS" "$binary" "${launch[@]}" ;;
esac
