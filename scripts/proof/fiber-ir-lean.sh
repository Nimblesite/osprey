#!/usr/bin/env bash
# [IR-LEAN-ATOMIC] [CONCURRENCY-FIBER-LIFETIME] Prove the holder count of the
# code that runs. The fiber runtime is compiled as it ships, with inlining off
# so `held_hold` and `held_release` stay functions of their own; LLVM's importer
# turns them into MLIR, @osprey/xdsl reads that, and EmittedFiberLifetime.lean
# proves them to be the acquire and release steps of FiberLifetime.lean. Every
# theorem is replayed in Osprey's proof kernel under each backend, and each
# broken version of the compiled code must lose its theorem.
#   fiber-ir-lean.sh <osprey> [cc]
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/fiber-ir-lean"
tools="$root/src/toolchain/benchmarks/proof/tools"
runtime="$root/src/toolchain/compiler/runtime"
source "$root/scripts/proof/replay.sh"
cc=${2:-cc}
mlir_bin=${MLIR_BIN:-$(dirname "$(command -v mlir-translate || echo /opt/homebrew/opt/llvm/bin/mlir-translate)")}
mkdir -p "$out/emitted" "$out/mutations"
"$1" "$root/src/osprey/examples/proof/ir_lean" --compile --quiet --no-warnings -o "$out/ir_lean"
# The two functions, as the shipped flags compile them. Aliasing metadata says
# nothing about what an instruction does and the reader has no use for it.
"$cc" -S -emit-llvm -O2 -fno-inline -std=c11 -D_GNU_SOURCE -I "$runtime" "$runtime/fiber_runtime.c" -o "$out/runtime.ll"
"$mlir_bin/llvm-extract" -S --func=held_hold --func=held_release "$out/runtime.ll" \
  | sed -E 's/, !tbaa ![0-9]+//' > "$out/emitted.ll"
cd "$tools"
lake build LlvmSubset FiberLifetime lean4export
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
# One variant of the compiled code, from LLVM IR to a checked Lean module.
translate() {
  local ir=$1 dir=$2
  mkdir -p "$dir"
  "$mlir_bin/mlir-translate" --import-llvm "$ir" 2> /dev/null | "$mlir_bin/mlir-opt" --mlir-print-op-generic > "$dir/emitted.mlir"
  "$out/ir_lean" "$dir/emitted.mlir" EmittedRuntime > "$dir/EmittedRuntime.lean"
  if grep -q 'IR-LEAN-FAILED\|REFUSED' "$dir/EmittedRuntime.lean"; then
    grep 'IR-LEAN-FAILED\|REFUSED' "$dir/EmittedRuntime.lean" >&2
    return 1
  fi
  lake env lean -DwarningAsError=true --root="$dir" -o "$dir/EmittedRuntime.olean" "$dir/EmittedRuntime.lean"
}
prove_module() { LEAN_PATH="$1:$lean_path" lean -DwarningAsError=true -o "$1/EmittedFiberLifetime.olean" EmittedFiberLifetime.lean; }
translate "$out/emitted.ll" "$out/emitted"
grep -m1 'Translated 2 of 2' "$out/emitted/EmittedRuntime.lean"
grep -c 'atomicrmw' "$out/emitted/EmittedRuntime.lean" | grep -qx 2
prove_module "$out/emitted"
# Osprey's kernel replays each theorem; the axioms are pinned.
export_and_replay() {
  local name=$1 axioms=$2
  LEAN_PATH="$out/emitted:$lean_path" "$exporter" EmittedFiberLifetime -- "EmittedFiberLifetime.$name" > "$out/emitted/$name.ndjson"
  replay "$name.ndjson" "EmittedFiberLifetime.$name" "$axioms" "$out/emitted"
}
for name in hold release_last release_more release_null release_last_is_step; do export_and_replay "$name" "propext"; done
for name in hold_is_step release_more_is_step; do export_and_replay "$name" "propext, Quot.sound"; done
# Each mutant is valid LLVM IR in the supported subset. It must reach its
# theorem and be refused there by Lean's kernel.
reject() {
  local fault=$1 edit=$2 dir="$out/mutations/$1"
  shift 2
  sed -E "$edit" "$out/emitted.ll" > "$dir.ll"
  if cmp -s "$out/emitted.ll" "$dir.ll"; then
    echo "the $fault mutation changed nothing" >&2
    return 1
  fi
  translate "$dir.ll" "$dir"
  if prove_module "$dir" > "$dir/proof.log" 2>&1; then
    echo "accepted $fault" >&2
    return 1
  fi
  local theorem
  for theorem in "$@"; do
    kernel_rejected_at EmittedFiberLifetime.lean "$theorem" "$dir/proof.log" || return 1
    echo "REJECT $fault at $theorem"
  done
}
# A release that counts up never frees; one that frees unless it is the last
# frees under the other holders; a hold that overwrites the count loses them.
reject leak 's/atomicrmw sub /atomicrmw add /' release_last release_more
reject early-free 's/icmp eq i64 (%[0-9]+), 1$/icmp ne i64 \1, 1/' release_last release_more
reject lost-hold 's/atomicrmw add /atomicrmw xchg /' hold
