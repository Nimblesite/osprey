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

# ---- SPEC-DERIVED CLI CONFORMANCE ------------------------------------------
# Each assertion names the clause of docs/specs/0028-Profiler.md it enforces.

# [PROF-CLI-RUN] "`osprey <file> --run --profile` (`--profile` implies `--run`
# when no other mode is given)" and its five numbered post-processing steps.
for f in profdemo.speedscope.json profdemo.cpuprofile profdemo.folded profdemo.profile.json; do
  test -s "$f" || { echo "FAIL [PROF-CLI-RUN]: missing export $f"; exit 1; }
done

# [PROF-CLI-RUN] step 3: "Write `<stem>.folded` — collapsed stacks with the
# fiber as a synthetic root frame (`fiber-1;main;fib`)."
grep -qE '^(main|fiber-[0-9]+);' profdemo.folded \
  || { echo "FAIL [PROF-CLI-RUN]: folded stacks lack a synthetic fiber root"; exit 1; }

# [PROF-CLI-REPORT] "a top-10 table with columns
# `SELF% TOTAL% SELF TOTAL FUNCTION LOCATION`" and "Sampling does not produce
# call counts, so the report has no calls column."
echo "$out" | grep -q "SELF%" || { echo "FAIL [PROF-CLI-REPORT]: no SELF% column"; exit 1; }
echo "$out" | grep -q "TOTAL%" || { echo "FAIL [PROF-CLI-REPORT]: no TOTAL% column"; exit 1; }
echo "$out" | grep -q "FUNCTION" || { echo "FAIL [PROF-CLI-REPORT]: no FUNCTION column"; exit 1; }
echo "$out" | grep -q "LOCATION" || { echo "FAIL [PROF-CLI-REPORT]: no LOCATION column"; exit 1; }
echo "$out" | grep -qiE '\bCALLS\b' \
  && { echo "FAIL [PROF-CLI-REPORT]: sampling yields no call counts, yet a calls column is printed"; exit 1; }

# [PROF-CLI-REPORT] "a header line (wall, CPU, samples, rate, fibers)".
echo "$out" | grep -qE 'wall.*CPU.*samples.*Hz.*fiber' \
  || { echo "FAIL [PROF-CLI-REPORT]: header line missing a required field"; exit 1; }

python3 - <<'EOF'
import json, shutil, subprocess
summary = json.load(open("profdemo.profile.json"))
total = summary["sampleCount"]
frames = summary["hotFunctions"]

# [PROF-COLLECT-UNWIND] "Frame 0 is the precise PC." Self-time IS frame 0, and
# profdemo spends ~100% of its CPU in fib/add/sub — pure integer arithmetic
# performing no syscall. macOS `sample` on the same workload attributes 549/549
# to Osprey code and nothing to libsystem, so any self-time landing on a kernel
# routine or an unsymbolized address is a misattributed leaf.
KERNELISH = ("task_", "host_", "pthread_", "_platform_", "__get", "mach_")
bogus = sum(f["selfSamples"] for f in frames
            if f["kind"] != "user" and (f["name"].startswith("0x")
                                        or f["name"].startswith(KERNELISH)))
share = 100.0 * bogus / total
assert share < 5.0, (
    f"[PROF-COLLECT-UNWIND] frame 0 is not the precise PC: {share:.1f}% of "
    f"self-samples ({bogus}/{total}) land on kernel or unsymbolized leaves")

# [PROF-COLLECT-UNWIND] the hottest leaf must be the code that is running.
hot = max(frames, key=lambda f: f["selfSamples"])
assert hot["kind"] == "user", (
    f"[PROF-COLLECT-UNWIND] hottest self-time frame is {hot['name']!r} "
    f"({hot['selfPct']}%), not Osprey code")

# [PROF-SYMBOLIZE-OFFLINE] "...falling back to `atos` on macOS and raw hex
# names when no symbolizer is present." A symbolizer IS present here, so raw
# hex names are not an available fallback.
have_symbolizer = bool(shutil.which("llvm-symbolizer") or shutil.which("atos"))
if have_symbolizer:
    hexed = [f["name"] for f in frames if f["name"].startswith("0x")]
    assert not hexed, (
        f"[PROF-SYMBOLIZE-OFFLINE] {len(hexed)} frames came back as raw hex "
        f"({hexed[:3]}) although a symbolizer is installed; raw hex is spec'd "
        "only for when none is present")

# [PROF-CLI-REPORT] "Below about 100 samples, the report flags low confidence."
# This run is far above that, so no low-confidence flag may appear.
assert total >= 100, f"[PROF-TEST] too few samples to assert on: {total}"

# [PROF-COLLECT-SAMPLER] "Samples record (t_ns, thread, stack, state). State is
# on-CPU or waiting" — the per-fiber split the summary reports must cover
# every sample and never exceed it.
for fiber in summary["fibers"]:
    assert 0 <= fiber["oncpuSamples"] <= fiber["samples"], (
        f"[PROF-COLLECT-SAMPLER] fiber {fiber['id']} reports "
        f"{fiber['oncpuSamples']} on-cpu of {fiber['samples']} samples")

# [PROF-ACTIVATE-ENV] "`OSPREY_PROFILE_HZ=<n>` overrides the sampling rate".
assert summary["rateHz"] == 8000, (
    f"[PROF-ACTIVATE-ENV] OSPREY_PROFILE_HZ=8000 was not honoured: "
    f"rateHz={summary['rateHz']}")
EOF

echo "PROFILER-E2E-OK"
