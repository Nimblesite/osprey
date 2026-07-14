#!/bin/bash
# [PROF-TEST] End-to-end profiler check (docs/specs/0028-Profiler.md):
# `osprey <file> --profile` must run the program, write all four exports, and
# print a terminal report attributing samples to the hot Osprey function.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/osprey"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/profdemo.osp" <<'EOF'
fn add(a: int, b: int) -> int = a + b
fn sub(a: int, b: int) -> int = a - b
fn fib(n: int) -> int = match n {
    0 => 0
    1 => 1
    _ => add(fib(sub(n, 1)), fib(sub(n, 2)))
}
let worker = spawn fib(33)
let local = fib(35)
print("${add(local, await(worker))}")
EOF

cd "$TMP"
# Pin a high rate so short/fast machines still collect an order-of-magnitude
# margin over the sampleCount assertion below.
out="$(OSPREY_PROFILE_HZ=8000 "$BIN" profdemo.osp --profile)"

echo "$out" | grep -q "12752043" || { echo "FAIL: program output missing/wrong"; exit 1; }
echo "$out" | grep -q "fib" || { echo "FAIL: report does not attribute samples to fib"; exit 1; }
echo "$out" | grep -q "samples" || { echo "FAIL: report missing sample header"; exit 1; }

for f in profdemo.speedscope.json profdemo.cpuprofile profdemo.profile.json; do
  test -s "$f" || { echo "FAIL: missing export $f"; exit 1; }
  python3 -c "import json; json.load(open('$f'))" || { echo "FAIL: invalid JSON in $f"; exit 1; }
done
test -s profdemo.folded || { echo "FAIL: missing export profdemo.folded"; exit 1; }
grep -q "fib" profdemo.folded || { echo "FAIL: folded stacks missing fib frames"; exit 1; }
python3 - <<'EOF'
import json
s = json.load(open("profdemo.speedscope.json"))
assert s["$schema"].startswith("https://www.speedscope.app"), "speedscope schema tag"
assert s["profiles"], "no per-fiber profiles"
for p in s["profiles"]:
    assert len(p["samples"]) == len(p["weights"]), "samples/weights mismatch"
names = [f["name"] for f in s["shared"]["frames"]]
assert any("fib" in n for n in names), "fib not in speedscope frame table"
summary = json.load(open("profdemo.profile.json"))
assert summary["sampleCount"] > 20, f"too few samples: {summary['sampleCount']}"
assert any(fn["name"] == "fib" for fn in summary["hotFunctions"]), "fib not hot"
EOF

echo "PROFILER-E2E-OK"
