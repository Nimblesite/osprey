#!/bin/bash
# [PROF-TEST] [PROF-ACTIVATE-ENV] [PROF-CLI-RUN] [PROF-CLI-REPORT]
# End-to-end profiler check (docs/specs/tooling/0028-Profiler.md):
# `osprey <file> --profile` must run the program, write all four exports, and
# print a terminal report attributing samples to the hot Osprey function.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/target/release/osprey"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# `add` and `sub` recurse so each stays a real frame. One-line wrappers are
# inlined into `fib` at -O2, and the optimizer keeps no inlined range a sample
# can land in for them on every host: on Apple Silicon `sub` was named in 0 of
# 6 runs, so the "every defined function is named" assertion below failed on a
# correct symbolizer [PROF-SYMBOLIZE-OFFLINE].
cat > "$TMP/profdemo.osp" <<'EOF'
fn add(a, b) = match b {
    0 => a
    1 => satAdd(a, 1)
    _ => satAdd(add(a, intDiv(b, 2)), satSub(b, intDiv(b, 2)))
}
fn sub(a, b) = match b {
    0 => a
    _ => satSub(sub(a, satSub(b, 1)), 1)
}
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

# ---- SPEC-DERIVED CLI CONFORMANCE ------------------------------------------
# Each assertion names the clause of docs/specs/tooling/0028-Profiler.md it enforces.

# [PROF-CLI-RUN] "`osprey <file> --run --profile` (`--profile` implies `--run`
# when no other mode is given)" and its five numbered post-processing steps.
for f in profdemo.speedscope.json profdemo.cpuprofile profdemo.folded profdemo.profile.json; do
  test -s "$f" || { echo "FAIL [PROF-CLI-RUN]: missing export $f"; exit 1; }
done

# [PROF-CLI-RUN] step 3: "Write `<stem>.folded` — collapsed stacks with the
# fiber as a synthetic root frame (`fiber-1;main;fib`)."
grep -q fib profdemo.folded || { echo 'FAIL: folded stacks missing fib frames'; exit 1; }
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

node --input-type=module <<'EOF'
import assert from 'node:assert/strict';
import fs from 'node:fs';
const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
read('profdemo.cpuprofile');
const speedscope = read('profdemo.speedscope.json');
assert(speedscope.$schema.startsWith('https://www.speedscope.app'), 'speedscope schema tag');
assert(speedscope.profiles.length > 0, 'no per-fiber profiles');
for (const profile of speedscope.profiles) assert.equal(profile.samples.length, profile.weights.length);
const table = speedscope.shared.frames;
assert(table.some(frame => frame.name.includes('fib')), 'fib absent from speedscope frame table');

// [PROF-SYMBOLIZE-OFFLINE] Use the full frame table. The hot list can omit main on stripped hosts.
const named = new Set(table.filter(frame => /\.osp(?:ml)?$/.test(frame.file ?? '')).map(frame => frame.name));
for (const name of ['fib', 'add', 'sub', 'main']) assert(named.has(name), 'Missing Osprey source for ' + name);

const summary = read('profdemo.profile.json');
const hot = summary.hotFunctions;
const fib = hot.find(frame => frame.name === 'fib');
assert(fib && fib.totalPct > 50, '[PROF-COLLECT-UNWIND] Missing or incomplete fib frame chain');
const unnamed = hot.filter(frame => frame.name.startsWith('0x'));
console.log('[PROF-SYMBOLIZE-OFFLINE]', unnamed.length, 'of', hot.length, 'frames unnamed:', unnamed.slice(0, 3).map(frame => frame.name));

// [PROF-CLI-REPORT] The high sampling rate must provide enough samples for the report.
assert(summary.sampleCount >= 100, 'too few samples: ' + summary.sampleCount);
for (const fiber of summary.fibers) {
  assert(fiber.oncpuSamples >= 0 && fiber.oncpuSamples <= fiber.samples, '[PROF-COLLECT-SAMPLER] invalid fiber ' + fiber.id);
}
assert.equal(summary.rateHz, 8000, '[PROF-ACTIVATE-ENV] requested sample rate');
EOF

echo "PROFILER-E2E-OK"
