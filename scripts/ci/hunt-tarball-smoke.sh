#!/usr/bin/env bash
# [SYMEX-INSTALL] Hunt from an unpacked release tarball, never the build tree (#443): under each
# backend, `osprey hunt` must find, replay and reproduce src/osprey/tests/hunt/crash.osp's planted
# abort, linking the symbolic runtime that shipped beside the binary. Needs KLEE (brew install klee).
# Usage: hunt-tarball-smoke.sh <osprey-<version>-<platform>.tar.gz>
set -euo pipefail

tarball=${1:?usage: hunt-tarball-smoke.sh <tarball>}
crash="$(cd "$(dirname "$0")/../.." && pwd)/src/osprey/tests/hunt/crash.osp"
staging=$(basename "$tarball" .tar.gz)
unpacked=$(mktemp -d)
trap 'rm -rf "$unpacked"' EXIT
tar -xzf "$tarball" -C "$unpacked"
cp "$crash" "$unpacked/"
cd "$unpacked"

for memory in gc arc; do
  runtime="libosprey_runtime_klee_$memory.bc"
  for file in "$runtime" libosprey_gc_roots.so; do
    [ -s "$staging/$file" ] || { echo "hunt-tarball-smoke: $staging ships no $file" >&2; exit 1; }
  done
  status=0
  "./$staging/osprey" hunt crash.osp --memory="$memory" --max-time 120 --out "out-$memory" || status=$?
  [ "$status" = 1 ] || { echo "hunt-tarball-smoke: --memory=$memory exited $status, not 1 (a finding)" >&2; exit 1; }
  node -e '
    const [file, runtime] = process.argv.slice(1);
    const report = JSON.parse(require("fs").readFileSync(file, "utf8"));
    const found = report.findings.some((f) => f.class === "abort" && f.line === 4 && f.replay === "reproduced");
    const linked = report.context.engineArguments.some((arg) => arg.startsWith("-link-llvm-lib=") && arg.endsWith(runtime));
    if (!found || !linked) { console.error(JSON.stringify(report, null, 2)); process.exit(1); }
  ' "out-$memory/report.json" "/$staging/$runtime" || { echo "hunt-tarball-smoke: --memory=$memory did not reproduce the abort with the shipped runtime" >&2; exit 1; }
  echo "hunt-tarball-smoke: $memory reproduced crash.osp:4 with $staging/$runtime"
done
