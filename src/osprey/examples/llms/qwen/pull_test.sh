#!/bin/sh
# Offline end-to-end test of pull.sh: a local Range-capable server
# (pull_test_server.py) stands in for huggingface.co with a 3 MB random GGUF,
# and the fresh, resumed, corrupt-resume, present and missing-entry cases run
# against it. Everything lives under target/llms/qwen-pull/, removed on exit.
set -eu

HERE=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH='' cd -- "$HERE/../../../../.." && pwd)
WORK="$ROOT/target/llms/qwen-pull"
DATA="$WORK/hub"
GGUF_DIR="$WORK/cache"
LOG="$WORK/server.log"
PORT_FILE="$WORK/port"
ERRFILE="$WORK/pull.err"
QUANT="TESTQ4"
FILE="Qwen3.8-27B-$QUANT.gguf"
SIZE=3145728
REVISION="0000000000000000000000000000000000000000"
REPO="unsloth/Qwen3.8-27B-GGUF"
PASS=0

say() { printf '%s\n' "$*" >&2; }
fail() { say "qwen-pull test: FAIL in $1: $2"; exit 1; }

assert_eq() { # assert_eq <case> <what> <actual> <expected>
  if [ "$3" = "$4" ]; then PASS=$((PASS + 1)); else fail "$1" "$2 (got '$3', want '$4')"; fi
}

assert_cmd() { # assert_cmd <case> <what> <command...>
  name=$1; what=$2; shift 2
  if "$@"; then PASS=$((PASS + 1)); else fail "$name" "$what"; fi
}

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d ' ' -f1
  else sha256sum "$1" | cut -d ' ' -f1
  fi
}

# Run pull.sh against the local hub; leaves STATUS, OUT and ERRFILE for the asserts.
pull() {
  export QWEN_HUB="$HUB" QWEN_REPO="$REPO" QWEN_REVISION="$REVISION" QWEN_QUANT="$1" QWEN_DIR="$GGUF_DIR"
  if OUT=$(sh "$HERE/pull.sh" 2>"$ERRFILE"); then STATUS=0; else STATUS=$?; fi
}

setup() {
  rm -rf "$WORK"
  mkdir -p "$DATA" "$GGUF_DIR"
  head -c "$SIZE" /dev/urandom > "$DATA/$FILE"
  SHA=$(sha256_of "$DATA/$FILE")
  python3 -I -c '
import json, os, sys
name, d, oid = sys.argv[1], sys.argv[2], sys.argv[3]
size = os.path.getsize(os.path.join(d, name))
print(json.dumps([{"path": name, "size": size, "lfs": {"oid": oid, "size": size}}]))' \
    "$FILE" "$DATA" "$SHA" > "$DATA/tree.json"
}

start_server() {
  python3 -I "$HERE/pull_test_server.py" "$DATA" "$PORT_FILE" >"$LOG" 2>&1 &
  SERVER=$!
  trap 'kill "$SERVER" 2>/dev/null || true; wait "$SERVER" 2>/dev/null || true; rm -rf "$WORK"' EXIT
  waited=0
  while [ ! -s "$PORT_FILE" ] && [ "$waited" -lt 100 ]; do sleep 0.1; waited=$((waited + 1)); done
  [ -s "$PORT_FILE" ] || fail start "the server wrote no port to $PORT_FILE"
  HUB="http://127.0.0.1:$(cat "$PORT_FILE")"
}

case_fresh() {
  say "case 1: fresh download"
  pull "$QUANT"
  assert_eq fresh "exit status" "$STATUS" 0
  assert_eq fresh "stdout is the final path" "$OUT" "$GGUF_DIR/$FILE"
  assert_eq fresh "file size" "$(wc -c < "$GGUF_DIR/$FILE" | tr -d ' ')" "$SIZE"
  assert_eq fresh "file digest" "$(sha256_of "$GGUF_DIR/$FILE")" "$SHA"
  assert_cmd fresh "no .part remains" test ! -e "$GGUF_DIR/$FILE.part"
}

case_resume() {
  say "case 2: resume from a 1 MiB .part"
  rm "$GGUF_DIR/$FILE"
  head -c 1048576 "$DATA/$FILE" > "$GGUF_DIR/$FILE.part"
  pull "$QUANT"
  assert_eq resume "exit status" "$STATUS" 0
  assert_eq resume "stdout is the final path" "$OUT" "$GGUF_DIR/$FILE"
  assert_eq resume "file digest" "$(sha256_of "$GGUF_DIR/$FILE")" "$SHA"
  assert_cmd resume "server saw a 1 MiB range request" grep -q "range=bytes=1048576-" "$LOG"
  assert_cmd resume "no .part remains" test ! -e "$GGUF_DIR/$FILE.part"
}

case_corrupt() {
  say "case 3: corrupt .part is kept and reported"
  rm "$GGUF_DIR/$FILE"
  head -c 1048576 /dev/urandom > "$GGUF_DIR/$FILE.part"
  pull "$QUANT"
  actual=$(sha256_of "$GGUF_DIR/$FILE.part")
  assert_eq corrupt "exit status" "$STATUS" 1
  assert_cmd corrupt "message names the .part" grep -q "$GGUF_DIR/$FILE.part" "$ERRFILE"
  assert_cmd corrupt "message has the expected digest" grep -q "$SHA" "$ERRFILE"
  assert_cmd corrupt "message has the actual digest" grep -q "$actual" "$ERRFILE"
  assert_cmd corrupt ".part was not deleted" test -f "$GGUF_DIR/$FILE.part"
}

case_present() {
  say "case 4: a present file is not fetched again"
  cp "$DATA/$FILE" "$GGUF_DIR/$FILE"
  resolves=$(grep -c resolve "$LOG" || true)
  pull "$QUANT"
  assert_eq present "exit status" "$STATUS" 0
  assert_eq present "stdout is the final path" "$OUT" "$GGUF_DIR/$FILE"
  assert_eq present "no resolve request was made" "$(grep -c resolve "$LOG" || true)" "$resolves"
  assert_cmd present "message says it is present" grep -q "is present" "$ERRFILE"
}

case_missing() {
  say "case 5: a quant absent from the tree"
  pull nothing
  assert_eq missing "exit status" "$STATUS" 1
  assert_cmd missing "message names the file" grep -q "Qwen3.8-27B-nothing.gguf" "$ERRFILE"
  assert_cmd missing "message names the revision" grep -q "$REVISION" "$ERRFILE"
}

setup
start_server
case_fresh
case_resume
case_corrupt
case_present
case_missing
printf 'qwen-pull: %d assertions passed\n' "$PASS"
