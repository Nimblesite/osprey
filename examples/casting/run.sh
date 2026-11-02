#!/usr/bin/env bash
# Opens the casting player in the GPUI host. The interface is player.machine.json; the
# application is examples/casting compiled to a C archive, linked with the TLS and
# media server and visualiser runtime ([FFI-TLS-CLIENT], [FFI-FILE-SERVER], [FFI-MEDIA-RENDER]) and OpenSSL.
# With no arguments on a Mac it runs from Casting.app, so the Dock shows its name and icon [CAST-DESIGN-ICON].
#   run.sh --bundle   build and bundle only; prints the bundle's path
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$here/build"
rm -f "$here/build/libosprey_cast_net.a"
ar rcs "$here/build/libosprey_cast_net.a" "$repo/compiler/bin/tls_client_runtime.o" "$repo/compiler/bin/file_server_runtime.o" "$repo/compiler/bin/media_render_runtime.o"
ssl=$(brew --prefix openssl 2>/dev/null || echo /opt/homebrew/opt/openssl)
export OSPREY_GPUI_LINK_SEARCH="$here/build:$ssl/lib"
export OSPREY_GPUI_LINK_LIBS="static=osprey_cast_net,dylib=ssl,dylib=crypto"
export OSPREY_GPUI_APP="$here"
export OSPREY_GPUI_APP_NAME=casting
# The Sherbet canvas is 868 by 1113 mock pixels: 434 by 557 points draws it one to one on Retina.
export OSPREY_GPUI_TITLE=Casting OSPREY_GPUI_CHROME=bare OSPREY_GPUI_WINDOW=434x557 OSPREY_GPUI_LIGHTS=9,8
machine="$here/player.machine.json"
if [ "$(uname -s)" = Darwin ] && { [ $# -eq 0 ] || [ "$1" = --bundle ]; }; then
    "$repo/hosts/gpui/run.sh" --build
    app=$("$repo/hosts/gpui/bundle.sh" "$here/build" Casting dev.ospreylang.casting \
        "$here/designs/logo-icon.png" "$machine" "$here/document-types.plist" | tail -1)
    if [ $# -gt 0 ]; then
        echo "$app"
        exit 0
    fi
    exec "$app/Contents/MacOS/Casting"
fi
exec "$repo/hosts/gpui/run.sh" "$@" --machine "$machine"
