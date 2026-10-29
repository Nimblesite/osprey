#!/usr/bin/env zsh
# The proof speed gate of [PROOF-SPEED] (spec 0049): for every case under
# benchmarks/proof/<case>/, Osprey's kernel and Lean's check the same theorem on
# the same machine in the same run, and the gate is red when Osprey's wall time
# is not below Lean's. Both wall times and peak memory are appended to
# benchmarks/proof/results.jsonl with the hashes of both inputs.
#
# A case is a directory holding `osprey.toml` (the Osprey program whose run
# checks the theorem) or a `kernel` symlink to one, and `<case>.lean` (the same
# theorem for `lean --run`). M0's case points at examples/proof/kernel.
#
# Lean is a hard requirement: without `lean` there is no baseline and the gate
# cannot be green, so the script exits 2 rather than passing silently.
#
# Usage: run.sh [case-filter]
#   PROOF_RUNS (default 10)  timed runs per kernel; the minimum wall time counts.
set -u

HERE=${0:A:h}
ROOT=${HERE:h:h}
OSP=$ROOT/target/release/osprey
FILTER=${1:-}
RUNS=${PROOF_RUNS:-10}
OUT=$HERE/results.jsonl

have() { command -v "$1" >/dev/null 2>&1 }
have lean || { echo "proof bench: lean is not installed (elan: https://github.com/leanprover/elan); the speed gate has no baseline" >&2; exit 2 }
[[ -x "$OSP" ]] || { echo "proof bench: $OSP missing; run make build first" >&2; exit 2 }

# wall_ms <cmd...> — minimum wall time in milliseconds over RUNS runs.
wall_ms() {
  local best=999999999 start end ms
  for _ in $(seq 1 "$RUNS"); do
    start=$(python3 -c 'import time; print(int(time.monotonic()*1000))')
    "$@" >/dev/null 2>&1
    end=$(python3 -c 'import time; print(int(time.monotonic()*1000))')
    ms=$(( end - start ))
    (( ms < best )) && best=$ms
  done
  echo "$best"
}

# peak_kb <cmd...> — peak resident set in kilobytes.
peak_kb() {
  /usr/bin/time -l "$@" 2>&1 >/dev/null | awk '/maximum resident set size/ { print int($1 / 1024) }'
}

sha() { shasum -a 256 "$1" | cut -d' ' -f1 }

status=0
for dir in "$HERE"/*/; do
  name=${dir:t}
  [[ -n "$FILTER" && "$name" != *"$FILTER"* ]] && continue
  project=$dir/kernel
  [[ -e "$project/osprey.toml" ]] || project=$dir
  lean_file=$dir/$name.lean
  [[ -f "$lean_file" ]] || { echo "$name: no $name.lean" >&2; status=1; continue }
  bin=$dir/.bench-osprey
  "$OSP" "$project" --compile -o "$bin" >/dev/null 2>&1 || { echo "$name: osprey compile failed" >&2; status=1; continue }
  "$bin" | grep -q '^checked: ' || { echo "$name: osprey kernel did not print a checked line" >&2; status=1; continue }
  lean --run "$lean_file" | grep -q '^checked: ' || { echo "$name: lean did not accept the theorem" >&2; status=1; continue }
  osprey_ms=$(wall_ms "$bin")
  lean_ms=$(wall_ms lean --run "$lean_file")
  osprey_kb=$(peak_kb "$bin")
  lean_kb=$(peak_kb lean --run "$lean_file")
  verdict=pass
  (( osprey_ms < lean_ms )) || verdict=fail
  printf '{"case":"%s","osprey_ms":%s,"lean_ms":%s,"osprey_peak_kb":%s,"lean_peak_kb":%s,"lean":"%s","osprey_input":"%s","lean_input":"%s","verdict":"%s","at":"%s"}\n' \
    "$name" "$osprey_ms" "$lean_ms" "$osprey_kb" "$lean_kb" "$(lean --version | tr -d '\n')" \
    "$(sha "$project/main.ospml")" "$(sha "$lean_file")" "$verdict" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$OUT"
  echo "$name: osprey ${osprey_ms}ms vs lean ${lean_ms}ms -> $verdict"
  [[ $verdict == pass ]] || status=1
  rm -f "$bin"
done
exit $status
