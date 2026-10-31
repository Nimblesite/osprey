#!/usr/bin/env bash
# Opens the casting player in the GPUI host. The interface is player.machine.json; the
# application is examples/casting compiled to a C archive, linked with the TLS and
# one-file media server runtime ([FFI-TLS-CLIENT], [FFI-FILE-SERVER]) and OpenSSL.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$here/build"
rm -f "$here/build/libosprey_cast_net.a"
ar rcs "$here/build/libosprey_cast_net.a" "$repo/compiler/bin/tls_client_runtime.o" "$repo/compiler/bin/file_server_runtime.o"
ssl=$(brew --prefix openssl 2>/dev/null || echo /opt/homebrew/opt/openssl)
export OSPREY_GPUI_LINK_SEARCH="$here/build:$ssl/lib"
export OSPREY_GPUI_LINK_LIBS="static=osprey_cast_net,dylib=ssl,dylib=crypto"
export OSPREY_GPUI_APP="$here"
export OSPREY_GPUI_APP_NAME=casting
exec "$repo/hosts/gpui/run.sh" "$@" --machine "$here/player.machine.json"
