#!/usr/bin/env bash
# Drives the real app window the way a person does, for testing it end to end:
#   app.sh open <project> <env file>   start from no saved conversations
#   app.sh shot <file.png>             capture the app's window
#   app.sh type <words>                type into the focused composer
#   app.sh key return                  press Return
#   app.sh click <x> <y>               click at a point in the window, in points from its top left
# Typing goes through macOS System Events, so it needs Accessibility permission
# for the terminal; capturing needs Screen Recording.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
name="Osprey Agent"
bundle="$repo/hosts/swift/build/$name.app"
helper="$repo/hosts/swift/build/window-id"
activate() { osascript -e "tell application \"$name\" to activate" >/dev/null; sleep 0.3; }
case "$1" in
    open)
        [ -x "$helper" ] || swiftc -O "$here/shot.swift" -o "$helper"
        pkill -f "$bundle/Contents/MacOS" 2>/dev/null || true
        sleep 0.5
        rm -f "$HOME/Library/Application Support/org.ospreylang.agent/chats.json"
        # A key under the app's own support folder raises no privacy prompt, as a file in ~/Documents would.
        support="$HOME/Library/Application Support/org.ospreylang.agent"
        mkdir -p "$support"
        (umask 077; cp "$3" "$support/key.env")
        open "$bundle" --args "$2" "$support/key.env"
        sleep 2.5
        ;;
    shot)
        activate
        screencapture -x -o -l "$("$helper" "$name")" "$2"
        ;;
    type)
        activate
        osascript -e "tell application \"System Events\" to keystroke \"$2\""
        ;;
    key)
        activate
        osascript -e 'tell application "System Events" to key code 36'
        ;;
    click)
        activate
        osascript -e "tell application \"System Events\" to tell process \"$name\"
            set {left, top} to position of window 1
            click at {left + $2, top + $3}
        end tell" >/dev/null
        ;;
    *) echo "Usage: $0 open|shot|type|key|click" >&2; exit 2 ;;
esac
