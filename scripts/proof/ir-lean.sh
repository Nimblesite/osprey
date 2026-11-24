#!/usr/bin/env bash
# [IR-LEAN] Prove theorems about the code the compiler emitted, through the
# IR to Lean framework: the compiler's LLVM IR, LLVM's own importer to MLIR,
# the @osprey/xdsl reader, and the LlvmSubset semantics in Lean. Every theorem
# is replayed in Osprey's proof kernel under each backend, and each broken
# version of the emitted code must lose its theorem.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/ir-lean"
tools="$root/src/toolchain/benchmarks/proof/tools"
source "$root/scripts/proof/replay.sh"
program="$root/src/osprey/tests/regressions/ownership/single_owner.test.osp"
mlir_bin=${MLIR_BIN:-$(dirname "$(command -v mlir-translate || echo /opt/homebrew/opt/llvm/bin/mlir-translate)")}
mkdir -p "$out/emitted" "$out/mutations"
"$1" "$program" --llvm --memory=ownership > "$out/emitted.ll"
"$1" "$root/src/osprey/examples/proof/ir_lean" --compile --quiet --no-warnings -o "$out/ir_lean"
cd "$tools"
lake build LlvmSubset lean4export
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
# One variant of the emitted code, from LLVM IR to a checked Lean module.
translate() {
  local ir=$1 dir=$2
  mkdir -p "$dir"
  clang -Wno-override-module -c -x ir "$ir" -o /dev/null
  "$mlir_bin/mlir-translate" --import-llvm "$ir" | "$mlir_bin/mlir-opt" --mlir-print-op-generic > "$dir/emitted.mlir"
  "$out/ir_lean" "$dir/emitted.mlir" Emitted 'grow$mono' 'growWhen$mono' 'keep$mono' relabel > "$dir/Emitted.lean"
  if grep -q 'IR-LEAN-FAILED\|REFUSED' "$dir/Emitted.lean"; then
    grep 'IR-LEAN-FAILED\|REFUSED' "$dir/Emitted.lean" >&2
    return 1
  fi
  lake env lean -DwarningAsError=true --root="$dir" -o "$dir/Emitted.olean" "$dir/Emitted.lean"
}
prove() { LEAN_PATH="$1:$lean_path" lean -DwarningAsError=true -o "$1/EmittedOwnership.olean" EmittedOwnership.lean; }
translate "$out/emitted.ll" "$out/emitted"
grep -m1 'Translated' "$out/emitted/Emitted.lean"
prove "$out/emitted"
# Osprey's kernel replays each theorem; the axioms are pinned.
export_and_replay() {
  local module=$1 theorem=$2 name=${2##*.}
  LEAN_PATH="$out/emitted:$lean_path" "$exporter" "$module" -- "$theorem" > "$out/emitted/$name.ndjson"
  replay "$name.ndjson" "$theorem" "Quot.sound, propext" "$out/emitted"
}
source "$root/scripts/proof/ownership-record.sh"
check_record_update
export_and_replay LlvmSubset LlvmSubset.run_follows_ledger
for name in growScalar growManaged growWhen; do export_and_replay EmittedOwnership "EmittedOwnership.$name"; done
# Each mutant is valid LLVM IR in the supported subset. It must reach the
# theorem and be refused there by Lean's kernel.
reject() {
  local fault=$1 theorem=$2 dir="$out/mutations/$1"
  translate "$dir.ll" "$dir"
  if prove "$dir" > "$dir/proof.log" 2>&1; then
    echo "accepted $fault" >&2
    return 1
  fi
  grep -q "error: (kernel) declaration type mismatch, 'EmittedOwnership.$theorem'" "$dir/proof.log"
  echo "REJECT $fault at $theorem"
}
for fault in leak double-free early-free stale-phi; do
  awk -v fault="$fault" '
    /^define / {body = /@grow\$mono[0-9]+\(i8\* %\$p0, i64 %\$p1\)/}
    /^define / {conditional = /@growWhen\$mono[0-9]+\(/}
    conditional && fault == "stale-phi" && / = phi i8\*/ {changed += sub(/\[ %r[0-9]+,/, "[ %$p0,")}
    body && fault == "early-free" && / = call i8\* @osprey_list_append_of/ {append = $0; next}
    body && fault != "stale-phi" && /call void @osp_ownership_free/ {
      changed++
      if (fault == "leak") next
      if (fault == "early-free") {print; print append; next}
      print
    }
    {print}
    END {exit changed != 1}
  ' "$out/emitted.ll" > "$out/mutations/$fault.ll"
  theorem=growScalar
  if [[ $fault == stale-phi ]]; then theorem=growWhen; fi
  reject "$fault" "$theorem"
done

