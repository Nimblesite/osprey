#!/usr/bin/env bash
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Prove the current compiler's emitted specialization.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/ownership"
tools="$root/src/toolchain/benchmarks/proof/tools"
program="$root/src/osprey/tests/regressions/ownership/single_owner.test.osp"
mkdir -p "$out"
"$1" "$program" --llvm --memory=ownership > "$out/ownership.ll"
runtime="$root/src/toolchain/compiler/runtime"
compile_runtime() {
  clang -O1 -fno-inline -fno-discard-value-names -I "$runtime" -include "$runtime/osp_arc_shim.h" \
    -S -emit-llvm "$1" -o "$2"
}
compile_runtime "$runtime/list_runtime.c" "$out/runtime.ll"
compile_runtime "$runtime/map_runtime.c" "$out/map.ll"
compile_runtime "$runtime/string_runtime.c" "$out/strings.ll"
reflect() {
  node "$root/scripts/proof/ownership-lowering.mjs" "$program" "$1" \
    "$tools/OwnershipLowering.lean.in" "${3:-$out/runtime.ll}" "$out/map.ll" "${4:-$out/strings.ll}" > "$2"
}
cd "$tools"
reflect "$out/ownership.ll" "$out/OwnershipLowering.lean"
lake build lean4export
lake env lean -DwarningAsError=true --root="$out" -o "$out/OwnershipLowering.olean" "$out/OwnershipLowering.lean"
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
for name in compiledGrow compiledGrowWhen runtimeRootOrigins renderedStringOrigin stringResultOrigins; do
  theorem="OwnershipLowering.$name"
  LEAN_PATH="$out:$lean_path" "$exporter" OwnershipLowering -- "$theorem" > "$out/$name.ndjson"
  for backend in gc arc wasm; do
    if [[ $backend == wasm ]]; then
      result=$(printf '%s.ndjson\t%s\n' "$name" "$theorem" | node "$root/scripts/proof/proof-batch-wasm.mjs" "$root/target/proof/openai/check-batch.wasm" "$out")
    else
      result=$(printf '%s\t%s\n' "$out/$name.ndjson" "$theorem" | "$root/target/proof/openai/check-batch.$backend")
    fi
    echo "$backend $result"
    test "$result" = "PASS $theorem | axioms: none"
  done
done
# The mutants are valid instructions in the supported fragment. Refuse them
# at the theorem itself, independently of reflection or LLVM syntax checks.
mkdir -p "$out/mutations"
reject() {
  local fault=$1 theorem=$2
  local file="$out/mutations/$fault.lean" log="$out/mutations/$fault.log"
  if lake env lean "$file" > "$log" 2>&1; then
    echo "accepted $fault" >&2
    return 1
  fi
  local start end
  start=$(grep -n "^theorem $theorem " "$file" | cut -d: -f1)
  end=$(awk -v start="$start" 'NR > start && /^(def |theorem |end )/ {print NR; exit}' "$file")
  awk -F: -v start="$start" -v end="$end" \
    '/\.lean:[0-9]+:[0-9]+: error:/ && $2 >= start && $2 < end {failed=1} END {exit !failed}' "$log"
  echo "REJECT $fault at $theorem"
}
for fault in leak double-free early-free stale-phi; do
  awk -v fault="$fault" '
    /^define / {body = /@grow\$mono[0-9]+\(i8\* %\$p0, i64 %\$p1\)/}
    /^define / {conditional = /@growWhen\$mono[0-9]+\(/}
    conditional && fault == "stale-phi" && / = phi i8\*/ {sub(/\[ %r[0-9]+,/, "[ %$p0,")}
    body && fault == "early-free" && / = call i8\* @osprey_list_append_of/ {append = $0; next}
    body && fault != "stale-phi" && /call void @osp_ownership_free/ {
      if (fault == "leak") next
      if (fault == "early-free") {print; print append; next}
      print
    }
    {print}
  ' "$out/ownership.ll" > "$out/mutations/$fault.ll"
  reflect "$out/mutations/$fault.ll" "$out/mutations/$fault.lean"
  theorem=compiledGrow
  if [[ $fault == stale-phi ]]; then theorem=compiledGrowWhen; fi
  reject "$fault" "$theorem"
done

# Mutate a real C return and recompile it through the same ownership shim.
# A valid alias return must reach the theorem and fail there.
awk '
  /^OspreyList \*osprey_list_append_of\(/ {inside=1}
  inside && /return alloc_list\(1, 0, 1,/ {print "    return l;"; changed++; inside=0; next}
  {print}
  END {if (changed != 1) exit 1}
' "$runtime/list_runtime.c" > "$out/mutations/alias.c"
compile_runtime "$out/mutations/alias.c" "$out/mutations/alias.ll"
reflect "$out/ownership.ll" "$out/mutations/alias.lean" "$out/mutations/alias.ll"
reject alias runtimeRootOrigins

sed '/^char \*osp_string_dup_internal(/,/^}/s/return out;/return (char *)s;/' \
  "$runtime/string_runtime.c" > "$out/mutations/borrowed-string.c"
compile_runtime "$out/mutations/borrowed-string.c" "$out/mutations/borrowed-string.ll"
reflect "$out/ownership.ll" "$out/mutations/borrowed-string.lean" "$out/runtime.ll" "$out/mutations/borrowed-string.ll"
reject borrowed-string renderedStringOrigin

sed '/^char \*osp_string_concat(/,/^}/s/return out;/return (char *)left;/' \
  "$runtime/string_runtime.c" > "$out/mutations/borrowed-concat.c"
compile_runtime "$out/mutations/borrowed-concat.c" "$out/mutations/borrowed-concat.ll"
reflect "$out/ownership.ll" "$out/mutations/borrowed-concat.lean" "$out/runtime.ll" "$out/mutations/borrowed-concat.ll"
reject borrowed-concat stringResultOrigins
