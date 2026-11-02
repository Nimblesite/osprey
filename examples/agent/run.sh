#!/usr/bin/env bash
# Osprey Agent on an Osprey Native host ([NATIVE-APPS-ALL]).
#   run.sh [project folder] [dotenv file]   build the app and open it on the SwiftUI host
#   run.sh --test                           run the suite, which needs no network and no window
# The dotenv file holds DEEPSEEKKEY=...; without one the composer asks for the key.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
compiler=${OSPREY_BIN:-"$repo/target/release/osprey"}
if [ "${1:-}" = --test ]; then
    "$compiler" "$here/test" --run --quiet
    exec "$compiler" "$here/test-app" --run --quiet
fi
project=${1:-$PWD}
env_file=${2:-}
if [ -z "$env_file" ] && [ -f "$repo/.env" ]; then env_file="$repo/.env"; fi
identifier=org.ospreylang.agent
arguments=("$project")
if [ -n "$env_file" ]; then
    # Copied beside the app's own data: reading a file in ~/Documents would raise a privacy prompt.
    support="$HOME/Library/Application Support/$identifier"
    mkdir -p "$support"
    (umask 077; cp "$env_file" "$support/key.env")
    arguments+=("$support/key.env")
fi
export OSPREY_SWIFT_APP="$here" OSPREY_SWIFT_NAME="Osprey Agent" OSPREY_SWIFT_BUNDLE_ID="$identifier"
exec "$repo/hosts/swift/run.sh" -- "${arguments[@]}"
