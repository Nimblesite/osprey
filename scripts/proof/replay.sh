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
    local prefix="PASS $theorem | axioms: "
    [[ $result == "$prefix"* ]] || return 1
    local actual=${result#"$prefix"}
    [[ $(axiom_names "$actual") == "$(axiom_names "$axioms")" ]] || return 1
  done
}

# The certificate's traversal order does not change its axiom dependencies.
axiom_names() {
  printf '%s\n' "$1" | tr ',' '\n' | sed 's/^ //' | LC_ALL=C sort
}

# Lean can report a declaration or application mismatch for a broken proof.
# Require its kernel diagnostic to point inside the intended theorem.
kernel_rejected_at() {
  local source=$1 theorem=$2 log=$3
  awk -v theorem="$theorem" -v source="$source" '
    FNR == NR {if ($1 == "theorem" && $2 == theorem) start=FNR
      else if (start && !end && /^(theorem |def |end )/) end=FNR
      next}
    index($0, source ":") == 1 && /: error: \(kernel\)/ {
      split($0, pos, ":"); if (start && pos[2]>=start && (!end || pos[2]<end)) rejected=1}
    END {exit !rejected}' "$source" "$log"
}
