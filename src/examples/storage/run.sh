#!/usr/bin/env bash
# Osprey Storage on an Osprey Native host ([NATIVE-APPS-ALL]).
#   run.sh                 build the app and open it on the GPUI host
#   run.sh --host=winui    build storage.dll and open it in the WinUI host (Windows)
#   run.sh --test          run the suite, which needs no disk and no window
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
case "${1:-}" in
    --test) exec "$compiler" test "$here/test" ;;
    --host=winui)
        mkdir -p "$here/build"
        "$compiler" build "$here" --target=windows-x64 -o "$here/build/storage.dll"
        exec dotnet run --project "$repo/hosts/winui/OspreyHost" -c Release -- "$here/build/storage.dll"
        ;;
    "" | --host=gpui)
        export OSPREY_GPUI_APP="$here"
        exec "$repo/hosts/gpui/run.sh"
        ;;
    *) echo "Usage: $0 [--host=gpui|--host=winui|--test]" >&2; exit 2 ;;
esac
