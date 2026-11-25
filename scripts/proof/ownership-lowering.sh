#!/usr/bin/env bash
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Prove the current compiler's emitted specialization.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${OSPREY_OWNERSHIP_PROOF_DIR:-"$root/target/proof/ownership"}
tools="$root/src/toolchain/benchmarks/proof/tools"
source "$root/scripts/proof/replay.sh"
program="$root/src/osprey/tests/regressions/ownership/single_owner.test.osp"
mkdir -p "$out"
"$1" "$program" --llvm --memory=ownership > "$out/ownership.ll"
# The reflections read LLVM 21+ IR, which addresses fields by byte offset;
# clang 18 spells the same fields as struct-typed GEPs. CI pins the proof
# compiler with OSPREY_PROOF_CLANG.
clang=${OSPREY_PROOF_CLANG:-clang}
validate_ir() { "$clang" -Wno-override-module -c -x ir "$1" -o /dev/null; }
validate_ir "$out/ownership.ll"
runtime="$root/src/toolchain/compiler/runtime"
compile_runtime() {
  "$clang" -O1 -fno-inline -fno-discard-value-names -I "$runtime" "${@:3}" \
    -S -emit-llvm "$1" -o "$2"
}
shim=(-include "$runtime/osp_arc_shim.h")
compile_runtime "$runtime/list_runtime.c" "$out/runtime.ll" "${shim[@]}"
compile_runtime "$runtime/map_runtime_hamt.c" "$out/hamt.ll" "${shim[@]}"
compile_runtime "$runtime/map_runtime.c" "$out/map.ll" "${shim[@]}"
compile_runtime "$runtime/string_runtime.c" "$out/strings.ll" "${shim[@]}"
compile_runtime "$runtime/memory_ownership.c" "$out/heap.ll"
compile_runtime "$tools/ownership-pair-walk.c" "$out/pair-walk.ll" -O2 -finline
reflect() {
  node "$root/scripts/proof/ownership-lowering.mjs" "$program" "$1" \
    "$tools/OwnershipLowering.lean.in" "${3:-$out/runtime.ll}" "$out/map.ll" "${4:-$out/strings.ll}" "${5:-$out/heap.ll}" "${6:-$out/pair-walk.ll}" "${7:-$out/hamt.ll}" > "$2"
}
cd "$tools"
reflect "$out/ownership.ll" "$out/OwnershipLowering.lean"
lake build lean4export
lake env lean -DwarningAsError=true --root="$out" -o "$out/OwnershipLowering.olean" "$out/OwnershipLowering.lean"
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
for name in compiledGrow compiledGrowWhen runtimeRootOrigins renderedStringOrigin stringResultOrigins compiledFreeNull compiledPairFields compiledChildDrop compiledWorklistPush compiledPairQueue compiledGrowthBytes compiledPop compiledBodyReclaim compiledPushPop compiledLiveDebit compiledReclaimPrefix compiledLiveCredit compiledAccountingRoundTrip compiledResultTake compiledResultBorrow compiledResultBorrowUses compiledCompatibilityReturns compiledMapMissFrame compiledSlotRetains compiledSlotWrite compiledCollisionRanges compiledCollisionWrite; do
  theorem="OwnershipLowering.$name"
  axioms=none
  if [[ $name == compiledAccountingRoundTrip ]]; then axioms=propext; fi
  LEAN_PATH="$out:$lean_path" "$exporter" OwnershipLowering -- "$theorem" > "$out/$name.ndjson"
  replay "$name.ndjson" "$theorem" "$axioms"
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
  grow_mutation "$fault" "$out/ownership.ll" "$out/mutations/$fault.ll"
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

for fault in skipped-live-count skipped-live-bytes debit-state-field keep-live-header double-credit stale-peak; do
  awk -v fault="$fault" '
    /^define / {debit = /@arc_live_dec\(/; credit = /@arc_live_inc\(/; reclaim = /@arc_free_or_recycle\(/}
    credit && fault == "double-credit" && / = add i64 .*, 1$/ {sub(/1$/, "2")}
    credit && fault == "stale-peak" && /store i64 .*@g_peak_bytes,/ {next}
    debit && fault == "skipped-live-count" && /store i64 .*@g_live,/ {next}
    debit && fault == "skipped-live-bytes" && /store i64 .*@g_live_bytes,/ {next}
    reclaim && fault == "debit-state-field" && / = getelementptr .*i64 12$/ {sub(/12$/, "8")}
    reclaim && fault == "keep-live-header" && /store i32 0, ptr/ {sub(/store i32 0/, "store i32 1")}
    {print}
  ' "$out/heap.ll" > "$out/mutations/$fault.ll"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/$fault.ll"
  case "$fault" in
    skipped-*) reject "$fault" compiledLiveDebit ;;
    double-credit|stale-peak) reject "$fault" compiledLiveCredit ;;
    *) reject "$fault" compiledReclaimPrefix ;;
  esac
done

reject double-credit compiledAccountingRoundTrip
reject skipped-live-bytes compiledAccountingRoundTrip

for fault in attached-message freed-message null-fallback; do
  awk -v fault="$fault" '
    /^define / {inside = /@missing\(/}
    inside && / = select i1 / {
      message = $1
      if (fault == "null-fallback") sub($7, $9 ",")
    }
    inside && /^  store i8\* null, i8\*\* %r[0-9]+$/ {
      if (fault == "attached-message") next
      if (fault == "freed-message") print "  call void @osp_ownership_free(i8* " message ")"
    }
    {print}
  ' "$out/ownership.ll" > "$out/mutations/$fault.ll"
  reflect "$out/mutations/$fault.ll" "$out/mutations/$fault.lean"
  reject "$fault" compiledResultTake
done

for fault in detached-borrow early-parent-free wrong-borrow-fallback leaked-render-buffer unwritten-return; do
  awk -v fault="$fault" '
    /^define / {inside = /@missingKept\(/}
    inside && / = bitcast .* to i8\*$/ {parent = $1}
    inside && fault == "leaked-render-buffer" && /call void @osp_ownership_free/ && !omitted++ {next}
    inside && fault == "unwritten-return" && /call void .*@osp_format_into/ && /, i8\* %r[0-9]+, i8\* %r[0-9]+, i8\* %r[0-9]+\)$/ {next}
    inside && / = getelementptr \{.*i32 0, i32 2$/ {field = $1}
    inside && / = call i64 @osp_strlen\(/ {
      print
      if (fault == "detached-borrow") print "  store i8* null, i8** " field
      if (fault == "early-parent-free") print "  call void @osp_ownership_free(i8* " parent ")"
      next
    }
    inside && fault == "wrong-borrow-fallback" && / = select i1 / {sub($9 "$", substr($7, 1, length($7)-1))}
    {print}
  ' "$out/ownership.ll" > "$out/mutations/$fault.ll"
  reflect "$out/mutations/$fault.ll" "$out/mutations/$fault.lean"
  case "$fault" in
    wrong-borrow-fallback) reject "$fault" compiledResultBorrowUses ;;
    *) reject "$fault" compiledResultBorrow ;;
  esac
done
reject detached-borrow compiledResultBorrowUses

# Recompile each changed hook so Clang recomputes its memory effects.
reject_active_hook() {
  local name=$1 pattern=$2
  sed "$pattern" "$runtime/memory_ownership.c" > "$out/mutations/$name.c"
  compile_runtime "$out/mutations/$name.c" "$out/mutations/$name.ll"
  reflect "$out/ownership.ll" "$out/mutations/$name.lean" "$out/runtime.ll" "$out/strings.ll" "$out/mutations/$name.ll"
  reject "$name" compiledCompatibilityReturns
}
reject_active_hook unique-drop 's/osp_release_unique(void \*body) { (void)body; }/osp_release_unique(void *body) { tracked_drop(body); }/'
reject_active_hook active-finalizer 's/{ (void)finalize; }/{ if (finalize) finalize(NULL); }/'

# A child miss returns no owned replacement. Both mutants reach the theorem.
for fault in release-miss return-parent; do
  action='osp_release(new_child); return NULL;'
  if [[ $fault == return-parent ]]; then action='return node;'; fi
  sed "/if (!\\*shrunk)/{n;s/return NULL;/$action/;}" "$runtime/map_runtime_hamt.c" > "$out/mutations/$fault.c"
  compile_runtime "$out/mutations/$fault.c" "$out/mutations/$fault.ll" "${shim[@]}"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "" "" "" "" "$out/mutations/$fault.ll"
  reject "$fault" compiledMapMissFrame
done

# Recompile the shipped list unit against the changed local trie header.
mkdir -p "$out/mutations/slot"
cp "$runtime/list_runtime.c" "$out/mutations/slot/"
sed 's/if (i != slot) osp_retain/if (i == slot) osp_retain/' "$runtime/list_trie.h" > "$out/mutations/slot/list_trie.h"
compile_runtime "$out/mutations/slot/list_runtime.c" "$out/mutations/retain-overwritten.ll" "${shim[@]}"
reflect "$out/ownership.ll" "$out/mutations/retain-overwritten.lean" "$out/mutations/retain-overwritten.ll"
reject retain-overwritten compiledSlotRetains

awk '/^define / {slot = /@replace_slot\(/} slot {changed += sub(/store i64 %value,/, "store i64 0,")} {print} END {exit changed != 1}' \
  "$out/runtime.ll" > "$out/mutations/zero-slot.ll"
reflect "$out/ownership.ll" "$out/mutations/zero-slot.lean" "$out/mutations/zero-slot.ll"
reject zero-slot compiledSlotWrite

for fault in collision-prefix collision-offset collision-value; do
  awk -v fault="$fault" '
    /^define / {words = /@collision_words\(/}
    words && fault == "collision-prefix" && /@retain_slots\(/ {changed += sub(/i32 noundef %at,/, "i32 noundef %count,")}
    words && fault == "collision-offset" && /getelementptr .*i8, ptr/ {changed += sub(/, i64 8$/, ", i64 16")}
    words && fault == "collision-value" {changed += sub(/store i64 %value,/, "store i64 0,")}
    {print} END {exit changed != 1}' "$out/hamt.ll" > "$out/mutations/$fault.ll"
  reflect "$out/ownership.ll" "$out/mutations/$fault.lean" "" "" "" "" "$out/mutations/$fault.ll"
  theorem=compiledCollisionRanges
  if [[ $fault == collision-value ]]; then theorem=compiledCollisionWrite; fi
  reject "$fault" "$theorem"
done
