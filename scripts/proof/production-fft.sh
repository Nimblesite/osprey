#!/usr/bin/env bash
# [SYNTH-DSP-PROOFS] Derive both production FFT theorems from this checkout.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/production-fft"
tools="$root/src/toolchain/benchmarks/proof/tools"
source="$root/src/osprey/packages/synth/src/ffts.ospml"
checker="$root/target/proof/openai/check-batch.arc"
mkdir -p "$out"
reflect() {
    cargo run --quiet --manifest-path "$root/src/toolchain/Cargo.toml" -p osprey-syntax --example synth_proof -- "$1" > "$out/bodies"
    awk -v body="$out/bodies" '/^-- INSERT PRODUCTION BODIES$/ {while ((getline line < body) > 0) print line; next} {print}' \
        "$tools/ProductionFft.lean.in" > "$out/ProductionFft.lean"
}
cd "$tools"
reflect "$source"
lake env lean --root="$out" -o "$out/ProductionFft.olean" "$out/ProductionFft.lean"
lean_path=$(lake env printenv LEAN_PATH)
exporter=$(lake env which lean4export)
for theorem in combinedCorrect quartetCorrect conjugateComponents conjugateTwice combinedDftBins; do
    name="ProductionFft.$theorem"
    LEAN_PATH="$out:$lean_path" "$exporter" ProductionFft -- "$name" > "$out/$theorem.ndjson"
    result=$(printf '%s\t%s\n' "$out/$theorem.ndjson" "$name" | "$checker")
    echo "$result"
    test "$result" = "PASS $name | axioms: none"
done

# Feed real source edits through the same parser. A syntax/translation error
# is an infrastructure failure; only rejection at the named theorem counts.
reject_edit() {
    local theorem=$1 rewrite=$2
    sed "$rewrite" "$source" > "$out/mutant.ospml"
    if cmp -s "$source" "$out/mutant.ospml"; then
        echo "mutation did not change ffts.ospml" >&2
        return 1
    fi
    reflect "$out/mutant.ospml"
    if lake env lean --root="$out" "$out/ProductionFft.lean" > "$out/rejected.log" 2>&1; then
        echo "accepted broken production $theorem" >&2
        return 1
    fi
    local start end
    start=$(grep -n "^theorem $theorem " "$out/ProductionFft.lean" | cut -d: -f1)
    end=$(awk -v start="$start" 'NR > start && /^(theorem |end )/ {print NR; exit}' "$out/ProductionFft.lean")
    awk -F: -v start="$start" -v end="$end" '/ProductionFft.lean:[0-9]+:[0-9]+: error:/ && $2 >= start && $2 < end { rejected=1 } END {exit !rejected}' "$out/rejected.log"
    echo "REJECT $theorem production mutation"
}
reject_edit combinedCorrect 's/listPrepend rest.upper (subtracted (e, t))/listPrepend rest.upper (added (e, t))/'
reject_edit quartetCorrect 's/added (upper, difference), subtracted (lower, sum)/subtracted (lower, sum), added (upper, difference)/'
reject_edit conjugateComponents 's/im = 0.0 - value.im/im = value.im/'
reflect "$source"
