# [PROOF-TRUST] Replay one Lean export with the three Osprey kernels. The
# calling proof target has built the checkers and set root and out.
replay() {
  local file=$1 theorem=$2 axioms=$3 directory=${4:-$out} backend result
  for backend in gc arc wasm; do
    if [[ $backend == wasm ]]; then
      result=$(printf '%s\t%s\n' "$file" "$theorem" | node "$root/scripts/proof/proof-batch-wasm.mjs" "$root/target/proof/openai/check-batch.wasm" "$directory")
    else
      result=$(printf '%s\t%s\n' "$directory/$file" "$theorem" | "$root/target/proof/openai/check-batch.$backend")
    fi
    echo "$backend $result"
    test "$result" = "PASS $theorem | axioms: $axioms"
  done
}
