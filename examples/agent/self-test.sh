#!/usr/bin/env bash
# Osprey Agent's whole UI on the SwiftUI host ([UI-MACHINE-SELF-TEST]). The same coding script runs
# two ways, and the tests the agent was asked to fix are run here, not taken on its word:
#   self-test.sh                   no window on screen, the model a test double, pictures against goldens
#   self-test.sh --live [dotenv]   a real window and real DeepSeek calls, and an LLM judge; spends tokens
#   self-test.sh --update-goldens  writes the pictures of a doubled run as this OS's goldens
# Records go to OSPREY_AGENT_RECORDS, or a new temporary folder.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
name="Osprey Agent"
mode=double
env_file=""
goldens_flag=()
case "${1:-}" in
    --live) mode=live; env_file=${2:-"$repo/.env"} ;;
    --update-goldens) goldens_flag=(--update-goldens) ;;
    "") ;;
    *) echo "Usage: self-test.sh [--live [dotenv] | --update-goldens]" >&2; exit 2 ;;
esac
records=${OSPREY_AGENT_RECORDS:-$(mktemp -d "${TMPDIR:-/tmp}/osprey-agent-self-test.XXXXXX")}
work=$(mktemp -d "${TMPDIR:-/tmp}/osprey-agent-work.XXXXXX")
trap 'rm -rf "$work"' EXIT
echo "records: $records"
export OSPREY_SWIFT_APP="$here" OSPREY_SWIFT_NAME="$name" OSPREY_SWIFT_BUNDLE_ID=org.ospreylang.agent
bundle=$(bash "$repo/hosts/swift/run.sh" --build | tail -1)
host="$bundle/Contents/MacOS/$name"
project="$work/calc-project"
bash "$here/drive/demo-project.sh" "$project" >/dev/null
os_goldens="$here/goldens/macos-$(sw_vers -productVersion | cut -d. -f1)"

if [ "$mode" = live ]; then
    # The key leaves ~/Documents first: the app reading a file there would raise a privacy prompt.
    (umask 077; grep '^DEEPSEEKKEY=' "$env_file" > "$work/key.env")
    "$host" --self-test "$here/self-test/coding.json" "$records/coding" --store "$work/store" --patience 400 "$project" "$work/key.env"
    verdict=$(cd "$project" && python3 test_calc.py 2>&1) || { echo "$verdict"; echo "FAILED: the project's tests do not pass" >&2; exit 1; }
    echo "$verdict" | tail -3
    grep -q median "$project/calc/__init__.py" || { echo "FAILED: median was not added" >&2; exit 1; }
    python3 "$here/drive/judge.py" "$work/key.env" "$records/coding/04-feature.access.txt" "$verdict"
    echo "live self-test passed; pictures are in $records/coding"
    exit 0
fi

printf 'DEEPSEEKKEY=sk-double\n' > "$work/key.env"
"$host" --self-test "$here/self-test/coding.json" "$records/coding" --headless --store "$work/store-coding" --http-double "$here/self-test/doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project" "$work/key.env"
verdict=$(cd "$project" && python3 test_calc.py 2>&1) || { echo "$verdict"; echo "FAILED: the project's tests do not pass" >&2; exit 1; }
grep -q median "$project/calc/__init__.py" || { echo "FAILED: median was not added" >&2; exit 1; }
grep -q 'wrote calc/__init__.py' "$records/coding/requests.jsonl" || { echo "FAILED: the model never heard the write's result" >&2; exit 1; }

mkdir -p "$work/other-project"
"$host" --self-test "$here/self-test/controls.json" "$records/controls" --headless --store "$work/store-controls" --http-double "$here/self-test/controls-doubles.json" --folder-double "$work/other-project" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
"$host" --self-test "$here/self-test/restart.json" "$records/restart" --headless --store "$work/store-controls" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
if grep -q sk-double "$work/store-controls/chats.json"; then echo "FAILED: the key reached the saved conversations" >&2; exit 1; fi
echo "self-test passed"
