# [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Used by the IR-to-Lean gate,
# with the same emitted module, translator and checker binaries as the list proofs.
record() { LEAN_PATH="$1:$lean_path" lean -DwarningAsError=true -o "$1/OwnershipRecord.olean" OwnershipRecord.lean; }

check_record_update() {
  local fault dir
  record "$out/emitted"
  export_and_replay OwnershipRecord OwnershipRecord.compiledRelabelLedger
  # Check each lost field against the concrete return and the final drop.
  for fault in attached-field skipped-child; do
    dir="$out/mutations/$fault"
    awk -v fault="$fault" '
      /^define / {body = /@relabel\(/}
      body && fault == "attached-field" && !changed && /store i8\* null, i8\*\* %r[0-9]+$/ {changed=1; next}
      body && fault == "skipped-child" && /call .*@osp_alloc_tagged_noinit/ {changed=sub(/i64 3585\)/, "i64 3073)")}
      {print}
      END {if (!changed) exit 1}
    ' "$out/emitted.ll" > "$dir.ll"
    translate "$dir.ll" "$dir"
    if record "$dir" > "$dir/proof.log" 2>&1; then echo "accepted $fault" >&2; exit 1; fi
    grep -Fq "(kernel) declaration type mismatch, 'OwnershipRecord.compiledRelabel'" "$dir/proof.log"
    echo "REJECT $fault at compiledRelabel"
  done
}
