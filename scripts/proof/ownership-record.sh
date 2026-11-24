# [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Used by the IR-to-Lean gate,
# with the same emitted module, translator and checker binaries as the list proofs.
record() { LEAN_PATH="$1:$lean_path" lean -DwarningAsError=true -o "$1/OwnershipRecord.olean" OwnershipRecord.lean; }

check_record_update() {
  local fault dir theorem
  record "$out/emitted"
  export_and_replay OwnershipRecord OwnershipRecord.compiledRelabelLedger "propext, Quot.sound"
  export_and_replay OwnershipRecord OwnershipRecord.compiledIdentity
  export_and_replay OwnershipRecord OwnershipRecord.compiledFieldTransfer none
  # Check each lost field against the concrete return and the final drop.
  for fault in attached-field skipped-child freed-input attached-name leaked-foe; do
    dir="$out/mutations/$fault"
    theorem=compiledRelabel
    if [[ $fault == freed-input ]]; then theorem=compiledIdentity; fi
    if [[ $fault == attached-name || $fault == leaked-foe ]]; then theorem=compiledFieldTake; fi
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
    translate "$dir.ll" "$dir"
    if record "$dir" > "$dir/proof.log" 2>&1; then echo "accepted $fault" >&2; exit 1; fi
    kernel_rejected_at OwnershipRecord.lean "$theorem" "$dir/proof.log"
    if [[ $theorem == compiledFieldTake ]]; then
      kernel_rejected_at OwnershipRecord.lean compiledFieldTransfer "$dir/proof.log"
    fi
    echo "REJECT $fault at $theorem"
  done
}
