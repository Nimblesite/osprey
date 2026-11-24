#!/usr/bin/env bash
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Prove the current compiler's emitted specialization.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/ownership"
tools="$root/src/toolchain/benchmarks/proof/tools"
program="$root/src/osprey/tests/regressions/ownership/single_owner.test.osp"
theorem=OwnershipLowering.compiledGrow
mkdir -p "$out"
"$1" "$program" --llvm --memory=ownership > "$out/ownership.ll"
reflect() {
  node "$root/scripts/proof/ownership-lowering.mjs" "$program" "$1" \
    "$tools/OwnershipLowering.lean.in" > "$2"
}
cd "$tools"
reflect "$out/ownership.ll" "$out/OwnershipLowering.lean"
lake build lean4export
lake env lean -DwarningAsError=true --root="$out" -o "$out/OwnershipLowering.olean" "$out/OwnershipLowering.lean"
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
LEAN_PATH="$out:$lean_path" "$exporter" OwnershipLowering -- "$theorem" > "$out/compiledGrow.ndjson"
for backend in gc arc wasm; do
  if [[ $backend == wasm ]]; then
    result=$(printf 'compiledGrow.ndjson\t%s\n' "$theorem" | node "$root/scripts/proof/proof-batch-wasm.mjs" "$root/target/proof/openai/check-batch.wasm" "$out")
  else
    result=$(printf '%s\t%s\n' "$out/compiledGrow.ndjson" "$theorem" | "$root/target/proof/openai/check-batch.$backend")
  fi
  echo "$backend $result"
  test "$result" = "PASS $theorem | axioms: none"
done
# The mutants are valid instructions in the supported fragment. Refuse them
# at the theorem itself, independently of reflection or LLVM syntax checks.
mkdir -p "$out/mutations"
for fault in leak double-free early-free; do
  awk -v fault="$fault" '
    /^define / {body = /@grow\$mono[0-9]+\(i8\* %\$p0, i64 %\$p1\)/}
    body && fault == "early-free" && / = call i8\* @osprey_list_append_of/ {append = $0; next}
    body && /call void @osp_ownership_free/ {
      if (fault == "leak") next
      if (fault == "early-free") {print; print append; next}
      print
    }
    {print}
  ' "$out/ownership.ll" > "$out/mutations/$fault.ll"
  reflect "$out/mutations/$fault.ll" "$out/mutations/$fault.lean"
  if lake env lean "$out/mutations/$fault.lean" > "$out/mutations/$fault.log" 2>&1; then
    echo "accepted $fault" >&2
    exit 1
  fi
  grep -q "Tactic .rfl. failed" "$out/mutations/$fault.log"
  echo "REJECT $fault"
done
