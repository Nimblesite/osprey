#!/usr/bin/env bash
# The exhaustive manifest of the pinned Mathlib root ([PROOF-COMPLETION-MATHLIB],
# plan 0041 milestone 5), written under target/proof/mathlib/ (never committed):
#   manifest.ndjson  modules with imports, then every declaration Mathlib reaches with
#                    kind, module, hashes, direct dependencies and assumed axioms
#   sources.ndjson   every .lean source of every pinned package with its sha256, and
#                    each package's licence file with its sha256
#   receipt.txt      the pin, the totals line and both files' sha256
set -euo pipefail

export PATH="$HOME/.elan/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROJECT="$ROOT/src/toolchain/benchmarks/proof/mathlib"
OUT="$ROOT/target/proof/mathlib"

fail() { printf 'mathlib-manifest.sh: STOP — %s\n' "$*" >&2; exit 1; }

sources() {
    for package in "$PROJECT"/.lake/packages/*; do
        name="$(basename "$package")"
        for licence in "$package"/LICENSE*; do
            [ -f "$licence" ] || continue
            printf '{"package":"%s","licence":"%s","sha256":"%s"}\n' "$name" "$(basename "$licence")" "$(shasum -a 256 "$licence" | cut -d' ' -f1)"
        done
        (cd "$package" && git ls-files '*.lean' | sort | while read -r file; do
            printf '{"package":"%s","source":"%s","sha256":"%s"}\n' "$name" "$file" "$(shasum -a 256 "$file" | cut -d' ' -f1)"
        done)
    done
}

mkdir -p "$OUT"
cd "$PROJECT"
[ "$(cat lean-toolchain)" = "leanprover/lean4:v4.25.0" ] || fail "expected the pinned Lean 4.25.0 toolchain"
lake resolve-deps >&2
(cd .lake/packages/mathlib && lake exe cache get) >&2
lake build Mathlib manifest >&2
lake exe manifest Mathlib > "$OUT/manifest.ndjson"
sources > "$OUT/sources.ndjson"
{
    printf 'mathlib %s\n' "$(git -C .lake/packages/mathlib rev-parse HEAD)"
    tail -n 1 "$OUT/manifest.ndjson"
    (cd "$OUT" && shasum -a 256 manifest.ndjson sources.ndjson)
} > "$OUT/receipt.txt"
cat "$OUT/receipt.txt"
