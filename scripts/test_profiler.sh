#!/bin/bash
# [PROF-TEST] [PROF-ACTIVATE-ENV] [PROF-CLI-RUN] [PROF-CLI-REPORT]
# End-to-end profiler check (docs/specs/0028-Profiler.md):
# `osprey <file> --profile` must run the program, write all four exports, and
# print a terminal report attributing samples to the hot Osprey function.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/osprey"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/profdemo.osp" <<'EOF'
fn add(a, b) = a + b ?: 0
fn sub(a, b) = a - b ?: 0
fn fib(n) = match n {
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

# [PROF-COLLECT-UNWIND] Self-time must land on the code that is actually running.
# profdemo spends ~100% of its CPU in `fib`/`add`/`sub` — pure integer arithmetic
# that performs no syscall. Any self-time attributed to a Mach routine or to an
# unsymbolized address is therefore a misattributed leaf frame, and self-time is
# the whole point of a profiler. This assertion FAILS today: see the quarantine
# in compiler/runtime/profiler_sampler.c.
python3 - <<'EOF'
import json
summary = json.load(open("profdemo.profile.json"))
total = summary["sampleCount"]
bogus = sum(f["selfSamples"] for f in summary["hotFunctions"]
            if f["kind"] != "user" and (f["name"].startswith("0x")
                                        or f["name"].startswith("task_")
                                        or f["name"].startswith("host_")
                                        or f["name"].startswith("pthread_")
                                        or f["name"].startswith("_platform_")
                                        or f["name"].startswith("__get")))
share = 100.0 * bogus / total
assert share < 5.0, (
    f"{share:.1f}% of self-samples ({bogus}/{total}) land on kernel or "
    "unsymbolized leaves; profdemo makes no syscalls, so the sampled leaf PC "
    "is wrong and every SELF% figure is fiction")
hot = max(summary["hotFunctions"], key=lambda f: f["selfSamples"])
assert hot["kind"] == "user", (
    f"hottest self-time frame is {hot['name']!r} ({hot['selfPct']}%), not Osprey code")
EOF

echo "PROFILER-E2E-OK"
