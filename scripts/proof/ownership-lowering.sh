#!/usr/bin/env bash
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Prove the current compiler's emitted specialization.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/ownership"
tools="$root/src/toolchain/benchmarks/proof/tools"
program="$root/src/osprey/tests/regressions/ownership/single_owner.test.osp"
mkdir -p "$out"
"$1" "$program" --llvm --memory=ownership > "$out/ownership.ll"
validate_ir() { clang -Wno-override-module -c -x ir "$1" -o /dev/null; }
validate_ir "$out/ownership.ll"
runtime="$root/src/toolchain/compiler/runtime"
compile_runtime() {
  clang -O1 -fno-inline -fno-discard-value-names -I "$runtime" "${@:3}" \
    -S -emit-llvm "$1" -o "$2"
}
shim=(-include "$runtime/osp_arc_shim.h")
compile_runtime "$runtime/list_runtime.c" "$out/runtime.ll" "${shim[@]}"
compile_runtime "$runtime/map_runtime.c" "$out/map.ll" "${shim[@]}"
compile_runtime "$runtime/string_runtime.c" "$out/strings.ll" "${shim[@]}"
compile_runtime "$runtime/memory_ownership.c" "$out/heap.ll"
compile_runtime "$tools/ownership-pair-walk.c" "$out/pair-walk.ll" -O2 -finline
reflect() {
  node "$root/scripts/proof/ownership-lowering.mjs" "$program" "$1" \
    "$tools/OwnershipLowering.lean.in" "${3:-$out/runtime.ll}" "$out/map.ll" "${4:-$out/strings.ll}" "${5:-$out/heap.ll}" "${6:-$out/pair-walk.ll}" > "$2"
}
cd "$tools"
reflect "$out/ownership.ll" "$out/OwnershipLowering.lean"
lake build lean4export
lake env lean -DwarningAsError=true --root="$out" -o "$out/OwnershipLowering.olean" "$out/OwnershipLowering.lean"
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
for name in compiledGrow compiledGrowWhen runtimeRootOrigins renderedStringOrigin stringResultOrigins compiledFreeNull compiledPairFields compiledChildDrop compiledWorklistPush compiledPairQueue compiledGrowthBytes compiledPop compiledBodyReclaim compiledPushPop compiledLiveDebit compiledReclaimPrefix; do
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
  validate_ir "$out/mutations/$fault.ll"
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
compile_runtime "$out/mutations/alias.c" "$out/mutations/alias.ll" "${shim[@]}"
reflect "$out/ownership.ll" "$out/mutations/alias.lean" "$out/mutations/alias.ll"
reject alias runtimeRootOrigins

sed '/^char \*osp_string_dup_internal(/,/^}/s/return out;/return (char *)s;/' \
  "$runtime/string_runtime.c" > "$out/mutations/borrowed-string.c"
compile_runtime "$out/mutations/borrowed-string.c" "$out/mutations/borrowed-string.ll" "${shim[@]}"
reflect "$out/ownership.ll" "$out/mutations/borrowed-string.lean" "$out/runtime.ll" "$out/mutations/borrowed-string.ll"
reject borrowed-string renderedStringOrigin

sed '/^char \*osp_string_concat(/,/^}/s/return out;/return (char *)left;/' \
  "$runtime/string_runtime.c" > "$out/mutations/borrowed-concat.c"
compile_runtime "$out/mutations/borrowed-concat.c" "$out/mutations/borrowed-concat.ll" "${shim[@]}"
reflect "$out/ownership.ll" "$out/mutations/borrowed-concat.lean" "$out/runtime.ll" "$out/mutations/borrowed-concat.ll"
reject borrowed-concat stringResultOrigins

cp "$runtime/memory_ownership.c" "$out/mutations/"
sed '/^static void tracked_drop(/,/^}/s/if (!body)/if (body)/' \
  "$runtime/memory_tracked_heap.h" > "$out/mutations/memory_tracked_heap.h"
compile_runtime "$out/mutations/memory_ownership.c" "$out/mutations/inverted-drop.ll"
reflect "$out/ownership.ll" "$out/mutations/inverted-drop.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/inverted-drop.ll"
reject inverted-drop compiledFreeNull

for fault in duplicate-child omitted-child-bit; do
  awk -v fault="$fault" '
    /^define / {pair = /@pairUp\(/}
    pair && fault == "duplicate-child" && /store i8\* %\$p1, i8\*\* %r/ {sub(/%\$p1/, "%$p0")}
    pair && fault == "omitted-child-bit" && /@osp_alloc_tagged_noinit/ {sub(/i64 1537\)/, "i64 513)")}
    {print}
  ' "$out/ownership.ll" > "$out/mutations/$fault.ll"
  reflect "$out/mutations/$fault.ll" "$out/mutations/$fault.lean"
  reject "$fault" compiledPairFields
done

awk '/^define / {walk = /@proof_pair_drop\(/}
     walk && /call .*@arc_drop_child\(/ && ++drops == 2 {next} {print}' \
  "$out/pair-walk.ll" > "$out/mutations/missing-child-drop.ll"
reflect "$out/ownership.ll" "$out/mutations/missing-child-drop.lean" \
  "$out/runtime.ll" "$out/strings.ll" "$out/heap.ll" "$out/mutations/missing-child-drop.ll"
reject missing-child-drop compiledPairFields

for fault in queue-child-twice retain-moving-state; do
  awk -v fault="$fault" '
    /^define / {inside = /@arc_drop_hdr\(/}
    inside && fault == "queue-child-twice" && /call .*@arc_wl_push\(/ {print}
    inside && fault == "retain-moving-state" {sub(/store i32 1,/, "store i32 2,")}
    {print}
  ' "$out/heap.ll" > "$out/mutations/$fault.ll"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/$fault.ll"
  reject "$fault" compiledChildDrop
done

awk '/^define / {inside = /@arc_wl_push\(/}
     inside && / = add i64 / {nextTop = $1}
     inside && / = getelementptr inbounds .*ptr / {sub(/i64 %[^, ]+$/, "i64 " nextTop)}
     {print}' "$out/heap.ll" > "$out/mutations/wrong-worklist-slot.ll"
reflect "$out/ownership.ll" "$out/mutations/wrong-worklist-slot.lean" \
  "$out/runtime.ll" "$out/strings.ll" "$out/mutations/wrong-worklist-slot.ll"
reject wrong-worklist-slot compiledWorklistPush

reject duplicate-child compiledPairQueue

sed '/^define .*@proof_growth_bytes(/,/^}/s/1152921504606846975/1152921504606846974/' \
  "$out/pair-walk.ll" > "$out/mutations/small-growth-limit.ll"
reflect "$out/ownership.ll" "$out/mutations/small-growth-limit.lean" \
  "$out/runtime.ll" "$out/strings.ll" "$out/heap.ll" "$out/mutations/small-growth-limit.ll"
reject small-growth-limit compiledGrowthBytes

for fault in stale-pop duplicate-reclaim; do
  awk -v fault="$fault" '
    /^define / {pop = /@arc_release_zero_locked\(/; drop = /@arc_drop_body\(/}
    pop && / = phi i64 / {depth = $1}
    pop && fault == "stale-pop" && / = getelementptr inbounds / {sub(/i64 %[^, ]+$/, "i64 " depth)}
    drop && fault == "duplicate-reclaim" && /call .*@arc_free_or_recycle\(/ {print}
    {print}
  ' "$out/heap.ll" > "$out/mutations/$fault.ll"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/$fault.ll"
  theorem=compiledPop
  if [[ $fault == duplicate-reclaim ]]; then theorem=compiledBodyReclaim; fi
  reject "$fault" "$theorem"
done

reject stale-pop compiledPushPop

for fault in skipped-live-count skipped-live-bytes debit-state-field keep-live-header; do
  awk -v fault="$fault" '
    /^define / {debit = /@arc_live_dec\(/; reclaim = /@arc_free_or_recycle\(/}
    debit && fault == "skipped-live-count" && /store i64 .*@g_live,/ {next}
    debit && fault == "skipped-live-bytes" && /store i64 .*@g_live_bytes,/ {next}
    reclaim && fault == "debit-state-field" && / = getelementptr .*i64 12$/ {sub(/12$/, "8")}
    reclaim && fault == "keep-live-header" && /store i32 0, ptr/ {sub(/store i32 0/, "store i32 1")}
    {print}
  ' "$out/heap.ll" > "$out/mutations/$fault.ll"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/$fault.ll"
  case "$fault" in
    skipped-*) reject "$fault" compiledLiveDebit ;;
    *) reject "$fault" compiledReclaimPrefix ;;
  esac
done
