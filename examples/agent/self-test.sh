#!/usr/bin/env bash
# Osprey Agent's whole UI on the SwiftUI host ([UI-MACHINE-SELF-TEST]). The same coding script runs
# two ways, and the tests the agent was asked to fix are run here, not taken on its word:
#   self-test.sh                   no window on screen, the model a test double, pictures against goldens
#   self-test.sh --live [dotenv] [provider/model]   a real window and real calls to that model, default
#                                  deepseek/deepseek-chat, and an LLM judge when a DeepSeek key is there; spends tokens
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
    --live) mode=live; env_file=${2:-"$repo/.env"}; live_model=${3:-deepseek/deepseek-chat} ;;
    --update-goldens) goldens_flag=(--update-goldens) ;;
    "") ;;
    *) echo "Usage: self-test.sh [--live [dotenv] | --update-goldens]" >&2; exit 2 ;;
esac
records=${OSPREY_AGENT_RECORDS:-$(mktemp -d "${TMPDIR:-/tmp}/osprey-agent-self-test.XXXXXX")}
# One fixed folder on every machine: its path shows in the pictures, and TMPDIR differs per user.
work=/tmp/osprey-agent-work
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT
echo "records: $records"
export OSPREY_SWIFT_APP="$here" OSPREY_SWIFT_NAME="$name" OSPREY_SWIFT_BUNDLE_ID=org.ospreylang.agent
bundle=$(bash "$repo/hosts/swift/run.sh" --build | tail -1)
host="$bundle/Contents/MacOS/$name"
iconutil -c iconset "$bundle/Contents/Resources/AppIcon.icns" -o "$work/AppIcon.iconset" || { echo "FAILED: the bundle has no icon" >&2; exit 1; }
[ "$(sips -g pixelWidth "$work/AppIcon.iconset/icon_512x512@2x.png" | awk '/pixelWidth/ {print $2}')" = 1024 ] || { echo "FAILED: the icon has no 1024 pixel size" >&2; exit 1; }
project="$work/calc-project"
bash "$here/drive/demo-project.sh" "$project" >/dev/null
os_goldens="$here/goldens/macos-$(sw_vers -productVersion | cut -d. -f1)"

if [ "$mode" = live ]; then
    # The key leaves ~/Documents first: the app reading a file there would raise a privacy prompt.
    (umask 077; grep -E '^(DEEPSEEKKEY|[A-Z_]+_API_KEY)=' "$env_file" > "$work/key.env")
    # A store that opens on the chosen model, with the keys coming from the dotenv file.
    seed_store() { mkdir -p "$1"; printf '{"keys":[],"model":"%s","effort":""}' "$live_model" > "$1/providers.json"; }
    seed_store "$work/store"
    "$host" --self-test "$here/self-test/coding.json" "$records/coding" --store "$work/store" --patience 400 "$project" "$work/key.env"
    verdict=$(python3 "$here/self-test/verify_calc.py" "$project" --tests 2>&1) || { echo "$verdict"; exit 1; }
    echo "$verdict" | tail -5
    if grep -q '^DEEPSEEKKEY=' "$work/key.env"; then python3 "$here/drive/judge.py" "$work/key.env" "$records/coding/04-feature.access.txt" "$verdict"; else echo "no DeepSeek key: the LLM judge did not run"; fi
    # Harder tasks, each checked by acceptance tests the agent never sees.
    for task in "$here"/self-test/tasks/*/; do
        task_name=$(basename "$task")
        echo "task: $task_name"
        cp -R "$task/project" "$work/$task_name"
        seed_store "$work/store-$task_name"
        export MCP_LIVE_WORD="word-$RANDOM$RANDOM"
        if [ -f "$task/mcp.json" ]; then sed -e "s|@SERVER@|$here/self-test/mcp_server.py|" -e "s|@WORD@|$MCP_LIVE_WORD|" "$task/mcp.json" > "$work/store-$task_name/mcp.json"; fi
        "$host" --self-test "$task/task.json" "$records/$task_name" --store "$work/store-$task_name" --patience 400 "$work/$task_name" "$work/key.env"
        python3 "$task/verify.py" "$work/$task_name"
    done
    while IFS= read -r key; do
        if grep -rqF -- "$key" "$records"; then echo "FAILED: a key reached the records" >&2; exit 1; fi
    done < <(sed 's/^[^=]*=//' "$work/key.env")
    echo "live self-test passed; pictures are in $records"
    exit 0
fi

printf 'DEEPSEEKKEY=sk-double\n' > "$work/key.env"
"$host" --self-test "$here/self-test/coding.json" "$records/coding" --headless --store "$work/store-coding" --http-double "$here/self-test/doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project" "$work/key.env"
python3 "$here/self-test/verify_calc.py" "$project"
grep -q 'wrote calc/__init__.py' "$records/coding/requests.jsonl" || { echo "FAILED: the model never heard the write's result" >&2; exit 1; }

"$host" --self-test "$here/self-test/folder-panel.json" "$records/folder-panel" --store "$work/store-folder-panel" "$project"

mkdir -p "$work/other-project"
"$host" --self-test "$here/self-test/controls.json" "$records/controls" --headless --store "$work/store-controls" --http-double "$here/self-test/controls-doubles.json" --folder-double "$work/other-project" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
"$host" --self-test "$here/self-test/restart.json" "$records/restart" --headless --store "$work/store-controls" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
"$host" --self-test "$here/self-test/code-blocks.json" "$records/code-blocks" --headless --store "$work/store-code-blocks" --http-double "$here/self-test/code-blocks-doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
# One selection group across the transcript: drags, Cmd-A, copy and a click through real window events.
"$host" --self-test "$here/self-test/selection.json" "$records/selection" --headless --store "$work/store-selection" --http-double "$here/self-test/selection-doubles.json" "$project"
"$host" --self-test "$here/self-test/settings.json" "$records/settings" --headless --store "$work/store-settings" --http-double "$here/self-test/settings-doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
python3 "$here/self-test/verify_providers.py" "$records/settings/requests.jsonl"
"$host" --self-test "$here/self-test/settings-restart.json" "$records/settings-restart" --headless --store "$work/store-settings" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$project"
[ "$(stat -f %Lp "$work/store-settings/providers.json")" = 600 ] || { echo "FAILED: providers.json is readable by others" >&2; exit 1; }
grep -q sk-glm-test "$work/store-settings/providers.json" || { echo "FAILED: the GLM key was not saved" >&2; exit 1; }
if grep -q sk-kimi-test "$work/store-settings/providers.json"; then echo "FAILED: the removed Kimi key is still saved" >&2; exit 1; fi
if grep -rqE 'sk-glm-test|sk-kimi-test' "$work/store-settings/chats.json" "$records"; then echo "FAILED: a provider key reached the conversations or the records" >&2; exit 1; fi
if grep -rq sk-double "$work/store-controls/chats.json" "$records"; then echo "FAILED: the key reached a saved file or the records" >&2; exit 1; fi
[ -f "$work/other-project/moved.txt" ] || { echo "FAILED: the tool did not write in the chosen folder" >&2; exit 1; }
[ ! -e "$work/calc-project/moved.txt" ] || { echo "FAILED: the tool wrote in the old folder" >&2; exit 1; }
python3 -c 'import json, sys
requests = [json.loads(json.loads(line)["body"]) for line in open(sys.argv[1], encoding="utf-8")]
if not any(any(message.get("role") == "system" and sys.argv[2] in message.get("content", "") for message in request.get("messages", [])) for request in requests):
    raise SystemExit("FAILED: no system message names the chosen folder")' "$records/controls/requests.jsonl" "$work/other-project"

# MCP: real servers over stdio, HTTP and SSE, only the model doubled. Secrets stay in mcp.json.
mcp_servers=()
trap 'rm -rf "$work"; kill "${mcp_servers[@]+"${mcp_servers[@]}"}" 2>/dev/null || true' EXIT
serve_mcp() { # word, flags...; sets mcp_port
    local word=$1 output="$work/mcp-port-$RANDOM"; shift
    MCP_TEST_WORD=$word python3 "$here/self-test/mcp_server.py" --http "$@" > "$output" &
    mcp_servers+=($!)
    until grep -q listening "$output"; do sleep 0.1; done
    mcp_port=$(awk '{print $2}' "$output")
}
serve_mcp web-word --token hdr-secret-token; web_port=$mcp_port
serve_mcp sse-word --sse; sse_port=$mcp_port
mcp_project="$work/mcp-project"
mkdir -p "$mcp_project"
python3 -c 'import json, sys
print(json.dumps({"mcpServers": {"local": {"command": "python3", "args": [sys.argv[1], "--pid-file", sys.argv[2]]}}}))' "$here/self-test/mcp_server.py" "$work/local.pid" > "$mcp_project/.mcp.json"
echo "DEEPSEEKKEY=sk-double" > "$work/key-double.env"
for script in mcp.json mcp-restart.json mcp-doubles.json; do
    python3 -c 'import json, sys
text = open(sys.argv[1], encoding="utf-8").read()
for name, value in zip(("SERVER", "DEMO_PID", "LOCAL_PID", "WEB_PORT", "SSE_PORT"), sys.argv[3:]):
    text = text.replace(f"@{name}@", value)
open(sys.argv[2], "w", encoding="utf-8").write(text)' "$here/self-test/$script" "$work/$script" "$here/self-test/mcp_server.py" "$work/demo.pid" "$work/local.pid" "$web_port" "$sse_port"
done
"$host" --self-test "$work/mcp.json" "$records/mcp" --headless --store "$work/store-mcp" --http-double "$work/mcp-doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$mcp_project" "$work/key-double.env"
python3 "$here/self-test/verify_mcp.py" "$records/mcp/requests.jsonl"
"$host" --self-test "$work/mcp-restart.json" "$records/mcp-restart" --headless --store "$work/store-mcp" --http-double "$work/mcp-doubles.json" --goldens "$os_goldens" "${goldens_flag[@]+"${goldens_flag[@]}"}" "$mcp_project" "$work/key-double.env"
[ "$(stat -f %Lp "$work/store-mcp/mcp.json")" = 600 ] || { echo "FAILED: mcp.json is readable by others" >&2; exit 1; }
grep -q hdr-secret-token "$work/store-mcp/mcp.json" || { echo "FAILED: the header was not saved" >&2; exit 1; }
if grep -q '"demo"' "$work/store-mcp/mcp.json"; then echo "FAILED: the removed server is still saved" >&2; exit 1; fi
if grep -rqE 'hdr-secret-token|env-only-secret' "$work/store-mcp/chats.json" "$work/store-mcp/mcp-state.json" "$records"; then echo "FAILED: an MCP secret reached the conversations, the switches or the records" >&2; exit 1; fi
echo "self-test passed"
