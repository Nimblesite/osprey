# [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Used by the IR-to-Lean gate,
# with the same emitted module, translator and checker binaries as the list proofs.

check_record_update() {
  local fault dir
  local -a theorems
  prove_module "$out/emitted" OwnershipRecord
  export_and_replay OwnershipRecord OwnershipRecord.compiledRelabelLedger "propext, Quot.sound"
  export_and_replay OwnershipRecord OwnershipRecord.compiledIdentity
  export_and_replay OwnershipRecord OwnershipRecord.compiledFieldTransfer none
  # Check each lost field against the concrete return and the final drop.
  for fault in attached-field skipped-child freed-input attached-name leaked-foe; do
    dir="$out/mutations/$fault"
    case $fault in
      freed-input) theorems=(compiledIdentity) ;;
      attached-name|leaked-foe) theorems=(compiledFieldTake compiledFieldTransfer) ;;
      *) theorems=(compiledRelabel) ;;
    esac
    awk -v fault="$fault" '
      /^define / {take = /@foeName\(/; body = /@relabel\(/; identity = /@keep\$mono[0-9]+\(i8\* %\$p0\)/}
      take && fault == "attached-name" && /store i8\* null, i8\*\* %r[0-9]+$/ {changed++; next}
      take && fault == "leaked-foe" && /call void @osp_ownership_free/ {changed++; next}
      identity && fault == "freed-input" && /ret i8\* %\$p0$/ {print "  call void @osp_ownership_free(i8* %$p0)"; changed=1}
      body && fault == "attached-field" && !changed && /store i8\* null, i8\*\* %r[0-9]+$/ {changed=1; next}
      body && fault == "skipped-child" && /call .*@osp_alloc_tagged_noinit/ {changed=sub(/i64 3585\)/, "i64 3073)")}
      {print}
      END {if (!changed) exit 1}
    ' "$out/emitted.ll" > "$dir.ll"
    reject_module "$fault" OwnershipRecord "${theorems[@]}"
  done
}
